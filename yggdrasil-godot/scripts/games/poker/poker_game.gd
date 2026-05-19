extends GameBase

## Full Texas Hold'em poker — ported from Rust implementation

const SCREEN_W := 640.0
const SCREEN_H := 360.0

enum BettingRound { PRE_FLOP, FLOP, TURN, RIVER, SHOWDOWN }
enum SelectedAction { FOLD, CHECK, CALL, RAISE, ALL_IN }

const ACTION_LABELS := ["[F]old", "[C]heck", "Ca[L]l", "[R]aise", "[A]ll-in"]

# Game config
var small_blind := 10
var big_blind := 20
var starting_chips := 1000

# State
var players: Array[PokerPlayer] = []
var table_pot: int = 0
var table_current_bet: int = 0
var community_cards: Array[Card] = []
var deck: Deck
var round: int = BettingRound.PRE_FLOP
var dealer_position: int = 0
var action_position: int = 0
var local_player_index: int = 0
var selected_action: int = SelectedAction.CHECK
var raise_amount: int = 40
var game_is_over: bool = false
var winner_message: String = ""
var hand_count: int = 0

# AI
var ai_controllers: Dictionary = {}  # player_index -> PokerAI
var ai_timer: float = 0.0
var ai_delay: float = 0.4


func _ready() -> void:
	game_name = "poker"

	# Create local player
	var local := PokerPlayer.new("local", "You", starting_chips, true)
	players.append(local)
	local_player_index = 0

	# Add AI opponents
	_add_ai("Alice", PokerAI.Style.TIGHT)
	_add_ai("Bob", PokerAI.Style.LOOSE)
	_add_ai("Charlie", PokerAI.Style.AGGRESSIVE)

	raise_amount = big_blind * 2
	start_hand()
	queue_redraw()


func _add_ai(ai_name: String, style: int) -> void:
	var idx := players.size()
	var p := PokerPlayer.new("ai_%d" % idx, ai_name, starting_chips, false)
	players.append(p)
	ai_controllers[idx] = PokerAI.new(style)


func start_hand() -> void:
	table_pot = 0
	table_current_bet = 0
	community_cards.clear()
	deck = Deck.new()
	round = BettingRound.PRE_FLOP
	game_is_over = false
	winner_message = ""
	hand_count += 1

	for p in players:
		p.new_hand()

	# Rotate dealer
	dealer_position = _wrap(dealer_position + 1, players.size())
	for i in range(players.size()):
		players[i].is_dealer = (i == dealer_position)

	# Deal hole cards
	for p in players:
		if p.status == PokerPlayer.Status.ACTIVE:
			var c1 := deck.deal()
			var c2 := deck.deal()
			if c1 and c2:
				p.hole_cards = [c1, c2]

	# Deal community cards (face down)
	deck.burn()
	for i in range(3):
		var c := deck.deal()
		if c:
			community_cards.append(c)
	deck.burn()
	var turn_card := deck.deal()
	if turn_card:
		community_cards.append(turn_card)
	deck.burn()
	var river_card := deck.deal()
	if river_card:
		community_cards.append(river_card)

	# Collect blinds
	_collect_blinds()

	# Set first action
	if players.size() > 2:
		action_position = _wrap(dealer_position + 3, players.size())
	else:
		action_position = dealer_position

	selected_action = SelectedAction.CHECK
	raise_amount = big_blind * 2


func _collect_blinds() -> void:
	if players.size() < 2:
		return

	var sb_pos := _wrap(dealer_position + 1, players.size())
	var sb_amt := mini(small_blind, players[sb_pos].chips)
	players[sb_pos].chips -= sb_amt
	players[sb_pos].current_bet = sb_amt
	table_pot += sb_amt

	var bb_pos := _wrap(dealer_position + 2, players.size())
	var bb_amt := mini(big_blind, players[bb_pos].chips)
	players[bb_pos].chips -= bb_amt
	players[bb_pos].current_bet = bb_amt
	table_pot += bb_amt
	table_current_bet = bb_amt


