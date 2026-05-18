class_name PokerPlayer
extends RefCounted

enum Status { ACTIVE, FOLDED, ALL_IN, WAITING, SITTING_OUT }

var id: String
var player_name: String
var chips: int
var hole_cards: Array[Card]
var status: int = Status.WAITING
var current_bet: int = 0
var is_dealer: bool = false
var is_local: bool = false


func _init(p_id: String = "", p_name: String = "", p_chips: int = 1000, p_local: bool = false) -> void:
	id = p_id
	player_name = p_name
	chips = p_chips
	is_local = p_local


func can_act() -> bool:
	return status == Status.ACTIVE


func new_hand() -> void:
	hole_cards.clear()
	current_bet = 0
	if chips > 0:
		status = Status.ACTIVE
	else:
		status = Status.SITTING_OUT
