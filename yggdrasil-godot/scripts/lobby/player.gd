extends CharacterBody2D

const SPEED := 100.0

var nearby_cabinet: Node = null

@onready var sprite: Sprite2D = $Sprite2D
@onready var interaction_label: Label = $InteractionLabel


func _ready() -> void:
	interaction_label.visible = false


func _physics_process(_delta: float) -> void:
	var direction := Vector2.ZERO
	if Input.is_action_pressed("move_up"):
		direction.y -= 1
	if Input.is_action_pressed("move_down"):
		direction.y += 1
	if Input.is_action_pressed("move_left"):
		direction.x -= 1
	if Input.is_action_pressed("move_right"):
		direction.x += 1

	if direction != Vector2.ZERO:
		direction = direction.normalized()
		if direction.x != 0:
			sprite.flip_h = direction.x < 0

	velocity = direction * SPEED
	move_and_slide()


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("interact") and nearby_cabinet:
		nearby_cabinet.enter_game()


func _on_cabinet_entered(cabinet: Node) -> void:
	nearby_cabinet = cabinet
	interaction_label.visible = true
	interaction_label.text = "ENTER — " + cabinet.game_name
	SignalBus.player_entered_cabinet_zone.emit(cabinet.game_name)


func _on_cabinet_exited(_cabinet: Node) -> void:
	nearby_cabinet = null
	interaction_label.visible = false
	SignalBus.player_left_cabinet_zone.emit()