func execute_action(action: String, amount: int = 0) -> void:
	var player := players[action_position]

	match action:
		"fold":
			player.status = PokerPlayer.Status.FOLDED
		"check":
			pass
		"call":
			var call_amount := table_current_bet - player.current_bet
			var actual := mini(call_amount, player.chips)
			player.chips -= actual
			player.current_bet += actual
			table_pot += actual
			if player.chips == 0:
				player.status = PokerPlayer.Status.ALL_IN
		"raise":
			var total_bet := table_current_bet + amount
			var to_add := total_bet - player.current_bet
			var actual := mini(to_add, player.chips)
			player.chips -= actual
			player.current_bet += actual
			table_pot += actual
			table_current_bet = player.current_bet
			if player.chips == 0:
				player.status = PokerPlayer.Status.ALL_IN
		"allin":
			var amt := player.chips
			player.current_bet += amt
			table_pot += amt
			player.chips = 0
			player.status = PokerPlayer.Status.ALL_IN
			if player.current_bet > table_current_bet:
				table_current_bet = player.current_bet

	if _check_hand_over():
		_determine_winner()
		return

	_advance_action()


func _check_hand_over() -> bool:
	var active := 0
	for p in players:
		if p.status == PokerPlayer.Status.ACTIVE or p.status == PokerPlayer.Status.ALL_IN:
			active += 1
	return active <= 1


func _advance_action() -> void:
	var num := players.size()
	var next := _wrap(action_position + 1, num)
	var starting := next

	# Find next active player
	while true:
		if players[next].can_act():
			break
		next = _wrap(next + 1, num)
		if next == starting:
			_advance_round()
			return

	# Check if betting round is complete
	var all_matched := true
	for p in players:
		if p.can_act() and p.current_bet != table_current_bet:
			all_matched = false
			break

	if all_matched and next == _first_to_act():
		_advance_round()
	else:
		action_position = next


func _first_to_act() -> int:
	if round == BettingRound.PRE_FLOP:
		return _wrap(dealer_position + 3, players.size())
	return _wrap(dealer_position + 1, players.size())


func _advance_round() -> void:
	for p in players:
		p.current_bet = 0
	table_current_bet = 0

	var next_rounds := [BettingRound.FLOP, BettingRound.TURN, BettingRound.RIVER, BettingRound.SHOWDOWN]
	var idx := next_rounds.find(round + 1) if round < BettingRound.SHOWDOWN else -1

	if round < BettingRound.SHOWDOWN:
		round += 1
		if round == BettingRound.SHOWDOWN:
			_determine_winner()
		else:
			action_position = _first_to_act()
	else:
		_determine_winner()


func _determine_winner() -> void:
	game_is_over = true
	round = BettingRound.SHOWDOWN

	# Collect active players and evaluate hands
	var active_hands: Array = []  # [[index, EvaluatedHand], ...]
	for i in range(players.size()):
		var p := players[i]
		if p.status == PokerPlayer.Status.ACTIVE or p.status == PokerPlayer.Status.ALL_IN:
			if p.hole_cards.size() == 2:
				var all_cards: Array = []
				all_cards.append_array(community_cards)
				all_cards.append_array(p.hole_cards)
				var eval_hand := HandEvaluator.evaluate_hand(all_cards)
				active_hands.append([i, eval_hand])

	if active_hands.is_empty():
		# Everyone folded — find last standing
		for p in players:
			if p.status == PokerPlayer.Status.ACTIVE or p.status == PokerPlayer.Status.ALL_IN:
				p.chips += table_pot
				winner_message = "%s wins $%d!" % [p.player_name, table_pot]
				break
	elif active_hands.size() == 1:
		var winner_idx: int = active_hands[0][0]
		players[winner_idx].chips += table_pot
		winner_message = "%s wins $%d!" % [players[winner_idx].player_name, table_pot]
	else:
		# Find best hand
		var best_idx := 0
		for i in range(1, active_hands.size()):
			var cmp: int = active_hands[i][1].compare_to(active_hands[best_idx][1])
			if cmp > 0:
				best_idx = i

		var winner_idx: int = active_hands[best_idx][0]
		var best_hand: HandEvaluator.EvaluatedHand = active_hands[best_idx][1]
		players[winner_idx].chips += table_pot
		winner_message = "%s wins $%d with %s!" % [
			players[winner_idx].player_name, table_pot, best_hand.description()
		]

	SaveManager.record_game_result("poker", players[local_player_index].chips)


func is_local_turn() -> bool:
	return not game_is_over and action_position == local_player_index and players[local_player_index].can_act()


func _process(delta: float) -> void:
	if is_paused:
		return

	# AI turn
	if not game_is_over and not is_local_turn() and players[action_position].can_act():
		ai_timer += delta
		if ai_timer >= ai_delay:
			ai_timer = 0.0
			_do_ai_turn()

	queue_redraw()


