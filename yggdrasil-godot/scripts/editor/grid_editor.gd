extends Control
## Editor de grade estilo Sims/Paralives (YG-73), surface Godot.
##
## Coloca blocos numa grade 2D editável, alterna transparência por camada e
## conecta landmarks — tudo persistido via os mesmos EditOps REST que o player
## web. A UI é construída em código para manter a `.tscn` mínima.
##
## Coexiste com a interface de jogo atual: é uma cena à parte, não toca os
## adaptadores de arcade.

const InstanceApi := preload("res://scripts/editor/instance_api.gd")

const CELL := 24
const GRID_ORIGIN := Vector2(24, 64)

var api: InstanceApi
var client: Node            # autoload ApiClient (login + token persistido)
var instance: Dictionary = {}
var selected_type: String = ""
var selected_block: String = ""
var connect_from: String = ""
var palette: Array = []

var _grid_rect: Control
var _status: Label
var _login_panel: Control


func _ready() -> void:
	api = InstanceApi.new()
	add_child(api)
	if has_node("/root/ApiClient"):
		client = get_node("/root/ApiClient")

	_build_ui()

	# Autenticado (token de env/persistido)? entra direto. Senão, pede login.
	if client and client.token != "":
		api.token = client.token
		await _boot()
	else:
		_show_login()


# ---- Login (magic-link, via autoload ApiClient) ----

func _show_login() -> void:
	if client == null:
		_set_status("ApiClient indisponível — rode dentro do cliente Godot.")
		return
	_login_panel = VBoxContainer.new()
	_login_panel.position = Vector2(24, 80)
	_login_panel.add_theme_constant_override("separation", 8)
	add_child(_login_panel)

	var hint := Label.new()
	hint.text = "Entre para editar (o código vai para o log/SQLite do servidor):"
	_login_panel.add_child(hint)

	var email := LineEdit.new()
	email.placeholder_text = "email"
	email.custom_minimum_size = Vector2(260, 0)
	_login_panel.add_child(email)

	var send_btn := Button.new()
	send_btn.text = "Enviar código"
	_login_panel.add_child(send_btn)

	var code := LineEdit.new()
	code.placeholder_text = "código (6 dígitos)"
	code.editable = false
	_login_panel.add_child(code)

	var enter_btn := Button.new()
	enter_btn.text = "Entrar"
	enter_btn.disabled = true
	_login_panel.add_child(enter_btn)

	send_btn.pressed.connect(func() -> void:
		if email.text.strip_edges() == "":
			_set_status("Informe um email."); return
		send_btn.disabled = true
		var ok: bool = await client.request_login_code(email.text.strip_edges())
		if ok:
			code.editable = true
			enter_btn.disabled = false
			_set_status("Código enviado — confira o log do servidor.")
		else:
			send_btn.disabled = false
			_set_status("Falha ao solicitar código.")
	)

	enter_btn.pressed.connect(func() -> void:
		enter_btn.disabled = true
		var ok: bool = await client.verify_login(email.text.strip_edges(), code.text.strip_edges())
		if ok:
			api.token = client.token
			_login_panel.queue_free()
			_login_panel = null
			await _boot()
		else:
			enter_btn.disabled = false
			_set_status("Código inválido ou expirado.")
	)


func _boot() -> void:
	# cria (ou poderia listar) uma instância neuroanatomia para editar
	palette = await api.list_templates()
	instance = await api.create_from_template("neuroanatomia", "Meu universo")
	if instance.is_empty():
		_set_status("Falha ao criar instância — autentique-se primeiro.")
		return
	var tpl := await api.load_template_palette("neuroanatomia")
	if tpl.size() > 0:
		palette = tpl
		selected_type = palette[0].get("block_type", "landmark")
	_set_status("Editando: %s" % instance.get("title", "?"))
	_grid_rect.queue_redraw()


func _build_ui() -> void:
	set_anchors_preset(Control.PRESET_FULL_RECT)

	var title := Label.new()
	title.text = "Editor de Universos"
	title.position = Vector2(24, 16)
	add_child(title)

	_status = Label.new()
	_status.position = Vector2(24, 36)
	add_child(_status)

	_grid_rect = Control.new()
	_grid_rect.set_anchors_preset(Control.PRESET_FULL_RECT)
	_grid_rect.draw.connect(_draw_grid)
	_grid_rect.gui_input.connect(_on_grid_input)
	_grid_rect.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(_grid_rect)

	# botão de transparência do SNC (toggle de opacidade da camada)
	var snc_toggle := Button.new()
	snc_toggle.text = "SNC: transparência"
	snc_toggle.anchor_left = 1.0
	snc_toggle.anchor_right = 1.0
	snc_toggle.position = Vector2(-220, 16)
	snc_toggle.pressed.connect(_toggle_snc)
	add_child(snc_toggle)


func _set_status(s: String) -> void:
	if _status:
		_status.text = s


# ---- Render ----

