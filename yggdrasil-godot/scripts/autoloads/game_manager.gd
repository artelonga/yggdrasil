extends Node

enum State { LOBBY, TRANSITIONING, IN_GAME }

var current_state: State = State.LOBBY
var current_game_name: String = ""
var last_cabinet_position: Vector2 = Vector2.ZERO

var _transition_overlay: ColorRect
var _tween: Tween


func _ready() -> void:
	_transition_overlay = ColorRect.new()
	_transition_overlay.color = Color(0, 0, 0, 0)
	_transition_overlay.z_index = 100
	_transition_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var canvas := CanvasLayer.new()
	canvas.layer = 100
	canvas.add_child(_transition_overlay)
	add_child(canvas)
	_resize_overlay()
	get_tree().root.size_changed.connect(_resize_overlay)


func _resize_overlay() -> void:
	_transition_overlay.size = get_viewport().get_visible_rect().size


func transition_to_game(scene_path: String, game_name: String, cabinet_pos: Vector2) -> void:
	if current_state == State.TRANSITIONING:
		return
	current_state = State.TRANSITIONING
	current_game_name = game_name
	last_cabinet_position = cabinet_pos
	SignalBus.game_started.emit(game_name)

	_fade_out()
	await _tween.finished
	get_tree().change_scene_to_file(scene_path)
	_fade_in()
	await _tween.finished
	current_state = State.IN_GAME


func return_to_lobby() -> void:
	if current_state == State.TRANSITIONING:
		return
	current_state = State.TRANSITIONING
	SignalBus.game_ended.emit(current_game_name, 0)
	current_game_name = ""

	_fade_out()
	await _tween.finished
	get_tree().change_scene_to_file("res://scenes/lobby/lobby.tscn")
	_fade_in()
	await _tween.finished
	current_state = State.LOBBY


func _fade_out() -> void:
	if _tween and _tween.is_running():
		_tween.kill()
	_tween = create_tween()
	_tween.tween_property(_transition_overlay, "color:a", 1.0, 0.3)


func _fade_in() -> void:
	if _tween and _tween.is_running():
		_tween.kill()
	_tween = create_tween()
	_tween.tween_property(_transition_overlay, "color:a", 0.0, 0.3)
