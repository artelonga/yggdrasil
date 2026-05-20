//! universe-poker — Full Texas Hold'em for 3 players (1 human + 2 bots).
//!
//! State is persisted to the host KV store ("state" key) so it survives WASM
//! module restarts.
//!
//! Actions (input JSON field "action"):
//!   "start"  — start a new hand (required on first call or after round="done")
//!   "fold"   — human folds
//!   "call"   — human matches current_bet
//!   "check"  — human stays without paying (only valid when bet==current_bet)
//!   "raise"  — human raises; optional "amount" field (default 100)

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// xorshift64 RNG
// ---------------------------------------------------------------------------

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    if x == 0 {
        x = 0xdeadbeef_cafebabe;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

// ---------------------------------------------------------------------------
// Card + Deck
// ---------------------------------------------------------------------------

/// A standard playing card.
/// rank: 2-14 (14=Ace), suit: 0=Clubs 1=Diamonds 2=Hearts 3=Spades
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
struct Card {
    rank: u8,
    suit: u8,
}

fn build_deck() -> Vec<Card> {
    let mut deck = Vec::with_capacity(52);
    for suit in 0u8..4 {
        for rank in 2u8..=14 {
            deck.push(Card { rank, suit });
        }
    }
    deck
}

fn shuffle(deck: &mut [Card], rng: &mut u64) {
    let n = deck.len();
    for i in (1..n).rev() {
        let j = (xorshift64(rng) as usize) % (i + 1);
        deck.swap(i, j);
    }
}

// ---------------------------------------------------------------------------
// Hand evaluation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum HandRank {
    HighCard = 1,
    OnePair = 2,
    TwoPair = 3,
    ThreeOfAKind = 4,
    Straight = 5,
    Flush = 6,
    FullHouse = 7,
    FourOfAKind = 8,
    StraightFlush = 9,
}

/// A scored hand for comparison: (rank, tiebreak values).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScoredHand {
    rank: HandRank,
    /// Tiebreaker values in descending significance order.
    tiebreak: Vec<u8>,
}

impl PartialOrd for ScoredHand {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredHand {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.rank.cmp(&other.rank) {
            std::cmp::Ordering::Equal => self.tiebreak.cmp(&other.tiebreak),
            o => o,
        }
    }
}

/// Evaluate the best 5-card hand from up to 7 cards.
fn best_hand(cards: &[Card]) -> ScoredHand {
    // Generate all C(n,5) combinations and pick the best.
    let n = cards.len();
    let mut best: Option<ScoredHand> = None;

    if n < 5 {
        // Not enough cards — return high card of what we have.
        let mut ranks: Vec<u8> = cards.iter().map(|c| c.rank).collect();
        ranks.sort_unstable_by(|a, b| b.cmp(a));
        return ScoredHand {
            rank: HandRank::HighCard,
            tiebreak: ranks,
        };
    }

    let combos = combinations(n, 5);
    for combo in combos {
        let hand: Vec<Card> = combo.iter().map(|&i| cards[i]).collect();
        let scored = evaluate_five(&hand);
        if best.as_ref().is_none_or(|b| scored > *b) {
            best = Some(scored);
        }
    }

    best.unwrap()
}

/// Generate all index combinations of size k from n elements.
fn combinations(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut result = vec![];
    let mut combo = vec![0usize; k];
    let mut i = 0usize;
    // Initialize
    for (j, item) in combo.iter_mut().enumerate().take(k) {
        *item = j;
    }
    loop {
        result.push(combo.clone());
        // Find rightmost element that can be incremented
        let mut j = k;
        loop {
            if j == 0 {
                return result;
            }
            j -= 1;
            if combo[j] < n - k + j {
                break;
            }
        }
        combo[j] += 1;
        for l in (j + 1)..k {
            combo[l] = combo[l - 1] + 1;
        }
        i += 1;
        if i > 500 {
            // Safety valve (C(7,5)=21, should never hit)
            break;
        }
    }
    result
}

