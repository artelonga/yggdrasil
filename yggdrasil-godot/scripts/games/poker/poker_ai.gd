class_name PokerAI
extends RefCounted

## Simple AI that decides actions based on hand strength and randomness

enum Style { TIGHT, LOOSE, AGGRESSIVE }

var style: int


func _init(s: int = Style.TIGHT) -> void:
	style = s


func decide_action(
	hole_cards: Array[Card],
	community_cards: Array,
	current_bet: int,
	player_bet: int,
	player_chips: int,
	pot: int,
) -> Dictionary:
	# Evaluate current hand strength
	var all_cards: Array = []
	all_cards.append_array(community_cards)
	all_cards.append_array(hole_cards)

	var hand_strength := 0.0

	if all_cards.size() >= 5:
		var eval_hand := HandEvaluator.evaluate_hand(all_cards)
		hand_strength = float(eval_hand.rank) / 9.0  # 0.0 to 1.0
	else:
		# Pre-flop: evaluate hole cards
		hand_strength = _preflop_strength(hole_cards)

	var call_amount := current_bet - player_bet
	var roll := randf()

	# Adjust thresholds by style
	var fold_threshold := 0.3
	var raise_threshold := 0.6
	match style:
		Style.LOOSE:
			fold_threshold = 0.15
			raise_threshold = 0.5
		Style.AGGRESSIVE:
			fold_threshold = 0.25
			raise_threshold = 0.45

	# Decision logic
	if call_amount == 0:
		# Can check
		if hand_strength > raise_threshold or (hand_strength > 0.4 and roll < 0.2):
			var raise_amt := _calc_raise(current_bet, player_chips, pot, hand_strength)
			return {"action": "raise", "amount": raise_amt}
		return {"action": "check"}
	else:
		# Must call or fold
		if hand_strength < fold_threshold and roll > 0.15:
			return {"action": "fold"}
		elif hand_strength > raise_threshold and roll < 0.5:
			var raise_amt := _calc_raise(current_bet, player_chips, pot, hand_strength)
			return {"action": "raise", "amount": raise_amt}
		elif call_amount >= player_chips:
			if hand_strength > 0.5:
				return {"action": "allin"}
			return {"action": "fold"}
		else:
			return {"action": "call"}


func _preflop_strength(hole_cards: Array[Card]) -> float:
	if hole_cards.size() < 2:
		return 0.0

	var r1: int = hole_cards[0].rank
	var r2: int = hole_cards[1].rank
	var high := maxi(r1, r2)
	var low := mini(r1, r2)
	var paired := r1 == r2
	var suited := hole_cards[0].suit == hole_cards[1].suit

	var strength := 0.0

	# Pocket pairs
	if paired:
		strength = 0.4 + float(high - 2) / 24.0  # AA = ~0.9, 22 = ~0.4
	else:
		# High cards
		strength = float(high + low - 4) / 24.0
		if suited:
			strength += 0.05
		# Connected cards bonus
		if high - low <= 2:
			strength += 0.05

	return clampf(strength, 0.0, 1.0)


func _calc_raise(current_bet: int, chips: int, pot: int, strength: float) -> int:
	var base := current_bet
	if base == 0:
		base = 20  # min raise = big blind default

	var multiplier := 1.0 + strength * 2.0  # 1x to 3x
	var raise_amt := int(float(base) * multiplier)
	raise_amt = clampi(raise_amt, base, chips)
	return raise_amt
