class_name HandEvaluator
extends RefCounted

## Hand rankings (lowest to highest)
enum HandRank {
	HIGH_CARD = 0,
	ONE_PAIR = 1,
	TWO_PAIR = 2,
	THREE_OF_A_KIND = 3,
	STRAIGHT = 4,
	FLUSH = 5,
	FULL_HOUSE = 6,
	FOUR_OF_A_KIND = 7,
	STRAIGHT_FLUSH = 8,
	ROYAL_FLUSH = 9,
}


static func hand_rank_name(rank: int) -> String:
	match rank:
		HandRank.HIGH_CARD: return "High Card"
		HandRank.ONE_PAIR: return "One Pair"
		HandRank.TWO_PAIR: return "Two Pair"
		HandRank.THREE_OF_A_KIND: return "Three of a Kind"
		HandRank.STRAIGHT: return "Straight"
		HandRank.FLUSH: return "Flush"
		HandRank.FULL_HOUSE: return "Full House"
		HandRank.FOUR_OF_A_KIND: return "Four of a Kind"
		HandRank.STRAIGHT_FLUSH: return "Straight Flush"
		HandRank.ROYAL_FLUSH: return "Royal Flush"
	return "Unknown"


## Evaluated hand result
class EvaluatedHand:
	var rank: int  # HandRank value
	var kickers: Array[int] = []  # For tie-breaking, highest first
	var cards: Array  # Cards used

	func _init(r: int = 0, k: Array[int] = [], c: Array = []) -> void:
		rank = r
		kickers = k
		cards = c

	func description() -> String:
		match rank:
			HandRank.ROYAL_FLUSH:
				return "Royal Flush"
			HandRank.STRAIGHT_FLUSH:
				return "Straight Flush, %s high" % Card.rank_name(_kicker(0))
			HandRank.FOUR_OF_A_KIND:
				return "Four of a Kind, %ss" % Card.rank_name(_kicker(0))
			HandRank.FULL_HOUSE:
				return "Full House, %ss full of %ss" % [Card.rank_name(_kicker(0)), Card.rank_name(_kicker(1))]
			HandRank.FLUSH:
				return "Flush, %s high" % Card.rank_name(_kicker(0))
			HandRank.STRAIGHT:
				return "Straight, %s high" % Card.rank_name(_kicker(0))
			HandRank.THREE_OF_A_KIND:
				return "Three of a Kind, %ss" % Card.rank_name(_kicker(0))
			HandRank.TWO_PAIR:
				return "Two Pair, %ss and %ss" % [Card.rank_name(_kicker(0)), Card.rank_name(_kicker(1))]
			HandRank.ONE_PAIR:
				return "Pair of %ss" % Card.rank_name(_kicker(0))
			HandRank.HIGH_CARD:
				return "%s high" % Card.rank_name(_kicker(0))
		return "Unknown"

	func _kicker(idx: int) -> int:
		if idx < kickers.size():
			return kickers[idx]
		return 2

	## Compare: returns >0 if self is better, <0 if other is better, 0 if tie
	func compare_to(other: EvaluatedHand) -> int:
		if rank != other.rank:
			return rank - other.rank
		# Compare kickers
		for i in range(mini(kickers.size(), other.kickers.size())):
			if kickers[i] != other.kickers[i]:
				return kickers[i] - other.kickers[i]
		return 0


## Evaluate the best 5-card hand from up to 7 cards
static func evaluate_hand(cards: Array) -> EvaluatedHand:
	if cards.size() < 5:
		var sorted: Array[int] = []
		for c in cards:
			sorted.append(c.rank)
		sorted.sort()
		sorted.reverse()
		return EvaluatedHand.new(HandRank.HIGH_CARD, sorted, cards)

	var combos := _combinations_5(cards)
	var best: EvaluatedHand = null

	for combo in combos:
		var hand := _evaluate_5_cards(combo)
		if best == null or hand.compare_to(best) > 0:
			best = hand

	return best