/// Evaluate exactly 5 cards.
fn evaluate_five(cards: &[Card]) -> ScoredHand {
    debug_assert_eq!(cards.len(), 5);

    let mut ranks: Vec<u8> = cards.iter().map(|c| c.rank).collect();
    ranks.sort_unstable_by(|a, b| b.cmp(a));

    let suits: Vec<u8> = cards.iter().map(|c| c.suit).collect();
    let is_flush = suits.windows(2).all(|w| w[0] == w[1]);

    // Straight detection (including A-low: A,2,3,4,5)
    let is_straight = check_straight(&ranks);

    // Count frequencies
    let mut freq = [0u8; 15]; // index by rank (2-14)
    for &r in &ranks {
        freq[r as usize] += 1;
    }

    // Groups: sorted by (count desc, rank desc)
    let mut groups: Vec<(u8, u8)> = (2..=14u8)
        .filter(|&r| freq[r as usize] > 0)
        .map(|r| (freq[r as usize], r))
        .collect();
    groups.sort_unstable_by(|a, b| b.cmp(a));

    let counts: Vec<u8> = groups.iter().map(|&(c, _)| c).collect();
    let group_ranks: Vec<u8> = groups.iter().map(|&(_, r)| r).collect();

    if is_straight && is_flush {
        // Ace-low straight flush: A,2,3,4,5
        if ranks == [14, 5, 4, 3, 2] {
            return ScoredHand {
                rank: HandRank::StraightFlush,
                tiebreak: vec![5],
            };
        }
        return ScoredHand {
            rank: HandRank::StraightFlush,
            tiebreak: vec![ranks[0]],
        };
    }

    if counts[0] == 4 {
        return ScoredHand {
            rank: HandRank::FourOfAKind,
            tiebreak: group_ranks.clone(),
        };
    }

    if counts[0] == 3 && counts.get(1).copied().unwrap_or(0) == 2 {
        return ScoredHand {
            rank: HandRank::FullHouse,
            tiebreak: group_ranks.clone(),
        };
    }

    if is_flush {
        return ScoredHand {
            rank: HandRank::Flush,
            tiebreak: ranks.clone(),
        };
    }

    if is_straight {
        if ranks == [14, 5, 4, 3, 2] {
            return ScoredHand {
                rank: HandRank::Straight,
                tiebreak: vec![5],
            };
        }
        return ScoredHand {
            rank: HandRank::Straight,
            tiebreak: vec![ranks[0]],
        };
    }

    if counts[0] == 3 {
        return ScoredHand {
            rank: HandRank::ThreeOfAKind,
            tiebreak: group_ranks.clone(),
        };
    }

    if counts[0] == 2 && counts.get(1).copied().unwrap_or(0) == 2 {
        return ScoredHand {
            rank: HandRank::TwoPair,
            tiebreak: group_ranks.clone(),
        };
    }

    if counts[0] == 2 {
        return ScoredHand {
            rank: HandRank::OnePair,
            tiebreak: group_ranks.clone(),
        };
    }

    ScoredHand {
        rank: HandRank::HighCard,
        tiebreak: ranks.clone(),
    }
}

/// Check if sorted-descending ranks form a straight.
fn check_straight(ranks: &[u8]) -> bool {
    if ranks.len() != 5 {
        return false;
    }
    // Normal straight
    if ranks.windows(2).all(|w| w[0] == w[1] + 1) {
        return true;
    }
    // Ace-low: A,2,3,4,5 → sorted desc [14,5,4,3,2]
    ranks == [14, 5, 4, 3, 2]
}

// ---------------------------------------------------------------------------
// Bot hand-strength estimator (hole cards only)
// ---------------------------------------------------------------------------

fn hand_strength_estimate(hole: &[Card]) -> f32 {
    if hole.len() < 2 {
        return 0.3;
    }
    let c0 = hole[0];
    let c1 = hole[1];

    // Pair
    if c0.rank == c1.rank {
        return 0.8;
    }

    let suited = c0.suit == c1.suit;
    let connected = c0.rank.abs_diff(c1.rank) == 1;
    let high = c0.rank >= 10 || c1.rank >= 10;

    if suited && connected {
        return 0.7;
    }
    if high {
        return 0.6;
    }
    0.3
}

// ---------------------------------------------------------------------------
// Game state structs
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
struct PokerPlayer {
    name: String,
    chips: u32,
    hand: Vec<Card>,
    folded: bool,
    bet: u32,
    is_all_in: bool,
    is_human: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct PokerState {
    players: Vec<PokerPlayer>,
    deck: Vec<Card>,
    deck_pos: usize,
    community: Vec<Card>,
    pot: u32,
    current_bet: u32,
    dealer: usize,
    current_player: usize,
    round: String,
    is_over: bool,
    winner_idx: Option<usize>,
    winner_message: Option<String>,
    actions_this_round: usize,
    active_count: usize,
}

const SMALL_BLIND: u32 = 50;
const BIG_BLIND: u32 = 100;
const PLAYER_COUNT: usize = 3;

impl PokerState {
    fn new_game() -> Self {
        let players = vec![
            PokerPlayer {
                name: "Você".to_string(),
                chips: 1000,
                hand: vec![],
                folded: false,
                bet: 0,
                is_all_in: false,
                is_human: true,
            },
            PokerPlayer {
                name: "Bot Alice".to_string(),
                chips: 1000,
                hand: vec![],
                folded: false,
                bet: 0,
                is_all_in: false,
                is_human: false,
            },
            PokerPlayer {
                name: "Bot Bob".to_string(),
                chips: 1000,
                hand: vec![],
                folded: false,
                bet: 0,
                is_all_in: false,
                is_human: false,
            },
        ];

        PokerState {
            players,
            deck: vec![],
            deck_pos: 0,
            community: vec![],
            pot: 0,
            current_bet: 0,
            dealer: 0,
            current_player: 0,
            round: "waiting".to_string(),
            is_over: false,
            winner_idx: None,
            winner_message: None,
            actions_this_round: 0,
            active_count: PLAYER_COUNT,
        }
    }

