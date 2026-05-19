extends GameBase

# Server coordinate space
const SRV_W := 480.0
const SRV_H := 480.0

# Uniform scale to fit 640×360, keeping aspect ratio
const SCALE := 0.75  # min(640/480, 360/480) = 0.75
const OFFSET_X := (640 - SRV_W * SCALE) / 2.0  # 80
const OFFSET_Y := 0.0

# Server constants (reflected for hit detection display)
const ALIEN_W := 32.0
const ALIEN_H := 24.0
const PLAYER_W := 40.0
const PLAYER_H := 10.0
const PLAYER_Y_SRV := 450.0

var session_id: String = ""
var player_x: float = 220.0
var bullets: Array = []
var aliens: Array = []
var lives: int = 3
var game_is_over: bool = false
var won: bool = false
var connecting: bool = false
var awaiting: bool = false

var tick_timer: float = 0.0
const TICK_INTERVAL := 0.05  # 20 fps game logic


func _ready() -> void:
	game_name = "invaders"
	connecting = true
	_start_server_game()


func _start_server_game() -> void:
	var result := await ApiClient.start_game("invaders")
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
		player_x = float(st.get("player_x", player_x))
		bullets = st.get("bullets", bullets)
		aliens = st.get("aliens", aliens)
		lives = int(st.get("lives", lives))
		game_is_over = bool(st.get("game_over", game_is_over))
		won = bool(st.get("won", won))


func _process(delta: float) -> void:
	if (game_is_over or won) or is_paused or session_id.is_empty():
		return
	tick_timer += delta
	if tick_timer >= TICK_INTERVAL and not awaiting:
		tick_timer = 0.0
		_send_tick()


func _send_tick() -> void:
	var dir := "Tick"
	if Input.is_action_pressed("move_left"):
		dir = "Left"
	elif Input.is_action_pressed("move_right"):
		dir = "Right"
	_send_input(dir)


func _send_input(direction: String) -> void:
	if awaiting or session_id.is_empty():
		return
	awaiting = true
	var result := await ApiClient.send_game_input("invaders", session_id, direction)
	awaiting = false
	if result.is_empty():
		return
	_apply_state(result)
	if result.get("action", "continue") == "quit":
		SaveManager.record_game_result(game_name, score)
		game_over_signal.emit(score)
	queue_redraw()


func quit_to_lobby() -> void:
	if not session_id.is_empty():
		ApiClient.send_game_input("invaders", session_id, "Quit")
		session_id = ""
	SaveManager.record_game_result(game_name, score)
	game_quit_signal.emit()
	GameManager.return_to_lobby()


func _unhandled_input(event: InputEvent) -> void:
	if game_is_over or won:
		if event.is_action_pressed("interact"):
			game_is_over = false
			won = false
			session_id = ""
			score = 0
			lives = 3
			bullets = []
			aliens = []
			connecting = true
			_start_server_game()
		elif event.is_action_pressed("quit_game"):
			quit_to_lobby()
		return

	super._unhandled_input(event)
	if is_paused:
		return

	if event.is_action_pressed("interact"):
		_send_input("Shoot")


func _s(v: float) -> float:
	return v * SCALE


