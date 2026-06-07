extends Node

signal game_started(game_name: String)
signal game_ended(game_name: String, score: int)
signal wallet_updated(new_balance: int)
signal player_entered_cabinet_zone(game_name: String)
signal player_left_cabinet_zone()
