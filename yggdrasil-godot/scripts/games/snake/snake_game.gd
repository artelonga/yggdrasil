extends GameBase

# Map constants matching the server (40×20 grid, 16px cells)
const CELL_SIZE := 16
const GRID_W := 40
const GRID_H := 20

var session_id: String = ""
var tiles: Array = []
var pending_dir: String = "None"
var tick_timer: float = 0.0
const TICK_INTERVAL := 0.12
var game_is_over: bool = false
var awaiting: bool = false
var connecting: bool = false


func _ready() -> void:
	game_name = "snake"
	connecting = true
	_start_server_game()


func _start_server_game() -> void:
	var result := await ApiClient.start_game("snake")
	connecting = false
	if result.is_empty():
		game_is_over = true
		queue_redraw()
		return
	session_id = result.get("id", "")
	score = result.get("score", 0)
	var st = result.get("state", {})
	tiles = st.get("tiles", [])
	queue_redraw()


func _process(delta: float) -> void:
	if game_is_over or is_paused or awaiting or session_id.is_empty():
		return
	tick_timer += delta
	if tick_timer >= TICK_INTERVAL:
		tick_timer = 0.0
		_tick()


func _tick() -> void:
	awaiting = true
	var result := await ApiClient.send_game_input("snake", session_id, pending_dir)
	awaiting = false
	pending_dir = "None"
	if result.is_empty():
		return
	score = result.get("score", score)
	var st = result.get("state", {})
	if st is Dictionary:
		tiles = st.get("tiles", tiles)
	if result.get("action", "continue") == "quit":
		game_is_over = true
		SaveManager.record_game_result(game_name, score)
		game_over_signal.emit(score)
	queue_redraw()


func quit_to_lobby() -> void:
	if not session_id.is_empty():
		ApiClient.send_game_input("snake", session_id, "Quit")
		session_id = ""
	SaveManager.record_game_result(game_name, score)
	game_quit_signal.emit()
	GameManager.return_to_lobby()


func _unhandled_input(event: InputEvent) -> void:
	if game_is_over:
		if event.is_action_pressed("interact"):
			game_is_over = false
			session_id = ""
			pending_dir = "None"
			score = 0
			tiles = []
			connecting = true
			_start_server_game()
		elif event.is_action_pressed("quit_game"):
			quit_to_lobby()
		return

	super._unhandled_input(event)
	if is_paused:
		return

	if event.is_action_pressed("move_up"):
		pending_dir = "Up"
	elif event.is_action_pressed("move_down"):
		pending_dir = "Down"
	elif event.is_action_pressed("move_left"):
		pending_dir = "Left"
	elif event.is_action_pressed("move_right"):
		pending_dir = "Right"


func _draw() -> void:
	var offset_x := 0.0
	var offset_y := (360 - GRID_H * CELL_SIZE) / 2.0

	# Background
	draw_rect(Rect2(0, 0, 640, 360), Color(0.02, 0.06, 0.02))

	# Board
	draw_rect(Rect2(offset_x, offset_y, GRID_W * CELL_SIZE, GRID_H * CELL_SIZE),
		Color(0.05, 0.1, 0.05))
	draw_rect(Rect2(offset_x - 1, offset_y - 1, GRID_W * CELL_SIZE + 2, GRID_H * CELL_SIZE + 2),
		Color(0.2, 0.5, 0.2), false, 2.0)

	# Tiles
	for y in range(tiles.size()):
		var row = tiles[y]
		if not row is Array:
			continue
		for x in range(row.size()):
			var cell = row[x]
			var color := _tile_color(cell)
			if color.a > 0.01:
				draw_rect(Rect2(
					offset_x + x * CELL_SIZE + 1,
					offset_y + y * CELL_SIZE + 1,
					CELL_SIZE - 2, CELL_SIZE - 2
				), color)

	var font := ThemeDB.fallback_font
	draw_string(font, Vector2(8, offset_y + GRID_H * CELL_SIZE + 14),
		"Pontuação: %d" % score, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(0.7, 0.9, 0.7))
	draw_string(font, Vector2(500, offset_y + GRID_H * CELL_SIZE + 14),
		"Q: Sair", HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(0.4, 0.6, 0.4))

	if connecting:
		draw_rect(Rect2(0, 0, 640, 360), Color(0, 0, 0, 0.5))
		draw_string(font, Vector2(200, 170), "Conectando ao servidor...",
			HORIZONTAL_ALIGNMENT_CENTER, 240, 14, Color.YELLOW)

	if game_is_over:
		draw_rect(Rect2(0, 0, 640, 360), Color(0, 0, 0, 0.6))
		draw_string(font, Vector2(240, 150), "FIM DE JOGO",
			HORIZONTAL_ALIGNMENT_CENTER, 160, 20, Color.RED)
		draw_string(font, Vector2(220, 185), "Pontuação: %d" % score,
			HORIZONTAL_ALIGNMENT_CENTER, 200, 14, Color.WHITE)
		draw_string(font, Vector2(200, 215), "ENTER: reiniciar",
			HORIZONTAL_ALIGNMENT_CENTER, 240, 12, Color(0.7, 0.9, 0.7))
		draw_string(font, Vector2(200, 240), "Q: voltar ao lobby",
			HORIZONTAL_ALIGNMENT_CENTER, 240, 12, Color(0.4, 0.6, 0.4))


func _tile_color(cell) -> Color:
	if cell == "Wall":
		return Color(0.15, 0.25, 0.15)
	if cell == "Empty":
		return Color(0, 0, 0, 0)
	if cell is Dictionary and cell.has("Entity"):
		var ent = cell["Entity"]
		match ent.get("entity_type", ""):
			"Player":
				return Color(0.1, 0.95, 0.1)
			"Obstacle":
				return Color(0.0, 0.65, 0.0)
			"Item":
				return Color(0.9, 0.15, 0.15)
	return Color(0, 0, 0, 0)