## Evaluate exactly 5 cards
static func _evaluate_5_cards(cards: Array) -> EvaluatedHand:
	# Sort by rank descending
	var sorted := cards.duplicate()
	sorted.sort_custom(func(a, b): return a.rank > b.rank)

	var is_flush := _check_flush(sorted)
	var straight_high := _check_straight(sorted)

	# Count ranks
	var rank_counts: Dictionary = {}
	for card in sorted:
		rank_counts[card.rank] = rank_counts.get(card.rank, 0) + 1

	# Sort by count desc, then rank desc
	var counts: Array = []
	for r in rank_counts:
		counts.append([r, rank_counts[r]])
	counts.sort_custom(func(a, b):
		if a[1] != b[1]:
			return a[1] > b[1]
		return a[0] > b[0]
	)

	var c0_rank: int = counts[0][0]
	var c0_count: int = counts[0][1]
	var c1_rank: int = counts[1][0] if counts.size() > 1 else 0
	var c1_count: int = counts[1][1] if counts.size() > 1 else 0
	var c2_rank: int = counts[2][0] if counts.size() > 2 else 0

	var hand_rank: int
	var kickers: Array[int] = []

	if is_flush and straight_high == 14:
		hand_rank = HandRank.ROYAL_FLUSH
		kickers = [14, 13, 12, 11, 10]
	elif is_flush and straight_high > 0:
		hand_rank = HandRank.STRAIGHT_FLUSH
		kickers = [straight_high]
	elif c0_count == 4:
		hand_rank = HandRank.FOUR_OF_A_KIND
		kickers = [c0_rank, c1_rank]
	elif c0_count == 3 and c1_count == 2:
		hand_rank = HandRank.FULL_HOUSE
		kickers = [c0_rank, c1_rank]
	elif is_flush:
		hand_rank = HandRank.FLUSH
		for card in sorted:
			kickers.append(card.rank)
	elif straight_high > 0:
		hand_rank = HandRank.STRAIGHT
		kickers = [straight_high]
	elif c0_count == 3:
		hand_rank = HandRank.THREE_OF_A_KIND
		kickers = [c0_rank]
		for entry in counts:
			if entry[1] == 1:
				kickers.append(entry[0])
	elif c0_count == 2 and c1_count == 2:
		hand_rank = HandRank.TWO_PAIR
		kickers = [c0_rank, c1_rank, c2_rank]
	elif c0_count == 2:
		hand_rank = HandRank.ONE_PAIR
		kickers = [c0_rank]
		for entry in counts:
			if entry[1] == 1:
				kickers.append(entry[0])
	else:
		hand_rank = HandRank.HIGH_CARD
		for card in sorted:
			kickers.append(card.rank)

	return EvaluatedHand.new(hand_rank, kickers, sorted)


## Check if all cards are the same suit
static func _check_flush(cards: Array) -> bool:
	if cards.is_empty():
		return false
	var suit: int = cards[0].suit
	for card in cards:
		if card.suit != suit:
			return false
	return true


## Check for a straight, returns high card value (0 if no straight)
static func _check_straight(cards: Array) -> int:
	var values: Array[int] = []
	for card in cards:
		if card.rank not in values:
			values.append(card.rank)
	values.sort()
	values.reverse()

	if values.size() < 5:
		return 0

	# Check regular straight
	for i in range(values.size() - 4):
		if values[i] - values[i + 4] == 4:
			return values[i]

	# Check wheel (A-2-3-4-5)
	if 14 in values and 5 in values and 4 in values and 3 in values and 2 in values:
		return 5  # 5-high straight

	return 0


## Generate all C(n,5) combinations
static func _combinations_5(cards: Array) -> Array:
	var n := cards.size()
	if n < 5:
		return []
	if n == 5:
		return [cards]

	var result: Array = []
	var indices := [0, 1, 2, 3, 4]

	while true:
		var combo: Array = []
		for idx in indices:
			combo.append(cards[idx])
		result.append(combo)

		# Find rightmost index that can be incremented
		var i := 4
		while i >= 0 and indices[i] == n - 5 + i:
			i -= 1

		if i < 0:
			break

		indices[i] += 1
		for j in range(i + 1, 5):
			indices[j] = indices[j - 1] + 1

	return result