func _do_ai_turn() -> void:
	var ai: PokerAI = ai_controllers.get(action_position)
	if not ai:
		return

	var p := players[action_position]
	var visible_community: Array = _visible_community()
	var decision := ai.decide_action(
		p.hole_cards, visible_community,
		table_current_bet, p.current_bet, p.chips, table_pot
	)

	var action_str: String = decision.get("action", "fold")
	var amount: int = decision.get("amount", 0)
	execute_action(action_str, amount)


func _visible_community() -> Array:
	match round:
		BettingRound.PRE_FLOP: return []
		BettingRound.FLOP: return community_cards.slice(0, 3)
		BettingRound.TURN: return community_cards.slice(0, 4)
		_: return community_cards


func _unhandled_input(event: InputEvent) -> void:
	if game_is_over:
		if event.is_action_pressed("interact"):
			# Check if anyone is broke
			var active_count := 0
			for p in players:
				if p.chips > 0:
					active_count += 1
			if active_count >= 2 and players[local_player_index].chips > 0:
				start_hand()
		super._unhandled_input(event)
		return

	super._unhandled_input(event)

	if not is_local_turn() or is_paused:
		return

	# Action selection
	if event.is_action_pressed("move_left"):
		selected_action = _wrap(selected_action - 1, 5) if selected_action > 0 else SelectedAction.ALL_IN
	elif event.is_action_pressed("move_right"):
		selected_action = _wrap(selected_action + 1, 5)

	# Raise amount adjustment
	if selected_action == SelectedAction.RAISE:
		if event.is_action_pressed("move_up"):
			raise_amount = mini(raise_amount + big_blind, players[local_player_index].chips)
		elif event.is_action_pressed("move_down"):
			raise_amount = maxi(raise_amount - big_blind, big_blind)

	# Execute action
	if event.is_action_pressed("interact"):
		_execute_selected()

	# Direct keys
	if event is InputEventKey and event.pressed:
		match event.keycode:
			KEY_F:
				execute_action("fold")
			KEY_C:
				if players[local_player_index].current_bet == table_current_bet:
					execute_action("check")
			KEY_L:
				execute_action("call")
			KEY_R:
				selected_action = SelectedAction.RAISE
			KEY_A:
				execute_action("allin")


func _execute_selected() -> void:
	match selected_action:
		SelectedAction.FOLD:
			execute_action("fold")
		SelectedAction.CHECK:
			if players[local_player_index].current_bet == table_current_bet:
				execute_action("check")
		SelectedAction.CALL:
			execute_action("call")
		SelectedAction.RAISE:
			execute_action("raise", raise_amount)
		SelectedAction.ALL_IN:
			execute_action("allin")


func _wrap(value: int, count: int) -> int:
	if count <= 0:
		return 0
	return ((value % count) + count) % count


# ==================== RENDERING ====================

func _draw() -> void:
	# Background (dark green felt)
	draw_rect(Rect2(0, 0, SCREEN_W, SCREEN_H), Color(0.02, 0.12, 0.05))

	# Table ellipse (felt area)
	var table_center := Vector2(SCREEN_W / 2, SCREEN_H / 2 - 10)
	var table_rx := 250.0
	var table_ry := 120.0
	_draw_ellipse(table_center, table_rx, table_ry, Color(0.05, 0.2, 0.08))
	_draw_ellipse_outline(table_center, table_rx, table_ry, Color(0.3, 0.2, 0.1), 3.0)

	var font := ThemeDB.fallback_font

	# Header
	var round_name := _round_name()
	draw_string(font, Vector2(10, 16), "Hand #%d  |  %s  |  Pot: $%d" % [hand_count, round_name, table_pot],
		HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(0.7, 0.8, 0.7))
	draw_string(font, Vector2(SCREEN_W - 160, 16), "Blinds: $%d/$%d" % [small_blind, big_blind],
		HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(0.6, 0.7, 0.6))

	# Community cards
	_draw_community_cards(font)

	# Players around table
	_draw_players(font)

	# Local player's hole cards (larger, at bottom)
	_draw_hole_cards(font)

	# Actions / status
	if game_is_over:
		_draw_winner(font)
	elif is_local_turn():
		_draw_action_bar(font)
	else:
		draw_string(font, Vector2(SCREEN_W / 2 - 60, SCREEN_H - 12),
			"Waiting...", HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(0.5, 0.6, 0.5))

	# Controls hint
	draw_string(font, Vector2(10, SCREEN_H - 6),
		"Arrows: Select  Enter: Confirm  Q: Quit", HORIZONTAL_ALIGNMENT_LEFT, -1, 8, Color(0.3, 0.4, 0.3))


