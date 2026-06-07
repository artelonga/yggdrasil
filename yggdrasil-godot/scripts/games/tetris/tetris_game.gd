extends GameBase

const CELL_SIZE := 16
const BOARD_COLS := 10
const BOARD_ROWS := 20

# Board offset — center in 640×360
const BOARD_X := (640 - BOARD_COLS * CELL_SIZE) / 2.0  # 240
const BOARD_Y := (360 - BOARD_ROWS * CELL_SIZE) / 2.0  # 20

const PIECE_COLORS := [
	Color(0, 0, 0, 0),          # 0 = empty
	Color(0.0, 0.9, 0.9, 1),    # 1 I — Cyan
	Color(0.9, 0.9, 0.0, 1),    # 2 O — Yellow
	Color(0.6, 0.0, 0.8, 1),    # 3 T — Purple
	Color(0.0, 0.9, 0.0, 1),    # 4 S — Green
	Color(0.9, 0.0, 0.0, 1),    # 5 Z — Red
	Color(0.0, 0.0, 0.9, 1),    # 6 J — Blue
	Color(0.9, 0.5, 0.0, 1),    # 7 L — Orange
]

var session_id: String = ""
var board: Array = []   # 20×10 of int
var piece := {}         # {piece_type, x, y, shape}
var level: int = 1
var game_is_over: bool = false
var connecting: bool = false
var awaiting: bool = false

# Gravity timer — client sends "Drop" to server
var drop_timer: float = 0.0

# DAS (delayed auto shift)
var das_dir: int = 0
var das_timer: float = 0.0
const DAS_DELAY := 0.17
const DAS_REPEAT := 0.05
var das_repeating: bool = false


func _ready() -> void:
	game_name = "tetris"
	connecting = true
	_start_server_game()


func _start_server_game() -> void:
	var result := await ApiClient.start_game("tetris")
	connecting = false
	if result.is_empty():
		game_is_over = true
		queue_redraw()
		return
	session_id = result.get("id", "")
	_apply_state(result)
	queue_redraw()


func _apply_state(result: Dictionary) -> void:
	score = result.get("score", score)
	var st = result.get("state", {})
	if st is Dictionary:
		board = st.get("board", board)
		piece = st.get("piece", piece)
		level = st.get("level", level)
		game_is_over = st.get("game_over", game_is_over)


func _drop_interval() -> float:
	return maxf(0.05, 0.8 - (level - 1) * 0.07)


func _process(delta: float) -> void:
	if game_is_over or is_paused or session_id.is_empty():
		return

	# Gravity
	drop_timer += delta
	if drop_timer >= _drop_interval() and not awaiting:
		drop_timer = 0.0
		_send_input("Drop")

	# DAS
	if das_dir != 0:
		das_timer += delta
		var threshold := DAS_REPEAT if das_repeating else DAS_DELAY
		if das_timer >= threshold and not awaiting:
			das_timer = 0.0
			das_repeating = true
			_send_input("Left" if das_dir < 0 else "Right")


func _send_input(direction: String) -> void:
	if awaiting or session_id.is_empty():
		return
	awaiting = true
	var result := await ApiClient.send_game_input("tetris", session_id, direction)
	awaiting = false
	if result.is_empty():
		return
	_apply_state(result)
	if result.get("action", "continue") == "quit":
		game_is_over = true
		SaveManager.record_game_result(game_name, score)
		game_over_signal.emit(score)
	queue_redraw()


func quit_to_lobby() -> void:
	if not session_id.is_empty():
		ApiClient.send_game_input("tetris", session_id, "Quit")
		session_id = ""
	SaveManager.record_game_result(game_name, score)
	game_quit_signal.emit()
	GameManager.return_to_lobby()


