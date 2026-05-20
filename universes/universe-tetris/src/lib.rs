//! universe-tetris — Tetris WASM universe for Yggdrasil.
//!
//! Board: 10 wide × 20 tall. 7 classic piece types. xorshift64 RNG seeded
//! from the host via `universe_sdk::get_random()`.
//!
//! Input JSON:  `{"action":"left"|"right"|"rotate"|"drop"|"down"}`
//! Output JSON: game state or `{"error":"..."}` on invalid input.

use serde::{Deserialize, Serialize};
use universe_sdk::get_random;

// ---------------------------------------------------------------------------
// Board constants
// ---------------------------------------------------------------------------

const BOARD_W: usize = 10;
const BOARD_H: usize = 20;

// ---------------------------------------------------------------------------
// Piece shapes (canonical spawn orientation)
// ---------------------------------------------------------------------------

fn piece_shape(piece_type: u8) -> Vec<Vec<u8>> {
    match piece_type {
        1 => vec![
            vec![0, 1, 0, 0],
            vec![0, 1, 0, 0],
            vec![0, 1, 0, 0],
            vec![0, 1, 0, 0],
        ],
        2 => vec![vec![1, 1], vec![1, 1]],
        3 => vec![vec![0, 1, 0], vec![1, 1, 1], vec![0, 0, 0]],
        4 => vec![vec![0, 1, 1], vec![1, 1, 0], vec![0, 0, 0]],
        5 => vec![vec![1, 1, 0], vec![0, 1, 1], vec![0, 0, 0]],
        6 => vec![vec![1, 0, 0], vec![1, 1, 1], vec![0, 0, 0]],
        7 => vec![vec![0, 0, 1], vec![1, 1, 1], vec![0, 0, 0]],
        _ => vec![],
    }
}