    fn deal_card(&mut self) -> Option<Card> {
        if self.deck_pos < self.deck.len() {
            let c = self.deck[self.deck_pos];
            self.deck_pos += 1;
            Some(c)
        } else {
            None
        }
    }

    /// Start a new hand: shuffle, post blinds, deal hole cards, advance to pre-flop.
    fn start_hand(&mut self) {
        // Rotate dealer
        // (If it's the very first hand, dealer stays at 0)
        // Reset per-hand state
        for p in self.players.iter_mut() {
            p.hand.clear();
            p.folded = false;
            p.bet = 0;
            p.is_all_in = false;
        }
        self.community.clear();
        self.pot = 0;
        self.current_bet = 0;
        self.is_over = false;
        self.winner_idx = None;
        self.winner_message = None;
        self.actions_this_round = 0;
        self.active_count = PLAYER_COUNT;

        // Build and shuffle deck
        let mut rng = universe_sdk::get_random();
        if rng == 0 {
            rng = 0xdeadbeef_cafebabe;
        }
        let mut deck = build_deck();
        shuffle(&mut deck, &mut rng);
        self.deck = deck;
        self.deck_pos = 0;

        // Post blinds — SB = player after dealer, BB = player after SB
        let sb_idx = (self.dealer + 1) % PLAYER_COUNT;
        let bb_idx = (self.dealer + 2) % PLAYER_COUNT;

        self.post_blind(sb_idx, SMALL_BLIND);
        self.post_blind(bb_idx, BIG_BLIND);
        self.current_bet = BIG_BLIND;

        // Deal 2 cards to each player (starting from player after dealer)
        for offset in 1..=(PLAYER_COUNT * 2) {
            let p_idx = (self.dealer + offset) % PLAYER_COUNT;
            if let Some(card) = self.deal_card() {
                self.players[p_idx].hand.push(card);
            }
        }

        // First to act pre-flop: player after BB
        self.current_player = (bb_idx + 1) % PLAYER_COUNT;
        self.round = "pre-flop".to_string();
    }

    fn post_blind(&mut self, idx: usize, amount: u32) {
        let actual = self.players[idx].chips.min(amount);
        self.players[idx].chips -= actual;
        self.players[idx].bet += actual;
        self.pot += actual;
        if self.players[idx].chips == 0 {
            self.players[idx].is_all_in = true;
        }
    }

    /// Apply a player action. Returns an error string on invalid action.
    fn apply_action(
        &mut self,
        player_idx: usize,
        action: &str,
        raise_amount: u32,
    ) -> Result<(), &'static str> {
        let player = &self.players[player_idx];
        if player.folded {
            return Err("player already folded");
        }

        match action {
            "fold" => {
                self.players[player_idx].folded = true;
                self.active_count = self.players.iter().filter(|p| !p.folded).count();
            }
            "call" => {
                let to_call = self
                    .current_bet
                    .saturating_sub(self.players[player_idx].bet);
                let actual = self.players[player_idx].chips.min(to_call);
                self.players[player_idx].chips -= actual;
                self.players[player_idx].bet += actual;
                self.pot += actual;
                if self.players[player_idx].chips == 0 {
                    self.players[player_idx].is_all_in = true;
                }
            }
            "check" => {
                if self.players[player_idx].bet < self.current_bet {
                    return Err("cannot check, must call or fold");
                }
                // No chips change
            }
            "raise" => {
                // First, call the current bet
                let to_call = self
                    .current_bet
                    .saturating_sub(self.players[player_idx].bet);
                let call_actual = self.players[player_idx].chips.min(to_call);
                self.players[player_idx].chips -= call_actual;
                self.players[player_idx].bet += call_actual;
                self.pot += call_actual;

                // Then raise
                let raise_actual = self.players[player_idx].chips.min(raise_amount);
                if raise_actual > 0 {
                    self.players[player_idx].chips -= raise_actual;
                    self.players[player_idx].bet += raise_actual;
                    self.pot += raise_actual;
                    self.current_bet = self.players[player_idx].bet;
                    // Reset actions so others can respond
                    self.actions_this_round = 0;
                }
                if self.players[player_idx].chips == 0 {
                    self.players[player_idx].is_all_in = true;
                }
            }
            _ => return Err("unknown action"),
        }

