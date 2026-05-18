class_name Card
extends RefCounted

enum Suit { HEARTS, DIAMONDS, CLUBS, SPADES }
enum Rank {
	TWO = 2, THREE = 3, FOUR = 4, FIVE = 5, SIX = 6, SEVEN = 7,
	EIGHT = 8, NINE = 9, TEN = 10, JACK = 11, QUEEN = 12, KING = 13, ACE = 14
}

var rank: int
var suit: int


func _init(r: int = Rank.TWO, s: int = Suit.HEARTS) -> void:
	rank = r
	suit = s


static func suit_symbol(s: int) -> String:
	match s:
		Suit.HEARTS: return "♥"
		Suit.DIAMONDS: return "♦"
		Suit.CLUBS: return "♣"
		Suit.SPADES: return "♠"
	return "?"


static func rank_symbol(r: int) -> String:
	match r:
		2: return "2"
		3: return "3"
		4: return "4"
		5: return "5"
		6: return "6"
		7: return "7"
		8: return "8"
		9: return "9"
		10: return "10"
		Rank.JACK: return "J"
		Rank.QUEEN: return "Q"
		Rank.KING: return "K"
		Rank.ACE: return "A"
	return "?"


static func rank_name(r: int) -> String:
	match r:
		2: return "Dois"
		3: return "Três"
		4: return "Quatro"
		5: return "Cinco"
		6: return "Seis"
		7: return "Sete"
		8: return "Oito"
		9: return "Nove"
		10: return "Dez"
		11: return "Valete"
		12: return "Rainha"
		13: return "Rei"
		14: return "Ás"
	return "?"


func is_red() -> bool:
	return suit == Suit.HEARTS or suit == Suit.DIAMONDS


func display() -> String:
	return rank_symbol(rank) + suit_symbol(suit)