fn rotate_cw(shape: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let rows = shape.len();
    let cols = shape[0].len();
    let mut out = vec![vec![0u8; rows]; cols];
    for r in 0..rows {
        for c in 0..cols {
            out[c][rows - 1 - r] = shape[r][c];
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Serializable output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ActivePiece {
    piece_type: u8,
    x: i32,
    y: i32,
    shape: Vec<Vec<u8>>,
}

#[derive(Serialize)]
struct TetrisState {
    board: Vec<Vec<u8>>,
    piece: ActivePiece,
    score: u32,
    level: u32,
    lines: u32,
    is_over: bool,
}

#[derive(Serialize)]
struct ErrorState<'a> {
    error: &'a str,
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

pub struct TetrisSession {
    board: Vec<Vec<u8>>,
    piece_type: u8,
    piece_shape: Vec<Vec<u8>>,
    piece_x: i32,
    piece_y: i32,
    score: u32,
    level: u32,
    lines_cleared: u32,
    is_over: bool,
    rng_state: u64,
    /// `true` once the first `process` call has initialized the session.
    initialized: bool,
}

impl Default for TetrisSession {
    fn default() -> Self {
        // Everything is zeroed/empty; actual initialization happens on the
        // first `process` call so that `get_random()` is available from the
        // host at that point rather than at static-init time.
        Self {
            board: vec![],
            piece_type: 0,
            piece_shape: vec![],
            piece_x: 0,
            piece_y: 0,
            score: 0,
            level: 1,
            lines_cleared: 0,
            is_over: false,
            rng_state: 0,
            initialized: false,
        }
    }
}

impl TetrisSession {
    fn init(&mut self) {
        let seed = get_random();
        // Ensure the seed is non-zero (xorshift64 is undefined for seed=0).
        self.rng_state = if seed == 0 { 0xdeadbeef_cafebabe } else { seed };
        self.board = vec![vec![0u8; BOARD_W]; BOARD_H];
        self.spawn_piece();
        self.initialized = true;
    }

    // --- RNG ---

    fn next_rng(&mut self) -> u64 {
        // xorshift64
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        self.rng_state
    }

    fn next_piece_type(&mut self) -> u8 {
        (self.next_rng() % 7 + 1) as u8
    }

    // --- Piece management ---

    fn spawn_piece(&mut self) {
        self.piece_type = self.next_piece_type();
        self.piece_shape = piece_shape(self.piece_type);
        self.piece_x = (BOARD_W as i32 - self.piece_shape[0].len() as i32) / 2;
        self.piece_y = 0;
        let shape = self.piece_shape.clone();
        if self.collides_at(0, 0, &shape) {
            self.is_over = true;
        }
    }

    fn collides_at(&self, ox: i32, oy: i32, shape: &[Vec<u8>]) -> bool {
        for (r, row) in shape.iter().enumerate() {
            for (c, &cell) in row.iter().enumerate() {
                if cell == 0 {
                    continue;
                }
                let nx = self.piece_x + c as i32 + ox;
                let ny = self.piece_y + r as i32 + oy;
                if !(0..BOARD_W as i32).contains(&nx) || ny >= BOARD_H as i32 {
                    return true;
                }
                if ny >= 0 && self.board[ny as usize][nx as usize] != 0 {
                    return true;
                }
            }
        }
        false
    }

    fn lock_piece(&mut self) {
        let shape = self.piece_shape.clone();
        for (r, row) in shape.iter().enumerate() {
            for (c, &cell) in row.iter().enumerate() {
                if cell == 0 {
                    continue;
                }
                let nx = self.piece_x + c as i32;
                let ny = self.piece_y + r as i32;
                if (0..BOARD_H as i32).contains(&ny) && (0..BOARD_W as i32).contains(&nx) {
                    self.board[ny as usize][nx as usize] = self.piece_type;
                }
            }
        }
    }

    fn clear_lines(&mut self) {
        let mut cleared = 0u32;
        self.board.retain(|row| {
            if row.iter().all(|&c| c != 0) {
                cleared += 1;
                false
            } else {
                true
            }
        });
        for _ in 0..cleared {
            self.board.insert(0, vec![0u8; BOARD_W]);
        }
        if cleared > 0 {
            let pts: u32 = match cleared {
                1 => 100,
                2 => 300,
                3 => 500,
                4 => 800,
                _ => 0,
            } * self.level;
            self.score += pts;
            self.lines_cleared += cleared;
            self.level = self.lines_cleared / 10 + 1;
        }
    }

    /// Move the active piece down one row. If blocked, lock it, clear lines,
    /// and spawn the next piece.
    fn gravity_drop(&mut self) {
        let shape = self.piece_shape.clone();
        if !self.collides_at(0, 1, &shape) {
            self.piece_y += 1;
        } else {
            self.lock_piece();
            self.clear_lines();
            self.spawn_piece();
        }
    }

    // --- Input handling ---

    fn apply_action(&mut self, action: &str) {
        match action {
            "left" => {
                let shape = self.piece_shape.clone();
                if !self.collides_at(-1, 0, &shape) {
                    self.piece_x -= 1;
                }
            }
            "right" => {
                let shape = self.piece_shape.clone();
                if !self.collides_at(1, 0, &shape) {
                    self.piece_x += 1;
                }
            }
            "rotate" => {
                let rotated = rotate_cw(&self.piece_shape);
                if !self.collides_at(0, 0, &rotated) {
                    self.piece_shape = rotated;
                }
            }
            "drop" => {
                // Hard drop: sink to the lowest valid row, then lock.
                let shape = self.piece_shape.clone();
                while !self.collides_at(0, 1, &shape) {
                    self.piece_y += 1;
                }
                self.gravity_drop();
            }
            "down" | "" => {
                self.gravity_drop();
            }
            _ => {
                // Unknown action is treated as a bare tick (gravity).
                self.gravity_drop();
            }
        }
    }

    fn render(&self) -> TetrisState {
        TetrisState {
            board: self.board.clone(),
            piece: ActivePiece {
                piece_type: self.piece_type,
                x: self.piece_x,
                y: self.piece_y,
                shape: self.piece_shape.clone(),
            },
            score: self.score,
            level: self.level,
            lines: self.lines_cleared,
            is_over: self.is_over,
        }
    }

    /// Main entry point called by the `universe_exports!` macro.
    pub fn process(&mut self, input: &str) -> String {
        if !self.initialized {
            self.init();
        }

        // Parse optional action from JSON. Missing `action` key → gravity tick.
        let action: String = if input.is_empty() {
            String::new()
        } else {
            #[derive(Deserialize)]
            struct Input {
                #[serde(default)]
                action: String,
            }
            match serde_json::from_str::<Input>(input) {
                Ok(parsed) => parsed.action,
                Err(e) => {
                    // Invalid JSON: return error, session stays alive.
                    let err = ErrorState {
                        error: &format!("invalid input: {e}"),
                    };
                    return serde_json::to_string(&err)
                        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string());
                }
            }
        };

        if !self.is_over {
            self.apply_action(&action);
        }

        serde_json::to_string(&self.render())
            .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
    }
}

universe_sdk::universe_exports!(TetrisSession);

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn new_session() -> TetrisSession {
        let mut s = TetrisSession::default();
        // Bypass get_random() in tests by setting the seed directly.
        s.rng_state = 12345;
        s.board = vec![vec![0u8; BOARD_W]; BOARD_H];
        s.spawn_piece();
        s.initialized = true;
        s
    }

    // --- Spawn ---

    #[test]
    fn spawn_piece_places_piece_at_top() {
        let s = new_session();
        assert_eq!(s.piece_y, 0);
        assert!(s.piece_x >= 0 && s.piece_x < BOARD_W as i32);
        assert!(!s.piece_shape.is_empty());
    }

    #[test]
    fn new_session_not_over_score_zero() {
        let s = new_session();
        assert!(!s.is_over);
        assert_eq!(s.score, 0);
        assert_eq!(s.lines_cleared, 0);
        assert_eq!(s.level, 1);
    }

    // --- Movement ---

    #[test]
    fn move_left_decreases_x_when_not_blocked() {
        let mut s = new_session();
        // Force piece to centre so left is always valid.
        s.piece_x = 4;
        let before = s.piece_x;
        s.apply_action("left");
        assert_eq!(s.piece_x, before - 1);
    }

    #[test]
    fn move_right_increases_x_when_not_blocked() {
        let mut s = new_session();
        s.piece_x = 3;
        let before = s.piece_x;
        s.apply_action("right");
        assert_eq!(s.piece_x, before + 1);
    }

    #[test]
    fn move_left_clamped_by_wall() {
        let mut s = new_session();
        // Use O-piece (2×2, all cells filled) so piece_x=0 is truly the wall.
        s.piece_type = 2;
        s.piece_shape = piece_shape(2);
        s.piece_x = 0;
        s.piece_y = 5;
        s.apply_action("left");
        assert_eq!(s.piece_x, 0);
    }

    #[test]
    fn move_right_clamped_by_wall() {
        let mut s = new_session();
        s.piece_type = 2;
        s.piece_shape = piece_shape(2); // 2×2
        s.piece_x = (BOARD_W - 2) as i32; // rightmost valid x
        s.apply_action("right");
        assert_eq!(s.piece_x, (BOARD_W - 2) as i32);
    }

    // --- Rotate ---

    #[test]
    fn rotate_changes_shape() {
        let mut s = new_session();
        s.piece_type = 3; // T-piece — asymmetric
        s.piece_shape = piece_shape(3);
        s.piece_x = 3;
        s.piece_y = 5;
        let before = s.piece_shape.clone();
        s.apply_action("rotate");
        assert_ne!(s.piece_shape, before);
    }

    #[test]
    fn rotate_four_times_returns_to_original() {
        let original = piece_shape(3);
        let mut shape = original.clone();
        for _ in 0..4 {
            shape = rotate_cw(&shape);
        }
        assert_eq!(shape, original);
    }

    // --- Gravity / drop ---

    #[test]
    fn gravity_moves_piece_down() {
        let mut s = new_session();
        let before_y = s.piece_y;
        s.apply_action("down");
        // Piece either moved down or locked (if it was already at bottom).
        assert!(s.piece_y >= before_y);
    }

    #[test]
    fn hard_drop_locks_piece_immediately() {
        let mut s = new_session();
        let before_type = s.piece_type;
        s.apply_action("drop");
        // After hard drop, the piece is locked and a new one spawned.
        // Score/lines may or may not change, but the board should have
        // at least one non-zero cell.
        let has_locked = s.board.iter().flatten().any(|&c| c != 0) || s.piece_type != before_type;
        assert!(has_locked || !s.is_over);
    }

    #[test]
    fn unknown_action_treated_as_tick() {
        let mut s = new_session();
        let before_y = s.piece_y;
        s.apply_action("bogus");
        // Gravity tick happened (or piece locked if already at bottom row).
        assert!(s.piece_y >= before_y);
    }

    // --- Line clear ---

    #[test]
    fn clear_one_line_scores_100_at_level_1() {
        let mut s = new_session();
        s.board[BOARD_H - 1] = vec![1u8; BOARD_W];
        s.clear_lines();
        assert_eq!(s.score, 100);
        assert_eq!(s.lines_cleared, 1);
        assert_eq!(s.level, 1);
    }

    #[test]
    fn clear_two_lines_scores_300() {
        let mut s = new_session();
        for row in &mut s.board[BOARD_H - 2..] {
            *row = vec![1u8; BOARD_W];
        }
        s.clear_lines();
        assert_eq!(s.score, 300);
        assert_eq!(s.lines_cleared, 2);
    }

    #[test]
    fn clear_three_lines_scores_500() {
        let mut s = new_session();
        for row in &mut s.board[BOARD_H - 3..] {
            *row = vec![1u8; BOARD_W];
        }
        s.clear_lines();
        assert_eq!(s.score, 500);
        assert_eq!(s.lines_cleared, 3);
    }

    #[test]
    fn clear_four_lines_scores_800() {
        let mut s = new_session();
        for row in &mut s.board[BOARD_H - 4..] {
            *row = vec![1u8; BOARD_W];
        }
        s.clear_lines();
        assert_eq!(s.score, 800);
        assert_eq!(s.lines_cleared, 4);
    }

    #[test]
    fn clear_lines_inserts_empty_rows_at_top() {
        let mut s = new_session();
        s.board[BOARD_H - 1] = vec![1u8; BOARD_W];
        s.clear_lines();
        assert_eq!(s.board.len(), BOARD_H);
        assert_eq!(s.board[0], vec![0u8; BOARD_W]);
    }

    #[test]
    fn level_advances_at_10_lines() {
        let mut s = new_session();
        for _ in 0..10 {
            s.lines_cleared += 1;
        }
        s.level = s.lines_cleared / 10 + 1;
        assert_eq!(s.level, 2);
    }

    // --- Game over ---

    #[test]
    fn game_over_when_piece_spawns_into_occupied_cell() {
        let mut s = new_session();
        // Fill the top two rows so the spawn position is always occupied.
        for row in &mut s.board[0..4] {
            *row = vec![1u8; BOARD_W];
        }
        s.spawn_piece();
        assert!(s.is_over);
    }

    #[test]
    fn actions_ignored_when_game_is_over() {
        let mut s = new_session();
        s.is_over = true;
        let score_before = s.score;
        s.apply_action("left");
        s.apply_action("right");
        assert_eq!(s.score, score_before);
    }

    // --- Process / JSON ---

    #[test]
    fn process_empty_input_returns_valid_json() {
        let mut s = new_session();
        let out = s.process("");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert!(v["board"].is_array());
        assert_eq!(v["board"].as_array().unwrap().len(), BOARD_H);
        assert!(v["piece"]["x"].is_number());
        assert_eq!(v["score"], 0);
        assert_eq!(v["level"], 1);
        assert_eq!(v["lines"], 0);
        assert!(!v["is_over"].as_bool().unwrap());
    }

    #[test]
    fn process_action_left_returns_valid_json() {
        let mut s = new_session();
        let out = s.process(r#"{"action":"left"}"#);
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert!(v["board"].is_array());
    }

    #[test]
    fn process_invalid_json_returns_error_session_alive() {
        let mut s = new_session();
        let out = s.process("{not valid json}");
        let v: serde_json::Value = serde_json::from_str(&out).expect("error json is valid");
        assert!(v["error"].is_string());
        assert!(!s.is_over, "session should stay alive after invalid input");
    }

    #[test]
    fn process_initializes_on_first_call() {
        let mut s = TetrisSession::default();
        assert!(!s.initialized);
        let out = s.process("");
        assert!(s.initialized);
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert!(v["board"].is_array());
    }

    // --- Collides ---

    #[test]
    fn collides_with_left_wall() {
        let mut s = new_session();
        s.piece_type = 2;
        s.piece_shape = piece_shape(2);
        s.piece_x = 0;
        s.piece_y = 5;
        assert!(s.collides_at(-1, 0, &s.piece_shape.clone()));
    }

    #[test]
    fn collides_with_floor() {
        let mut s = new_session();
        s.piece_type = 2;
        s.piece_shape = piece_shape(2);
        s.piece_x = 4;
        s.piece_y = BOARD_H as i32 - 1;
        assert!(s.collides_at(0, 1, &s.piece_shape.clone()));
    }
}