        self.actions_this_round += 1;
        Ok(())
    }

    /// Advance current_player to the next non-folded, non-all-in player.
    /// Returns true if we should advance the round (all active players have acted
    /// and matched the current bet).
    fn advance_player(&mut self) -> bool {
        // Check if all active (non-folded) players have matched current_bet
        let all_matched = self
            .players
            .iter()
            .all(|p| p.folded || p.is_all_in || p.bet >= self.current_bet);

        // Also need everyone to have had a chance to act
        let active_non_all_in = self
            .players
            .iter()
            .filter(|p| !p.folded && !p.is_all_in)
            .count();

        if all_matched && self.actions_this_round >= active_non_all_in {
            return true; // Advance round
        }

        // Move to next non-folded, non-all-in player
        let start = self.current_player;
        let mut next = (self.current_player + 1) % PLAYER_COUNT;
        while next != start {
            if !self.players[next].folded && !self.players[next].is_all_in {
                self.current_player = next;
                return false;
            }
            next = (next + 1) % PLAYER_COUNT;
        }
        // All other players are folded/all-in — advance round
        true
    }

    /// Advance the game to the next round (flop → turn → river → showdown).
    fn next_round(&mut self) {
        // Reset per-round bet tracking
        for p in self.players.iter_mut() {
            p.bet = 0;
        }
        self.current_bet = 0;
        self.actions_this_round = 0;

        // First active player after dealer acts first post-flop
        let mut first = (self.dealer + 1) % PLAYER_COUNT;
        for _ in 0..PLAYER_COUNT {
            if !self.players[first].folded {
                break;
            }
            first = (first + 1) % PLAYER_COUNT;
        }
        self.current_player = first;

        match self.round.as_str() {
            "pre-flop" => {
                // Deal flop: burn 1, reveal 3
                let _ = self.deal_card(); // burn
                for _ in 0..3 {
                    if let Some(c) = self.deal_card() {
                        self.community.push(c);
                    }
                }
                self.round = "flop".to_string();
            }
            "flop" => {
                let _ = self.deal_card(); // burn
                if let Some(c) = self.deal_card() {
                    self.community.push(c);
                }
                self.round = "turn".to_string();
            }
            "turn" => {
                let _ = self.deal_card(); // burn
                if let Some(c) = self.deal_card() {
                    self.community.push(c);
                }
                self.round = "river".to_string();
            }
            "river" => {
                self.round = "showdown".to_string();
                self.do_showdown();
            }
            _ => {
                self.round = "done".to_string();
                self.is_over = true;
            }
        }

        // If showdown was triggered above, is_over is already set
    }

    fn do_showdown(&mut self) {
        // Find best hand among non-folded players
        let mut best_score: Option<ScoredHand> = None;
        let mut winner = 0usize;

        for (i, p) in self.players.iter().enumerate() {
            if p.folded {
                continue;
            }
            let mut all_cards = self.community.clone();
            all_cards.extend_from_slice(&p.hand);
            let score = best_hand(&all_cards);
            if best_score.as_ref().is_none_or(|b: &ScoredHand| score > *b) {
                best_score = Some(score);
                winner = i;
            }
        }

        // Award pot to winner
        self.players[winner].chips += self.pot;
        self.pot = 0;
        self.winner_idx = Some(winner);
        self.winner_message = Some(format!(
            "{} venceu com {:?}!",
            self.players[winner].name,
            best_score.map(|s| s.rank).unwrap_or(HandRank::HighCard)
        ));

        // Rotate dealer for next hand
        self.dealer = (self.dealer + 1) % PLAYER_COUNT;
        self.round = "done".to_string();
        self.is_over = true;
    }

    /// Early showdown when only one player remains active.
    fn early_showdown(&mut self) {
        let winner = self
            .players
            .iter()
            .enumerate()
            .find(|(_, p)| !p.folded)
            .map(|(i, _)| i)
            .unwrap_or(0);

        self.players[winner].chips += self.pot;
        self.pot = 0;
        self.winner_idx = Some(winner);
        self.winner_message = Some(format!(
            "{} venceu (todos os outros desistiram)!",
            self.players[winner].name
        ));
        self.dealer = (self.dealer + 1) % PLAYER_COUNT;
        self.round = "done".to_string();
        self.is_over = true;
    }

