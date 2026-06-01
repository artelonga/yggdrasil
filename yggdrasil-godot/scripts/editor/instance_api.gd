extends Node
## Cliente REST do editor de universos data-driven (YG-73).
##
## Consome o mesmo contrato host-neutral que o player web: instâncias, EditOps
## via PATCH, anexos e templates. Envia `Authorization: Bearer <token>` quando
## [member token] está definido (lido do mesmo JWT que o resto do cliente).

const DEFAULT_URL := "http://127.0.0.1:3030/api/v1"

var base_url: String = DEFAULT_URL
var token: String = ""


func _headers() -> PackedStringArray:
	var h := PackedStringArray(["Content-Type: application/json"])
	if token != "":
		h.append("Authorization: Bearer %s" % token)
	return h


# ---- Endpoints de instância ----

func list_mine() -> Array:
	var v: Variant = await _request("/instances", HTTPClient.METHOD_GET)
	return v if v is Array else []


func create_from_template(template_slug: String, title: String = "") -> Dictionary:
	var path := "/instances?template=%s" % template_slug
	if title != "":
		path += "&title=%s" % title.uri_encode()
	var v: Variant = await _request(path, HTTPClient.METHOD_POST)
	return v if v is Dictionary else {}


func load_instance(id: String) -> Dictionary:
	var v: Variant = await _request("/instances/%s" % id, HTTPClient.METHOD_GET)
	return v if v is Dictionary else {}


## Aplica um EditOp (ver `yggdrasil_core::instance::EditOp`). Devolve a instância
## atualizada, ou `{}` em erro de validação.
func patch(id: String, op: Dictionary) -> Dictionary:
	var v: Variant = await _request("/instances/%s" % id, HTTPClient.METHOD_PATCH, op)
	return v if v is Dictionary else {}


func delete_instance(id: String) -> bool:
	return await _request_status("/instances/%s" % id, HTTPClient.METHOD_DELETE) == 204


func list_templates() -> Array:
	var v: Variant = await _request("/templates", HTTPClient.METHOD_GET)
	return v if v is Array else []


## Detalhe de um template; devolve só o array `palette` (block_types ofertados).
func load_template_palette(slug: String) -> Array:
	var v: Variant = await _request("/templates/%s" % slug, HTTPClient.METHOD_GET)
	if v is Dictionary and v.has("palette"):
		return v["palette"]
	return []


# ---- HTTP ----

func _request(path: String, method: int, body: Variant = null) -> Variant:
	var http := HTTPRequest.new()
	add_child(http)
	var body_str := "" if body == null else JSON.stringify(body)
	http.request(base_url + path, _headers(), method, body_str)
	var result: Array = await http.request_completed
	http.queue_free()
	if result[0] != HTTPRequest.RESULT_SUCCESS:
		return null
	var code: int = result[1]
	if code < 200 or code >= 300:
		push_warning("instance_api %s %s → HTTP %d" % [method, path, code])
		return null
	var bytes: PackedByteArray = result[3]
	if bytes.is_empty():
		return null
	return JSON.parse_string(bytes.get_string_from_utf8())


func _request_status(path: String, method: int) -> int:
	var http := HTTPRequest.new()
	add_child(http)
	http.request(base_url + path, _headers(), method)
	var result: Array = await http.request_completed
	http.queue_free()
	if result[0] != HTTPRequest.RESULT_SUCCESS:
		return 0
	return result[1]
