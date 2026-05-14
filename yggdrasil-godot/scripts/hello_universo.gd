extends Node2D

# Hello universo — sanity-check do scaffold POC (YG-31).
#
# Imprime "hello from server" ou "hello from client" conforme o contexto
# (headless server vs. browser client). Não há lógica de jogo aqui — o
# objetivo é apenas validar que a toolchain Godot ↔ web ↔ headless funciona
# de ponta a ponta. Lógica real entra em YG-32 (Lobby.tscn) e diante.


func _ready() -> void:
	var role := "server" if multiplayer.is_server() else "client"
	print("hello from %s" % role)
