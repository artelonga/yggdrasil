extends Node

# yggdrasil-web default port
const DEFAULT_URL := "http://127.0.0.1:3030/api/v1"

var base_url: String = DEFAULT_URL
var token: String = ""
var user_id: String = ""
var is_connected: bool = false

signal connection_changed(connected: bool)


func _ready() -> void:
	# Restaura sessão: $GODOT_JWT (dev/headless) tem prioridade; senão o token
	# persistido pelo SaveManager (carregado antes deste autoload).
	var env_token := OS.get_environment("GODOT_JWT")
	if env_token != "":
		_apply_token(env_token, false)
	elif SaveManager.auth_token != "":
		_apply_token(SaveManager.auth_token, false)


# ---- Auth (magic-link, espelha /api/v1/auth/{code,verify}) ----

## Solicita o código de acesso para `email`. Retorna true em 2xx.
func request_login_code(email: String) -> bool:
	var resp := await _http_post_status("/auth/code", {"email": email})
	return resp >= 200 and resp < 300


## Verifica `code` para `email`; em sucesso, guarda e persiste o JWT.
## Retorna true se autenticou.
func verify_login(email: String, code: String) -> bool:
	var resp := await _http_post("/auth/verify", {"email": email, "code": code})
	var tok: String = resp.get("token", "")
	if tok == "":
		return false
	_apply_token(tok, true)
	return true


func logout() -> void:
	token = ""
	user_id = ""
	is_connected = false
	SaveManager.set_auth_token("")
	connection_changed.emit(false)


func _apply_token(tok: String, persist: bool) -> void:
	token = tok
	user_id = _decode_sub(tok)
	is_connected = true
	if persist:
		SaveManager.set_auth_token(tok)
	connection_changed.emit(true)


func _decode_sub(jwt: String) -> String:
	var parts := jwt.split(".")
	if parts.size() < 2:
		return ""
	var payload := parts[1]
	# base64url → base64 com padding
	payload = payload.replace("-", "+").replace("_", "/")
	while payload.length() % 4 != 0:
		payload += "="
	var bytes := Marshalls.base64_to_raw(payload)
	var parsed: Variant = JSON.parse_string(bytes.get_string_from_utf8())
	if parsed is Dictionary:
		return parsed.get("sub", "")
	return ""


func start_game(game_name: String) -> Dictionary:
	return await _http_get("/games/%s/start" % game_name)


func send_game_input(game_name: String, session_id: String, direction: String) -> Dictionary:
	return await _http_post("/games/%s/%s/input" % [game_name, session_id], {
		"direction": direction,
		"user_id": user_id,
	}, false)


# ---- HTTP helpers ----
# Renomeados de _get/_post: `_get` colide com o virtual `Object._get()` e vira
# erro de parse no Godot 4.5 (detectado por scripts/godot.sh check).

func _http_get(path: String) -> Dictionary:
	var http := HTTPRequest.new()
	add_child(http)
	var url := base_url + path
	var headers := PackedStringArray(["Content-Type: application/json"])
	http.request(url, headers, HTTPClient.METHOD_GET)
	var result: Array = await http.request_completed
	http.queue_free()
	return _parse_response(result)


func _http_post(path: String, body: Dictionary, _auth: bool = false) -> Dictionary:
	var http := HTTPRequest.new()
	add_child(http)
	var url := base_url + path
	var headers := PackedStringArray(["Content-Type: application/json"])
	var body_str := JSON.stringify(body)
	http.request(url, headers, HTTPClient.METHOD_POST, body_str)
	var result: Array = await http.request_completed
	http.queue_free()
	return _parse_response(result)


func _http_post_status(path: String, body: Dictionary) -> int:
	var http := HTTPRequest.new()
	add_child(http)
	var headers := PackedStringArray(["Content-Type: application/json"])
	http.request(base_url + path, headers, HTTPClient.METHOD_POST, JSON.stringify(body))
	var result: Array = await http.request_completed
	http.queue_free()
	if result[0] != HTTPRequest.RESULT_SUCCESS:
		return 0
	return result[1]


func _parse_response(result: Array) -> Dictionary:
	if result[0] != HTTPRequest.RESULT_SUCCESS:
		return {}
	var response_code: int = result[1]
	if response_code < 200 or response_code >= 300:
		return {}
	var body_bytes: PackedByteArray = result[3]
	if body_bytes.is_empty():
		return {}
	var body_str := body_bytes.get_string_from_utf8()
	var parsed = JSON.parse_string(body_str)
	if parsed is Dictionary:
		return parsed
	return {}