    /// Available actions for the human player.
    fn available_actions(&self) -> Vec<&'static str> {
        if self.is_over || self.current_player != 0 || self.players[0].folded {
            return vec![];
        }
        let player = &self.players[0];
        let mut actions = vec!["fold"];
        if player.bet >= self.current_bet {
            actions.push("check");
        } else {
            actions.push("call");
        }
        if player.chips > 0 {
            actions.push("raise");
        }
        actions
    }
}

// ---------------------------------------------------------------------------
// Bot logic
// ---------------------------------------------------------------------------

fn bot_action(state: &PokerState, bot_idx: usize) -> (&'static str, u32) {
    let strength = hand_strength_estimate(&state.players[bot_idx].hand);
    let to_call = state.current_bet.saturating_sub(state.players[bot_idx].bet);

    if strength >= 0.7 {
        ("raise", 100)
    } else if strength >= 0.4 {
        if to_call == 0 {
            ("check", 0)
        } else {
            ("call", 0)
        }
    } else {
        // Fold only if there's a cost, otherwise check
        if to_call > 0 {
            ("fold", 0)
        } else {
            ("check", 0)
        }
    }
}

// ---------------------------------------------------------------------------
// Output JSON (bots' hands hidden)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PlayerOutput<'a> {
    name: &'a str,
    chips: u32,
    /// Empty for bots.
    hand: Vec<Card>,
    folded: bool,
    bet: u32,
    is_all_in: bool,
    is_human: bool,
}

#[derive(Serialize)]
struct OutputState<'a> {
    players: Vec<PlayerOutput<'a>>,
    community: &'a [Card],
    pot: u32,
    current_bet: u32,
    dealer: usize,
    current_player: usize,
    round: &'a str,
    is_over: bool,
    winner_idx: Option<usize>,
    winner_message: Option<&'a str>,
    available_actions: Vec<&'static str>,
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

struct PokerSession {
    state: PokerState,
}

impl Default for PokerSession {
    fn default() -> Self {
        let state = universe_sdk::kv_load("state")
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(PokerState::new_game);
        Self { state }
    }
}

impl PokerSession {
    fn save(&self) {
        if let Ok(bytes) = serde_json::to_vec(&self.state) {
            universe_sdk::kv_store("state", &bytes);
        }
    }

    fn build_output(&self) -> String {
        let s = &self.state;
        let players: Vec<PlayerOutput<'_>> = s
            .players
            .iter()
            .map(|p| PlayerOutput {
                name: &p.name,
                chips: p.chips,
                hand: if p.is_human || s.is_over {
                    p.hand.clone()
                } else {
                    vec![]
                },
                folded: p.folded,
                bet: p.bet,
                is_all_in: p.is_all_in,
                is_human: p.is_human,
            })
            .collect();

        let out = OutputState {
            players,
            community: &s.community,
            pot: s.pot,
            current_bet: s.current_bet,
            dealer: s.dealer,
            current_player: s.current_player,
            round: &s.round,
            is_over: s.is_over,
            winner_idx: s.winner_idx,
            winner_message: s.winner_message.as_deref(),
            available_actions: s.available_actions(),
        };

        serde_json::to_string(&out).unwrap_or_else(|_| "{}".to_string())
    }

    fn process(&mut self, input: &str) -> String {
        let v: serde_json::Value = serde_json::from_str(input).unwrap_or(serde_json::Value::Null);
        let action = v.get("action").and_then(|a| a.as_str()).unwrap_or("");

        match action {
            "start" => {
                if self.state.round == "waiting" || self.state.round == "done" {
                    self.state.start_hand();
                    // Let bots act if they come before the human
                    self.run_bots_if_needed();
                }
            }
            "fold" | "call" | "check" | "raise" => {
                if !self.state.is_over
                    && self.state.current_player == 0
                    && !self.state.players[0].folded
                {
                    let raise_amount = v
                        .get("amount")
                        .and_then(|a| a.as_u64())
                        .map(|n| n as u32)
                        .unwrap_or(BIG_BLIND);

                    if self.state.apply_action(0, action, raise_amount).is_ok() {
                        // Check if only one player left
                        if self.state.active_count <= 1 {
                            self.state.early_showdown();
                        } else if self.state.advance_player() {
                            self.state.next_round();
                            if !self.state.is_over {
                                self.run_bots_if_needed();
                            }
                        } else {
                            self.run_bots_if_needed();
                        }
                    }
                }
            }
            _ => {
                // Unknown / empty — just return current state
            }
        }

        self.save();
        self.build_output()
    }

