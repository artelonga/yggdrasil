class_name Deck
extends RefCounted

var cards: Array[Card] = []
var dealt: int = 0


func _init(shuffled: bool = true) -> void:
	_build()
	if shuffled:
		shuffle_deck()


func _build() -> void:
	cards.clear()
	dealt = 0
	for s in [Card.Suit.HEARTS, Card.Suit.DIAMONDS, Card.Suit.CLUBS, Card.Suit.SPADES]:
		for r in range(2, 15):
			cards.append(Card.new(r, s))


func shuffle_deck() -> void:
	dealt = 0
	for i in range(cards.size() - 1, 0, -1):
		var j := randi() % (i + 1)
		var tmp := cards[i]
		cards[i] = cards[j]
		cards[j] = tmp


func deal() -> Card:
	if dealt >= cards.size():
		return null
	var card := cards[dealt]
	dealt += 1
	return card


func deal_n(n: int) -> Array[Card]:
	var result: Array[Card] = []
	for i in range(n):
		var c := deal()
		if c:
			result.append(c)
	return result


func burn() -> void:
	deal()


func remaining() -> int:
	return cards.size() - dealt


func reset() -> void:
	shuffle_deck()