func _round_name() -> String:
	match round:
		BettingRound.PRE_FLOP: return "Pre-Flop"
		BettingRound.FLOP: return "Flop"
		BettingRound.TURN: return "Turn"
		BettingRound.RIVER: return "River"
		BettingRound.SHOWDOWN: return "Showdown"
	return ""


func _draw_community_cards(font: Font) -> void:
	var num_visible := 0
	match round:
		BettingRound.PRE_FLOP: num_visible = 0
		BettingRound.FLOP: num_visible = 3
		BettingRound.TURN: num_visible = 4
		BettingRound.RIVER, BettingRound.SHOWDOWN: num_visible = 5

	var card_w := 32.0
	var card_h := 44.0
	var spacing := 8.0
	var total_w := 5 * card_w + 4 * spacing
	var start_x := (SCREEN_W - total_w) / 2.0
	var y := SCREEN_H / 2.0 - card_h / 2.0 - 15

	for i in range(5):
		var x := start_x + i * (card_w + spacing)
		if i < num_visible and i < community_cards.size():
			_draw_card_face(Vector2(x, y), card_w, card_h, community_cards[i], font)
		else:
			_draw_card_back(Vector2(x, y), card_w, card_h)


func _draw_card_face(pos: Vector2, w: float, h: float, card: Card, font: Font) -> void:
	# Card background
	draw_rect(Rect2(pos, Vector2(w, h)), Color.WHITE)
	draw_rect(Rect2(pos, Vector2(w, h)), Color(0.3, 0.3, 0.3), false, 1.0)

	var color := Color(0.8, 0.1, 0.1) if card.is_red() else Color(0.1, 0.1, 0.1)
	var text := Card.rank_symbol(card.rank) + Card.suit_symbol(card.suit)
	draw_string(font, pos + Vector2(3, 12), text, HORIZONTAL_ALIGNMENT_LEFT, int(w) - 4, 10, color)
	# Center symbol
	draw_string(font, pos + Vector2(w / 2 - 6, h / 2 + 4), Card.suit_symbol(card.suit),
		HORIZONTAL_ALIGNMENT_LEFT, -1, 16, color)


func _draw_card_back(pos: Vector2, w: float, h: float) -> void:
	draw_rect(Rect2(pos, Vector2(w, h)), Color(0.15, 0.25, 0.6))
	draw_rect(Rect2(pos + Vector2(3, 3), Vector2(w - 6, h - 6)), Color(0.2, 0.35, 0.7))
	draw_rect(Rect2(pos, Vector2(w, h)), Color(0.3, 0.3, 0.5), false, 1.0)


func _draw_players(font: Font) -> void:
	# Position players around the table
	var positions := _get_seat_positions()

	for i in range(players.size()):
		var p := players[i]
		var pos: Vector2 = positions[i]

		# Name
		var name_color := Color(0.3, 1.0, 0.3) if p.is_local else Color(0.8, 0.8, 0.7)
		if i == action_position and not game_is_over:
			name_color = Color.YELLOW  # Action indicator

		var status_icon := ""
		if p.status == PokerPlayer.Status.FOLDED:
			name_color = Color(0.4, 0.4, 0.4)
			status_icon = " (fold)"
		elif p.status == PokerPlayer.Status.ALL_IN:
			name_color = Color(1.0, 0.3, 0.3)
			status_icon = " ALL-IN"
		elif p.status == PokerPlayer.Status.SITTING_OUT:
			name_color = Color(0.3, 0.3, 0.3)
			status_icon = " (out)"

		var dealer_mark := " [D]" if p.is_dealer else ""
		draw_string(font, pos, "%s%s%s" % [p.player_name, dealer_mark, status_icon],
			HORIZONTAL_ALIGNMENT_LEFT, -1, 10, name_color)

		# Chips
		draw_string(font, pos + Vector2(0, 13), "$%d" % p.chips,
			HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(0.9, 0.9, 0.3))

		# Current bet
		if p.current_bet > 0:
			draw_string(font, pos + Vector2(0, 24), "Bet: $%d" % p.current_bet,
				HORIZONTAL_ALIGNMENT_LEFT, -1, 8, Color(0.7, 0.7, 0.5))

		# Show opponent cards at showdown
		if round == BettingRound.SHOWDOWN and not p.is_local and p.hole_cards.size() == 2:
			if p.status != PokerPlayer.Status.FOLDED:
				var card_text := p.hole_cards[0].display() + " " + p.hole_cards[1].display()
				draw_string(font, pos + Vector2(0, 35), card_text,
					HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color.WHITE)


