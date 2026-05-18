extends GameBase

## PointSet: Select groups of adjacent same-colored points to clear them.
## Larger groups score exponentially more. Gravity fills gaps. Chain combos multiply.

const CELL_SIZE := 20
const GRID_COLS := 16
const GRID_ROWS := 12
const BOARD_OFFSET := Vector2(80, 30)
const NUM_COLORS := 5

const POINT_COLORS := [
	Color(0.9, 0.2, 0.2),   # Red
	Color(0.2, 0.7, 0.9),   # Cyan
	Color(0.2, 0.9, 0.2),   # Green
	Color(0.9, 0.9, 0.2),   # Yellow
	Color(0.8, 0.3, 0.9),   # Purple
]

var grid: Array[Array] = []
var cursor: Vector2i = Vector2i(0, 0)
var selected_group: Array[Vector2i] = []
var combo: int = 0
var game_is_over: bool = false
var time_left: float = 120.0
var clearing: bool = false
var clear_timer: float = 0.0


func _ready() -> void:
	game_name = "pointset"
	_init_grid()
	_update_selection()
	queue_redraw()


func _init_grid() -> void:
	grid.clear()
	for y in range(GRID_ROWS):
		var row: Array[int] = []
		for x in range(GRID_COLS):
			row.append((randi() % NUM_COLORS) + 1)
		grid.append(row)
	combo = 0


func _process(delta: float) -> void:
	if game_is_over or is_paused:
		return

	time_left -= delta
	if time_left <= 0:
		time_left = 0
		game_is_over = true
		SaveManager.record_game_result("pointset", score)

	if clearing:
		clear_timer += delta
		if clear_timer >= 0.2:
			clearing = false
			clear_timer = 0.0
			_apply_gravity()
			_fill_empty()
			_update_selection()
			if not _has_valid_moves():
				game_is_over = true
				SaveManager.record_game_result("pointset", score)

	queue_redraw()


func _unhandled_input(event: InputEvent) -> void:
	if game_is_over:
		if event.is_action_pressed("interact"):
			_restart()
		super._unhandled_input(event)
		return

	super._unhandled_input(event)

	if is_paused or clearing:
		return

	var moved := false
	if event.is_action_pressed("move_up") and cursor.y > 0:
		cursor.y -= 1
		moved = true
	elif event.is_action_pressed("move_down") and cursor.y < GRID_ROWS - 1:
		cursor.y += 1
		moved = true
	elif event.is_action_pressed("move_left") and cursor.x > 0:
		cursor.x -= 1
		moved = true
	elif event.is_action_pressed("move_right") and cursor.x < GRID_COLS - 1:
		cursor.x += 1
		moved = true

	if moved:
		_update_selection()

	if event.is_action_pressed("interact"):
		_clear_selection()


func _update_selection() -> void:
	selected_group.clear()
	var color_id: int = grid[cursor.y][cursor.x]
	if color_id == 0:
		return
	var visited: Dictionary = {}
	var stack: Array[Vector2i] = [cursor]
	while not stack.is_empty():
		var pos: Vector2i = stack.pop_back()
		if visited.has(pos):
			continue
		if pos.x < 0 or pos.x >= GRID_COLS or pos.y < 0 or pos.y >= GRID_ROWS:
			continue
		if grid[pos.y][pos.x] != color_id:
			continue
		visited[pos] = true
		selected_group.append(pos)
		stack.append(Vector2i(pos.x + 1, pos.y))
		stack.append(Vector2i(pos.x - 1, pos.y))
		stack.append(Vector2i(pos.x, pos.y + 1))
		stack.append(Vector2i(pos.x, pos.y - 1))


func _clear_selection() -> void:
	if selected_group.size() < 2:
		return

	var n := selected_group.size()
	combo += 1
	var combo_mult := 1 + (combo - 1) * 0.5
	score += int(n * (n - 1) * combo_mult)

	for pos in selected_group:
		grid[pos.y][pos.x] = 0

	clearing = true
	clear_timer = 0.0
	selected_group.clear()


func _apply_gravity() -> void:
	for x in range(GRID_COLS):
		var write_y := GRID_ROWS - 1
		for read_y in range(GRID_ROWS - 1, -1, -1):
			if grid[read_y][x] != 0:
				grid[write_y][x] = grid[read_y][x]
				if write_y != read_y:
					grid[read_y][x] = 0
				write_y -= 1
		while write_y >= 0:
			grid[write_y][x] = 0
			write_y -= 1


func _fill_empty() -> void:
	for y in range(GRID_ROWS):
		for x in range(GRID_COLS):
			if grid[y][x] == 0:
				grid[y][x] = (randi() % NUM_COLORS) + 1


