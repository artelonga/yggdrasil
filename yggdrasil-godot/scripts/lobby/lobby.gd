extends Node2D

@onready var player: CharacterBody2D = $Player


func _ready() -> void:
	if GameManager.last_cabinet_position != Vector2.ZERO:
		player.global_position = GameManager.last_cabinet_position + Vector2(0, 24)