func _get_seat_positions() -> Array[Vector2]:
	# Arrange seats around table: bottom center (local), then clockwise
	var seats: Array[Vector2] = []
	match players.size():
		1:
			seats = [Vector2(280, 270)]
		2:
			seats = [Vector2(280, 270), Vector2(280, 28)]
		3:
			seats = [Vector2(280, 270), Vector2(470, 100), Vector2(80, 100)]
		4:
			seats = [Vector2(280, 270), Vector2(500, 170), Vector2(280, 28), Vector2(60, 170)]
		_:
			# Generic layout for 5-6 players
			seats = [
				Vector2(280, 270),  # local bottom
				Vector2(500, 200),  # right
				Vector2(450, 50),   # top right
				Vector2(120, 50),   # top left
				Vector2(40, 200),   # left
			]
			if players.size() > 5:
				seats.append(Vector2(280, 28))  # top center

	# Pad if needed
	while seats.size() < players.size():
		seats.append(Vector2(320, 180))

	return seats


func _draw_hole_cards(font: Font) -> void:
	var p := players[local_player_index]
	if p.hole_cards.size() < 2:
		return

	var card_w := 38.0
	var card_h := 52.0
	var x := SCREEN_W / 2.0 - card_w - 4
	var y := SCREEN_H - card_h - 28

	_draw_card_face(Vector2(x, y), card_w, card_h, p.hole_cards[0], font)
	_draw_card_face(Vector2(x + card_w + 8, y), card_w, card_h, p.hole_cards[1], font)


func _draw_action_bar(font: Font) -> void:
	var y := SCREEN_H - 22.0
	var x := 120.0

	draw_string(font, Vector2(10, y + 4), "Your turn:", HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color.YELLOW)

	for i in range(5):
		var label: String = ACTION_LABELS[i]
		var color: Color
		if i == selected_action:
			color = Color.WHITE
			# Highlight box
			var tw := font.get_string_size(label, HORIZONTAL_ALIGNMENT_LEFT, -1, 10).x
			draw_rect(Rect2(x - 2, y - 8, tw + 4, 14), Color(0.3, 0.3, 0.5))
		else:
			color = Color(0.5, 0.5, 0.6)
		draw_string(font, Vector2(x, y + 4), label, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, color)
		x += font.get_string_size(label, HORIZONTAL_ALIGNMENT_LEFT, -1, 10).x + 16

	# Show raise amount when raise is selected
	if selected_action == SelectedAction.RAISE:
		draw_string(font, Vector2(x + 10, y + 4), "Amount: $%d (Up/Down)" % raise_amount,
			HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(0.8, 0.8, 0.3))


func _draw_winner(font: Font) -> void:
	# Semi-transparent overlay at bottom
	draw_rect(Rect2(0, SCREEN_H - 60, SCREEN_W, 60), Color(0, 0, 0, 0.7))
	draw_string(font, Vector2(SCREEN_W / 2 - 150, SCREEN_H - 38),
		winner_message, HORIZONTAL_ALIGNMENT_LEFT, 300, 12, Color.YELLOW)
	draw_string(font, Vector2(SCREEN_W / 2 - 80, SCREEN_H - 18),
		"Press ENTER for next hand", HORIZONTAL_ALIGNMENT_LEFT, 200, 10, Color(0.6, 0.7, 0.6))


func _draw_ellipse(center: Vector2, rx: float, ry: float, color: Color) -> void:
	var points := PackedVector2Array()
	for i in range(64):
		var angle := float(i) / 64.0 * TAU
		points.append(center + Vector2(cos(angle) * rx, sin(angle) * ry))
	draw_colored_polygon(points, color)


func _draw_ellipse_outline(center: Vector2, rx: float, ry: float, color: Color, width: float) -> void:
	var points := PackedVector2Array()
	for i in range(65):
		var angle := float(i) / 64.0 * TAU
		points.append(center + Vector2(cos(angle) * rx, sin(angle) * ry))
	draw_polyline(points, color, width)
