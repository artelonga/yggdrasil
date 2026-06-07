class_name GameBase
extends Node2D

signal game_over_signal(score: int)
signal game_quit_signal

var score: int = 0
var is_paused: bool = false
var game_name: String = ""


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("pause"):
		toggle_pause()
	if event.is_action_pressed("quit_game"):
		quit_to_lobby()


func toggle_pause() -> void:
	is_paused = !is_paused
	get_tree().paused = is_paused


func quit_to_lobby() -> void:
	if is_paused:
		toggle_pause()
	SaveManager.record_game_result(game_name, score)
	game_quit_signal.emit()
	GameManager.return_to_lobby()
