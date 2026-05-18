extends Area2D

@export var game_name: String = "Game"
@export var game_scene_path: String = ""
@export var cabinet_color: Color = Color.CYAN

@onready var sprite: Sprite2D = $Sprite2D
@onready var label: Label = $Label


func _ready() -> void:
	sprite.modulate = cabinet_color
	label.text = game_name
	body_entered.connect(_on_body_entered)
	body_exited.connect(_on_body_exited)


func _on_body_entered(body: Node2D) -> void:
	if body.has_method("_on_cabinet_entered"):
		body._on_cabinet_entered(self)


func _on_body_exited(body: Node2D) -> void:
	if body.has_method("_on_cabinet_exited"):
		body._on_cabinet_exited(self)


func enter_game() -> void:
	if game_scene_path.is_empty():
		return
	GameManager.transition_to_game(game_scene_path, game_name, global_position)