func _unhandled_input(event: InputEvent) -> void:
	if game_is_over:
		if event.is_action_pressed("interact"):
			game_is_over = false
			session_id = ""
			board = []
			piece = {}
			score = 0
			level = 1
			drop_timer = 0.0
			connecting = true
			_start_server_game()
		elif event.is_action_pressed("quit_game"):
			quit_to_lobby()
		return

	super._unhandled_input(event)
	if is_paused:
		return

	if event.is_action_pressed("move_left"):
		_send_input("Left")
		das_dir = -1
		das_timer = 0.0
		das_repeating = false
	elif event.is_action_pressed("move_right"):
		_send_input("Right")
		das_dir = 1
		das_timer = 0.0
		das_repeating = false
	elif event.is_action_released("move_left") and das_dir == -1:
		das_dir = 0
	elif event.is_action_released("move_right") and das_dir == 1:
		das_dir = 0

	if event.is_action_pressed("move_up"):
		_send_input("Rotate")
	elif event.is_action_pressed("move_down"):
		_send_input("Down")
	elif event.is_action_pressed("interact"):
		_send_input("HardDrop")


func _draw() -> void:
	draw_rect(Rect2(0, 0, 640, 360), Color(0.05, 0.05, 0.1))

	# Board border
	draw_rect(Rect2(BOARD_X - 2, BOARD_Y - 2,
		BOARD_COLS * CELL_SIZE + 4, BOARD_ROWS * CELL_SIZE + 4),
		Color(0.3, 0.3, 0.4), false, 2.0)
	draw_rect(Rect2(BOARD_X, BOARD_Y,
		BOARD_COLS * CELL_SIZE, BOARD_ROWS * CELL_SIZE),
		Color(0.08, 0.08, 0.12))

	# Locked cells
	for y in range(board.size()):
		var row = board[y]
		if not row is Array:
			continue
		for x in range(row.size()):
			var ct: int = int(row[x])
			if ct > 0 and ct < PIECE_COLORS.size():
				_draw_cell(x, y, PIECE_COLORS[ct])

	# Active piece
	if piece.has("shape") and piece.has("x") and piece.has("y"):
		var ct: int = piece.get("piece_type", 0)
		var color: Color = PIECE_COLORS[ct] if ct < PIECE_COLORS.size() else Color.WHITE
		var px: int = piece["x"]
		var py: int = piece["y"]
		var shape = piece.get("shape", [])
		for r in range(shape.size()):
			var row2 = shape[r]
			if not row2 is Array:
				continue
			for c in range(row2.size()):
				if int(row2[c]) != 0:
					_draw_cell(px + c, py + r, color)

	# HUD
	var font := ThemeDB.fallback_font
	var hx := BOARD_X + BOARD_COLS * CELL_SIZE + 16.0
	draw_string(font, Vector2(hx, BOARD_Y + 20), "SCORE", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(0.7, 0.7, 0.8))
	draw_string(font, Vector2(hx, BOARD_Y + 36), str(score), HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color.WHITE)
	draw_string(font, Vector2(hx, BOARD_Y + 70), "NÍVEL", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(0.7, 0.7, 0.8))
	draw_string(font, Vector2(hx, BOARD_Y + 86), str(level), HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color.WHITE)

	draw_string(font, Vector2(10, 50), "CONTROLES", HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(0.5, 0.5, 0.6))
	draw_string(font, Vector2(10, 66), "Setas: mover", HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(0.4, 0.4, 0.5))
	draw_string(font, Vector2(10, 80), "Cima: girar", HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(0.4, 0.4, 0.5))
	draw_string(font, Vector2(10, 94), "Baixo: descer", HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(0.4, 0.4, 0.5))
	draw_string(font, Vector2(10, 108), "Enter: soltar", HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(0.4, 0.4, 0.5))
	draw_string(font, Vector2(10, 122), "Q: sair", HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(0.4, 0.4, 0.5))

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
			HORIZONTAL_ALIGNMENT_CENTER, 240, 12, Color(0.7, 0.7, 0.8))
		draw_string(font, Vector2(200, 240), "Q: voltar ao lobby",
			HORIZONTAL_ALIGNMENT_CENTER, 240, 12, Color(0.5, 0.5, 0.6))


func _draw_cell(gx: int, gy: int, color: Color) -> void:
	if gy < 0:
		return
	var rect := Rect2(
		BOARD_X + gx * CELL_SIZE + 1,
		BOARD_Y + gy * CELL_SIZE + 1,
		CELL_SIZE - 2, CELL_SIZE - 2
	)
	draw_rect(rect, color)
	var hl := color.lightened(0.3)
	draw_line(rect.position, rect.position + Vector2(rect.size.x, 0), hl, 1.0)
	draw_line(rect.position, rect.position + Vector2(0, rect.size.y), hl, 1.0)