    /// Run bot turns until it's the human's turn or the round ends.
    fn run_bots_if_needed(&mut self) {
        // Safety limit to prevent infinite loops
        let mut iterations = 0usize;
        loop {
            if self.state.is_over {
                break;
            }
            let cp = self.state.current_player;
            if self.state.players[cp].is_human {
                break;
            }
            if self.state.players[cp].folded || self.state.players[cp].is_all_in {
                // Skip to next
                if self.state.advance_player() {
                    self.state.next_round();
                }
                iterations += 1;
                if iterations > 30 {
                    break;
                }
                continue;
            }

            let (bot_act, raise_amt) = bot_action(&self.state, cp);
            let _ = self.state.apply_action(cp, bot_act, raise_amt);

            if self.state.active_count <= 1 {
                self.state.early_showdown();
                break;
            }

            if self.state.advance_player() {
                self.state.next_round();
                if self.state.is_over {
                    break;
                }
            }

            iterations += 1;
            if iterations > 30 {
                break;
            }
        }
    }
}

universe_sdk::universe_exports!(PokerSession);

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_card(rank: u8, suit: u8) -> Card {
        Card { rank, suit }
    }

    // -----------------------------------------------------------------------
    // Deck + shuffle
    // -----------------------------------------------------------------------

    #[test]
    fn test_deck_has_52_cards() {
        let deck = build_deck();
        assert_eq!(deck.len(), 52);
    }

    #[test]
    fn test_deck_has_no_duplicates() {
        let deck = build_deck();
        let mut seen = std::collections::HashSet::new();
        for c in &deck {
            assert!(seen.insert((c.rank, c.suit)), "duplicate card: {:?}", c);
        }
    }

    #[test]
    fn test_shuffle_changes_order() {
        let original = build_deck();
        let mut deck = original.clone();
        let mut rng = 0xdeadbeef_cafebabeu64;
        shuffle(&mut deck, &mut rng);
        // Very unlikely to be equal after shuffle
        assert_ne!(deck, original);
    }

    // -----------------------------------------------------------------------
    // New game setup
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_game_has_three_players() {
        let state = PokerState::new_game();
        assert_eq!(state.players.len(), 3);
    }

    #[test]
    fn test_new_game_player_chips() {
        let state = PokerState::new_game();
        for p in &state.players {
            assert_eq!(p.chips, 1000);
        }
    }

    #[test]
    fn test_start_hand_posts_blinds() {
        let mut state = PokerState::new_game();
        state.start_hand();

        // Dealer=0: SB=player 1, BB=player 2
        assert_eq!(state.players[1].bet, SMALL_BLIND);
        assert_eq!(state.players[2].bet, BIG_BLIND);
        assert_eq!(state.pot, SMALL_BLIND + BIG_BLIND);
        assert_eq!(state.current_bet, BIG_BLIND);
    }

    #[test]
    fn test_start_hand_deals_two_cards() {
        let mut state = PokerState::new_game();
        state.start_hand();
        for p in &state.players {
            assert_eq!(p.hand.len(), 2, "player {} should have 2 cards", p.name);
        }
    }

    #[test]
    fn test_start_hand_round_is_preflop() {
        let mut state = PokerState::new_game();
        state.start_hand();
        assert_eq!(state.round, "pre-flop");
    }

    #[test]
    fn test_start_hand_first_actor_after_bb() {
        let mut state = PokerState::new_game();
        state.start_hand();
        // Dealer=0, SB=1, BB=2, first actor=0
        assert_eq!(state.current_player, 0);
    }

    // -----------------------------------------------------------------------
    // Player actions
    // -----------------------------------------------------------------------

    #[test]
    fn test_player_fold() {
        let mut state = PokerState::new_game();
        state.start_hand();
        let active_before = state.active_count;
        let _ = state.apply_action(0, "fold", 0);
        assert!(state.players[0].folded);
        assert_eq!(state.active_count, active_before - 1);
    }

    #[test]
    fn test_player_call_reduces_chips() {
        let mut state = PokerState::new_game();
        state.start_hand();
        // Player 0 must call BIG_BLIND (100) to stay in
        let chips_before = state.players[0].chips;
        let _ = state.apply_action(0, "call", 0);
        assert_eq!(state.players[0].chips, chips_before - BIG_BLIND);
    }

    #[test]
    fn test_player_raise_increases_current_bet() {
        let mut state = PokerState::new_game();
        state.start_hand();
        let _ = state.apply_action(0, "raise", 100);
        // After calling BB (100) and raising 100, current_bet = 200
        assert_eq!(state.current_bet, BIG_BLIND + 100);
    }

    #[test]
    fn test_check_invalid_when_debt_exists() {
        let mut state = PokerState::new_game();
        state.start_hand();
        // Player 0 has bet=0, current_bet=100 → check should fail
        let result = state.apply_action(0, "check", 0);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Round advancement
    // -----------------------------------------------------------------------

    #[test]
    fn test_advancing_rounds() {
        let mut state = PokerState::new_game();
        state.start_hand();

        // All three players call to match BB (player 0 calls, player 1 calls, player 2 already matches)
        // actions_this_round resets to 0; need to simulate full round
        // Force all players to call so round advances
        for i in 0..PLAYER_COUNT {
            let to_call = state.current_bet.saturating_sub(state.players[i].bet);
            if to_call > 0 && !state.players[i].folded {
                let _ = state.apply_action(i, "call", 0);
            } else if !state.players[i].folded {
                let _ = state.apply_action(i, "check", 0);
            }
        }

        state.next_round();
        assert_eq!(state.round, "flop");
        assert_eq!(state.community.len(), 3);
    }

    #[test]
    fn test_flop_turn_river_community_cards() {
        let mut state = PokerState::new_game();
        state.start_hand();

        // Force through all rounds
        state.next_round(); // → flop
        assert_eq!(state.community.len(), 3);
        assert_eq!(state.round, "flop");

        state.next_round(); // → turn
        assert_eq!(state.community.len(), 4);
        assert_eq!(state.round, "turn");

        state.next_round(); // → river
        assert_eq!(state.community.len(), 5);
        assert_eq!(state.round, "river");

        state.next_round(); // → showdown / done
        assert!(state.round == "showdown" || state.round == "done");
        assert!(state.winner_idx.is_some());
    }

    // -----------------------------------------------------------------------
    // Showdown
    // -----------------------------------------------------------------------

    #[test]
    fn test_early_showdown_when_others_fold() {
        let mut state = PokerState::new_game();
        state.start_hand();
        let _ = state.apply_action(1, "fold", 0);
        let _ = state.apply_action(2, "fold", 0);
        state.active_count = state.players.iter().filter(|p| !p.folded).count();
        state.early_showdown();
        assert_eq!(state.winner_idx, Some(0));
        assert!(state.is_over);
    }

    #[test]
    fn test_showdown_winner_gets_pot() {
        let mut state = PokerState::new_game();
        state.start_hand();
        let total_pot = state.pot;

        state.next_round(); // flop
        state.next_round(); // turn
        state.next_round(); // river
        state.next_round(); // showdown

        assert!(state.is_over);
        let _winner_idx = state.winner_idx.unwrap();
        // Winner's chips = starting - blind(if any) + pot
        // Just verify total chips are conserved
        let total_chips: u32 = state.players.iter().map(|p| p.chips).sum();
        assert_eq!(
            total_chips, 3000,
            "chip conservation failed: pot was {total_pot}"
        );
    }

    // -----------------------------------------------------------------------
    // Hand evaluation
    // -----------------------------------------------------------------------

    #[test]
    fn test_high_card() {
        let cards = vec![
            make_card(2, 0),
            make_card(5, 1),
            make_card(7, 2),
            make_card(9, 3),
            make_card(11, 0),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::HighCard);
    }

    #[test]
    fn test_one_pair() {
        let cards = vec![
            make_card(5, 0),
            make_card(5, 1),
            make_card(7, 2),
            make_card(9, 3),
            make_card(11, 0),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::OnePair);
    }

    #[test]
    fn test_two_pair() {
        let cards = vec![
            make_card(5, 0),
            make_card(5, 1),
            make_card(7, 2),
            make_card(7, 3),
            make_card(11, 0),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::TwoPair);
    }

    #[test]
    fn test_three_of_a_kind() {
        let cards = vec![
            make_card(5, 0),
            make_card(5, 1),
            make_card(5, 2),
            make_card(7, 3),
            make_card(11, 0),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::ThreeOfAKind);
    }

    #[test]
    fn test_straight() {
        let cards = vec![
            make_card(5, 0),
            make_card(6, 1),
            make_card(7, 2),
            make_card(8, 3),
            make_card(9, 0),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::Straight);
    }

    #[test]
    fn test_flush() {
        let cards = vec![
            make_card(2, 0),
            make_card(5, 0),
            make_card(7, 0),
            make_card(9, 0),
            make_card(11, 0),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::Flush);
    }

    #[test]
    fn test_full_house() {
        let cards = vec![
            make_card(5, 0),
            make_card(5, 1),
            make_card(5, 2),
            make_card(7, 3),
            make_card(7, 0),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::FullHouse);
    }

    #[test]
    fn test_four_of_a_kind() {
        let cards = vec![
            make_card(5, 0),
            make_card(5, 1),
            make_card(5, 2),
            make_card(5, 3),
            make_card(7, 0),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::FourOfAKind);
    }

    #[test]
    fn test_straight_flush() {
        let cards = vec![
            make_card(5, 0),
            make_card(6, 0),
            make_card(7, 0),
            make_card(8, 0),
            make_card(9, 0),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::StraightFlush);
    }

    #[test]
    fn test_ace_low_straight() {
        let cards = vec![
            make_card(14, 0),
            make_card(2, 1),
            make_card(3, 2),
            make_card(4, 3),
            make_card(5, 0),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::Straight);
        assert_eq!(h.tiebreak[0], 5); // wheel tops at 5
    }

    #[test]
    fn test_best_hand_from_seven_cards() {
        // Royal-flush setup: A K Q J 10 of clubs, plus 2♦ 3♥
        let cards = vec![
            make_card(14, 0),
            make_card(13, 0),
            make_card(12, 0),
            make_card(11, 0),
            make_card(10, 0),
            make_card(2, 1),
            make_card(3, 2),
        ];
        let h = best_hand(&cards);
        assert_eq!(h.rank, HandRank::StraightFlush);
    }

    #[test]
    fn test_hand_rank_ordering() {
        assert!(HandRank::StraightFlush > HandRank::FourOfAKind);
        assert!(HandRank::FourOfAKind > HandRank::FullHouse);
        assert!(HandRank::FullHouse > HandRank::Flush);
        assert!(HandRank::Flush > HandRank::Straight);
        assert!(HandRank::Straight > HandRank::ThreeOfAKind);
        assert!(HandRank::ThreeOfAKind > HandRank::TwoPair);
        assert!(HandRank::TwoPair > HandRank::OnePair);
        assert!(HandRank::OnePair > HandRank::HighCard);
    }

    // -----------------------------------------------------------------------
    // Bot AI
    // -----------------------------------------------------------------------

    #[test]
    fn test_bot_raises_strong_hand() {
        let mut state = PokerState::new_game();
        state.start_hand();
        // Give bot a pair (strength 0.8)
        state.players[1].hand = vec![make_card(10, 0), make_card(10, 1)];
        let (action, _) = bot_action(&state, 1);
        assert_eq!(action, "raise");
    }

    #[test]
    fn test_bot_folds_weak_hand_with_cost() {
        let mut state = PokerState::new_game();
        state.start_hand();
        // Give bot a weak hand (2 and 7 off-suit, strength 0.3)
        state.players[1].hand = vec![make_card(2, 0), make_card(7, 1)];
        state.current_bet = 200; // There's a cost to call
        let (action, _) = bot_action(&state, 1);
        assert_eq!(action, "fold");
    }

    #[test]
    fn test_bot_calls_medium_hand() {
        let mut state = PokerState::new_game();
        state.start_hand();
        // High card hand (strength 0.6)
        state.players[1].hand = vec![make_card(14, 0), make_card(10, 1)];
        state.current_bet = 100;
        state.players[1].bet = 0;
        let (action, _) = bot_action(&state, 1);
        assert_eq!(action, "call");
    }

    // -----------------------------------------------------------------------
    // Session process integration
    // -----------------------------------------------------------------------

    #[test]
    fn test_session_start_returns_valid_json() {
        let mut session = PokerSession {
            state: PokerState::new_game(),
        };
        let out = session.process(r#"{"action":"start"}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["round"].as_str().unwrap(), "pre-flop");
        assert_eq!(v["players"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_session_bots_hands_are_hidden() {
        let mut session = PokerSession {
            state: PokerState::new_game(),
        };
        let out = session.process(r#"{"action":"start"}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        // Bot players should have empty hand arrays
        for p in v["players"].as_array().unwrap() {
            if !p["is_human"].as_bool().unwrap() {
                assert_eq!(
                    p["hand"].as_array().unwrap().len(),
                    0,
                    "bot hand should be hidden"
                );
            }
        }
    }

    #[test]
    fn test_session_human_hand_is_visible() {
        let mut session = PokerSession {
            state: PokerState::new_game(),
        };
        let out = session.process(r#"{"action":"start"}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let human = &v["players"][0];
        assert_eq!(human["hand"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_session_fold_action() {
        let mut session = PokerSession {
            state: PokerState::new_game(),
        };
        session.process(r#"{"action":"start"}"#);
        // Ensure it's human's turn (player 0, current_player=0 in default setup)
        let out = session.process(r#"{"action":"fold"}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["players"][0]["folded"].as_bool().unwrap());
    }

    #[test]
    fn test_chip_conservation_throughout_hand() {
        let mut session = PokerSession {
            state: PokerState::new_game(),
        };
        session.process(r#"{"action":"start"}"#);

        // Human calls
        session.process(r#"{"action":"call"}"#);
        // Check chip conservation at any point
        let out = session.process(r#"{}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let chips: u32 = v["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["chips"].as_u64().unwrap() as u32)
            .sum();
        let pot = v["pot"].as_u64().unwrap() as u32;
        assert_eq!(chips + pot, 3000, "chips + pot must always equal 3000");
    }
}