func _has_valid_moves() -> bool:
	for y in range(GRID_ROWS):
		for x in range(GRID_COLS):
			var c: int = grid[y][x]
			if c == 0:
				continue
			if x + 1 < GRID_COLS and grid[y][x + 1] == c:
				return true
			if y + 1 < GRID_ROWS and grid[y + 1][x] == c:
				return true
	return false


func _restart() -> void:
	score = 0
	time_left = 120.0
	game_is_over = false
	combo = 0
	_init_grid()
	cursor = Vector2i(0, 0)
	_update_selection()


func _draw() -> void:
	draw_rect(Rect2(0, 0, 640, 360), Color(0.04, 0.02, 0.06))

	draw_rect(Rect2(BOARD_OFFSET.x - 1, BOARD_OFFSET.y - 1,
		GRID_COLS * CELL_SIZE + 2, GRID_ROWS * CELL_SIZE + 2), Color(0.08, 0.05, 0.1))
	draw_rect(Rect2(BOARD_OFFSET.x - 2, BOARD_OFFSET.y - 2,
		GRID_COLS * CELL_SIZE + 4, GRID_ROWS * CELL_SIZE + 4), Color(0.4, 0.2, 0.5), false, 2.0)

	for y in range(GRID_ROWS):
		for x in range(GRID_COLS):
			var color_id: int = grid[y][x]
			if color_id == 0:
				continue
			var cell_pos := BOARD_OFFSET + Vector2(x * CELL_SIZE, y * CELL_SIZE)
			var color: Color = POINT_COLORS[color_id - 1]
			var is_selected := Vector2i(x, y) in selected_group
			if is_selected:
				color = color.lightened(0.3)
			draw_rect(Rect2(cell_pos + Vector2(2, 2), Vector2(CELL_SIZE - 4, CELL_SIZE - 4)), color)
			draw_rect(Rect2(cell_pos + Vector2(4, 4), Vector2(CELL_SIZE - 10, CELL_SIZE - 10)), color.lightened(0.2))

	var cursor_pos := BOARD_OFFSET + Vector2(cursor.x * CELL_SIZE, cursor.y * CELL_SIZE)
	draw_rect(Rect2(cursor_pos, Vector2(CELL_SIZE, CELL_SIZE)), Color.WHITE, false, 2.0)

	var font := ThemeDB.fallback_font
	var hud_x := BOARD_OFFSET.x + GRID_COLS * CELL_SIZE + 20

	draw_string(font, Vector2(hud_x, 50), "SCORE", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(0.7, 0.5, 0.8))
	draw_string(font, Vector2(hud_x, 68), str(score), HORIZONTAL_ALIGNMENT_LEFT, -1, 14, Color.WHITE)

	draw_string(font, Vector2(hud_x, 100), "TEMPO", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(0.7, 0.5, 0.8))
	var time_color := Color.WHITE if time_left > 20 else Color(1, 0.3, 0.3)
	draw_string(font, Vector2(hud_x, 118), "%d" % ceili(time_left), HORIZONTAL_ALIGNMENT_LEFT, -1, 14, time_color)

	draw_string(font, Vector2(hud_x, 150), "COMBO", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(0.7, 0.5, 0.8))
	draw_string(font, Vector2(hud_x, 168), "x%d" % combo, HORIZONTAL_ALIGNMENT_LEFT, -1, 14, Color.YELLOW if combo > 1 else Color.WHITE)

	if selected_group.size() >= 2:
		draw_string(font, Vector2(hud_x, 200), "Grupo: %d" % selected_group.size(), HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(0.8, 0.8, 0.5))

	draw_string(font, Vector2(10, 350), "Setas: mover  Enter: limpar  Q: sair", HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(0.4, 0.3, 0.5))

	if game_is_over:
		draw_rect(Rect2(0, 0, 640, 360), Color(0, 0, 0, 0.6))
		var msg := "TEMPO ESGOTADO!" if time_left <= 0 else "SEM MOVIMENTOS!"
		draw_string(font, Vector2(240, 140), msg, HORIZONTAL_ALIGNMENT_CENTER, 160, 20, Color(0.9, 0.3, 0.9))
		draw_string(font, Vector2(220, 175), "Pontuação: %d" % score, HORIZONTAL_ALIGNMENT_CENTER, 200, 14, Color.WHITE)
		draw_string(font, Vector2(200, 210), "ENTER: reiniciar", HORIZONTAL_ALIGNMENT_CENTER, 240, 12, Color(0.7, 0.5, 0.8))
		draw_string(font, Vector2(200, 235), "Q: voltar ao lobby", HORIZONTAL_ALIGNMENT_CENTER, 240, 12, Color(0.5, 0.3, 0.6))
