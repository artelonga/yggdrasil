extends Node

var current_music: AudioStreamPlayer


func _ready() -> void:
	current_music = AudioStreamPlayer.new()
	current_music.bus = "Music"
	add_child(current_music)


func play_music(stream: AudioStream) -> void:
	if current_music.stream == stream and current_music.playing:
		return
	current_music.stream = stream
	current_music.play()


func stop_music() -> void:
	current_music.stop()


func play_sfx(stream: AudioStream) -> void:
	var player := AudioStreamPlayer.new()
	player.stream = stream
	player.bus = "SFX"
	add_child(player)
	player.play()
	player.finished.connect(player.queue_free)
