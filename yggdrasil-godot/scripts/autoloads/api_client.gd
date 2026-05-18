extends Node

# yggdrasil-web default port
const DEFAULT_URL := "http://127.0.0.1:3030/api/v1"

var base_url: String = DEFAULT_URL
var token: String = ""
var user_id: String = ""
var is_connected: bool = false

signal connection_changed(connected: bool)


func start_game(game_name: String) -> Dictionary:
	return await _get("/games/%s/start" % game_name)


func send_game_input(game_name: String, session_id: String, direction: String) -> Dictionary:
	return await _post("/games/%s/%s/input" % [game_name, session_id], {
		"direction": direction,
		"user_id": user_id,
	}, false)


# ---- HTTP helpers ----

func _get(path: String) -> Dictionary:
	var http := HTTPRequest.new()
	add_child(http)
	var url := base_url + path
	var headers := PackedStringArray(["Content-Type: application/json"])
	http.request(url, headers, HTTPClient.METHOD_GET)
	var result: Array = await http.request_completed
	http.queue_free()
	return _parse_response(result)


func _post(path: String, body: Dictionary, _auth: bool = false) -> Dictionary:
	var http := HTTPRequest.new()
	add_child(http)
	var url := base_url + path
	var headers := PackedStringArray(["Content-Type: application/json"])
	var body_str := JSON.stringify(body)
	http.request(url, headers, HTTPClient.METHOD_POST, body_str)
	var result: Array = await http.request_completed
	http.queue_free()
	return _parse_response(result)


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