func _cell_to_screen(x: int, y: int) -> Vector2:
	return GRID_ORIGIN + Vector2(x * CELL, y * CELL)


func _screen_to_cell(p: Vector2) -> Vector2i:
	var local := p - GRID_ORIGIN
	return Vector2i(int(local.x / CELL), int(local.y / CELL))


func _find_block(id: String) -> Dictionary:
	for layer in instance.get("layers", []):
		for b in layer.get("blocks", []):
			if b.get("id", "") == id:
				return b
	return {}


func _draw_grid() -> void:
	if instance.is_empty():
		return
	var grid: Dictionary = instance.get("grid", {})
	var w: int = grid.get("width", 24)
	var h: int = grid.get("height", 24)

	# linhas da grade
	for x in range(w + 1):
		var sx := GRID_ORIGIN.x + x * CELL
		_grid_rect.draw_line(Vector2(sx, GRID_ORIGIN.y), Vector2(sx, GRID_ORIGIN.y + h * CELL), Color(0.1, 0.1, 0.16), 1.0)
	for y in range(h + 1):
		var sy := GRID_ORIGIN.y + y * CELL
		_grid_rect.draw_line(Vector2(GRID_ORIGIN.x, sy), Vector2(GRID_ORIGIN.x + w * CELL, sy), Color(0.1, 0.1, 0.16), 1.0)

	# conexões
	for conn in instance.get("connections", []):
		var a := _find_block(conn.get("from", ""))
		var b := _find_block(conn.get("to", ""))
		if a.is_empty() or b.is_empty():
			continue
		var pa := _cell_to_screen(a.pos.x, a.pos.y) + Vector2(CELL, CELL) * 0.5
		var pb := _cell_to_screen(b.pos.x, b.pos.y) + Vector2(CELL, CELL) * 0.5
		_grid_rect.draw_line(pa, pb, Color(0.49, 0.78, 0.89, 0.7), 2.0)

	# blocos
	for layer in instance.get("layers", []):
		if layer.get("kind", "") == "background":
			continue
		for blk in layer.get("blocks", []):
			var center := _cell_to_screen(blk.pos.x, blk.pos.y) + Vector2(CELL, CELL) * 0.5
			var col := Color(0.49, 0.78, 0.89)
			if blk.get("id", "") == selected_block:
				col = Color.WHITE
			_grid_rect.draw_circle(center, CELL * 0.4, Color(col, 0.25))
			_grid_rect.draw_arc(center, CELL * 0.4, 0, TAU, 24, col, 2.0)


# ---- Interação ----

func _on_grid_input(event: InputEvent) -> void:
	if not (event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT):
		return
	var cell := _screen_to_cell(event.position)
	var hit := _block_at(cell)

	if hit != "":
		if connect_from != "" and connect_from != hit:
			await _add_connection(connect_from, hit)
			connect_from = ""
		else:
			selected_block = hit
		_grid_rect.queue_redraw()
		return

	# célula vazia → coloca bloco do tipo selecionado
	if selected_type != "":
		await _place_block(cell)


func _block_at(cell: Vector2i) -> String:
	for layer in instance.get("layers", []):
		for b in layer.get("blocks", []):
			if int(b.pos.x) == cell.x and int(b.pos.y) == cell.y:
				return b.get("id", "")
	return ""


func _blocks_layer() -> String:
	for layer in instance.get("layers", []):
		if layer.get("kind", "") == "blocks":
			return layer.get("id", "")
	return "base"


func _place_block(cell: Vector2i) -> void:
	var id := "%s-%d" % [selected_type, Time.get_ticks_msec()]
	var op := {
		"op": "place_block",
		"layer": _blocks_layer(),
		"block": {
			"id": id,
			"block_type": selected_type,
			"pos": {"x": cell.x, "y": cell.y},
		},
	}
	var updated := await api.patch(instance.id, op)
	if not updated.is_empty():
		instance = updated
		_grid_rect.queue_redraw()


func _add_connection(from_id: String, to_id: String) -> void:
	var op := {
		"op": "add_connection",
		"connection": {
			"id": "c-%d" % Time.get_ticks_msec(),
			"from": from_id,
			"to": to_id,
			"directed": true,
		},
	}
	var updated := await api.patch(instance.id, op)
	if not updated.is_empty():
		instance = updated
		_grid_rect.queue_redraw()


func _toggle_snc() -> void:
	# alterna a opacidade da camada SNC entre 0.5 e 0.1 (toggle de transparência)
	var current := 0.5
	for layer in instance.get("layers", []):
		if layer.get("id", "") == "snc":
			current = layer.get("opacity", 0.5)
	var next := 0.1 if current > 0.3 else 0.5
	var op := {"op": "edit_layer", "layer": "snc", "opacity": next}
	var updated := await api.patch(instance.id, op)
	if not updated.is_empty():
		instance = updated
		_set_status("SNC opacidade → %.1f" % next)