func _draw() -> void:
	draw_rect(Rect2(0, 0, 640, 360), Color(0.0, 0.0, 0.03))

	# Stars (decorative)
	var rng := RandomNumberGenerator.new()
	rng.seed = 42
	for i in range(50):
		var sx := OFFSET_X + rng.randf() * _s(SRV_W)
		var sy := OFFSET_Y + rng.randf() * (_s(SRV_H) - 30)
		draw_rect(Rect2(sx, sy, 1, 1), Color(0.5, 0.5, 0.6, 0.6))

	# Aliens
	var alien_colors := [Color(0.9, 0.2, 0.2), Color(0.2, 0.8, 0.2), Color(0.3, 0.3, 0.9), Color(0.8, 0.8, 0.2)]
	for alien in aliens:
		if not bool(alien.get("alive", false)):
			continue
		var ax := OFFSET_X + _s(float(alien.get("x", 0)))
		var ay := OFFSET_Y + _s(float(alien.get("y", 0)))
		var aw := _s(ALIEN_W)
		var ah := _s(ALIEN_H)
		var row: int = int(alien.get("row", 0))
		var color := alien_colors[clampi(row, 0, alien_colors.size() - 1)]
		draw_rect(Rect2(ax + _s(2), ay, aw - _s(4), ah), color)
		draw_rect(Rect2(ax, ay + _s(2), _s(2), ah - _s(4)), color)
		draw_rect(Rect2(ax + aw - _s(2), ay + _s(2), _s(2), ah - _s(4)), color)

	# Player
	var px := OFFSET_X + _s(player_x)
	var py := OFFSET_Y + _s(PLAYER_Y_SRV)
	var pw := _s(PLAYER_W)
	var ph := _s(PLAYER_H)
	draw_rect(Rect2(px, py, pw, ph), Color(0.2, 0.9, 0.2))
	draw_rect(Rect2(px + pw / 2 - _s(2), py - _s(4), _s(4), _s(4)), Color(0.3, 1.0, 0.3))

	# Bullets
	for bullet in bullets:
		var bx := OFFSET_X + _s(float(bullet.get("x", 0)))
		var by := OFFSET_Y + _s(float(bullet.get("y", 0)))
		var is_alien: bool = bool(bullet.get("alien", false))
		var bc := Color(1, 0.3, 0.3) if is_alien else Color(1, 1, 0.3)
		draw_rect(Rect2(bx - 1, by, 2, _s(8)), bc)

	# HUD
	var font := ThemeDB.fallback_font
	draw_string(font, Vector2(10, 16), "SCORE: %d" % score,
		HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color.WHITE)
	for i in range(lives):
		draw_rect(Rect2(10 + i * 20, 345, 14, 8), Color(0.2, 0.9, 0.2))
	draw_string(font, Vector2(500, 356),
		"Setas: mover  Enter: atirar  Q: sair",
		HORIZONTAL_ALIGNMENT_LEFT, -1, 8, Color(0.3, 0.3, 0.4))

	if connecting:
		draw_rect(Rect2(0, 0, 640, 360), Color(0, 0, 0, 0.5))
		draw_string(font, Vector2(200, 170), "Conectando ao servidor...",
			HORIZONTAL_ALIGNMENT_CENTER, 240, 14, Color.YELLOW)

	if won:
		draw_rect(Rect2(0, 0, 640, 360), Color(0, 0, 0, 0.6))
		draw_string(font, Vector2(200, 150), "VOCÊ VENCEU!", HORIZONTAL_ALIGNMENT_CENTER, 240, 20, Color.YELLOW)
		draw_string(font, Vector2(220, 185), "Pontuação: %d" % score, HORIZONTAL_ALIGNMENT_CENTER, 200, 14, Color.WHITE)
		draw_string(font, Vector2(200, 215), "ENTER: reiniciar", HORIZONTAL_ALIGNMENT_CENTER, 240, 12, Color(0.7, 0.7, 0.8))
		draw_string(font, Vector2(200, 240), "Q: voltar ao lobby", HORIZONTAL_ALIGNMENT_CENTER, 240, 12, Color(0.5, 0.5, 0.6))
	elif game_is_over:
		draw_rect(Rect2(0, 0, 640, 360), Color(0, 0, 0, 0.6))
		draw_string(font, Vector2(240, 150), "FIM DE JOGO", HORIZONTAL_ALIGNMENT_CENTER, 160, 20, Color.RED)
		draw_string(font, Vector2(220, 185), "Pontuação: %d" % score, HORIZONTAL_ALIGNMENT_CENTER, 200, 14, Color.WHITE)
		draw_string(font, Vector2(200, 215), "ENTER: reiniciar", HORIZONTAL_ALIGNMENT_CENTER, 240, 12, Color(0.7, 0.7, 0.8))
		draw_string(font, Vector2(200, 240), "Q: voltar ao lobby", HORIZONTAL_ALIGNMENT_CENTER, 240, 12, Color(0.5, 0.5, 0.6))
