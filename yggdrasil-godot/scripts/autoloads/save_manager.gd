extends Node

const SAVE_PATH := "user://save_data.json"

var player_profile: Dictionary = {
	"name": "Jogador",
	"created_at": "",
	"wallet": 0,
}

var game_stats: Dictionary = {
	"tetris": {"high_score": 0, "games_played": 0},
	"invaders": {"high_score": 0, "games_played": 0},
	"snake": {"high_score": 0, "games_played": 0},
	"pointset": {"high_score": 0, "games_played": 0},
	"poker": {"hands_played": 0, "hands_won": 0},
}

var settings: Dictionary = {
	"music_volume": 0.8,
	"sfx_volume": 1.0,
}


func _ready() -> void:
	load_save()


func save() -> void:
	var data := {
		"profile": player_profile,
		"stats": game_stats,
		"settings": settings,
	}
	var file := FileAccess.open(SAVE_PATH, FileAccess.WRITE)
	if file:
		file.store_string(JSON.stringify(data, "\t"))


func load_save() -> void:
	if not FileAccess.file_exists(SAVE_PATH):
		player_profile["created_at"] = Time.get_datetime_string_from_system()
		save()
		return
	var file := FileAccess.open(SAVE_PATH, FileAccess.READ)
	if not file:
		return
	var data = JSON.parse_string(file.get_as_text())
	if data is Dictionary:
		if data.has("profile"):
			player_profile.merge(data["profile"], true)
		if data.has("stats"):
			game_stats.merge(data["stats"], true)
		if data.has("settings"):
			settings.merge(data["settings"], true)


func record_game_result(game_name: String, score: int) -> void:
	if not game_stats.has(game_name):
		game_stats[game_name] = {"high_score": 0, "games_played": 0}
	var stats: Dictionary = game_stats[game_name]
	stats["games_played"] = stats.get("games_played", 0) + 1
	if score > stats.get("high_score", 0):
		stats["high_score"] = score
	save()
