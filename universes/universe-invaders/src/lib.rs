//! universe-invaders — Space Invaders WASM universe for Yggdrasil.
//!
//! Canvas: 480×480. 4 rows × 10 cols of aliens.
//! xorshift64 RNG seeded from the host via `universe_sdk::get_random()`.
//!
//! Input JSON:  `{"action":"left"|"right"|"fire"}`  (anything else = bare tick)
//! Output JSON: game state or `{"error":"..."}` on invalid input.

use serde::{Deserialize, Serialize};
use universe_sdk::get_random;

// ---------------------------------------------------------------------------
// Canvas / game constants
// ---------------------------------------------------------------------------

const W: i32 = 480;
const H: i32 = 480;
const ROWS: usize = 4;
const COLS: usize = 10;
const ALIEN_W: i32 = 32;
const ALIEN_H: i32 = 24;
const ALIEN_GAP: i32 = 8;
const PLAYER_W: i32 = 40;
const PLAYER_H: i32 = 10;
const BULLET_H: i32 = 10;
const PLAYER_STEP: i32 = 10;
const ALIEN_SPEED: i32 = 8; // pixels per move event
const ALIEN_DROP: i32 = 12; // pixels to drop on wall bounce (ALIEN_H/2)

// ---------------------------------------------------------------------------
// Serializable data types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
pub struct Bullet {
    pub x: i32,
    pub y: i32,
    pub vy: i32,
    pub alien: bool,
}

#[derive(Serialize, Clone)]
pub struct Alien {
    pub x: i32,
    pub y: i32,
    pub alive: bool,
    pub row: usize,
}

#[derive(Serialize)]
struct InvadersState<'a> {
    player_x: i32,
    bullets: &'a [Bullet],
    aliens: &'a [Alien],
    score: u32,
    lives: u32,
    is_over: bool,
    won: bool,
    width: i32,
    height: i32,
}

#[derive(Serialize)]
struct ErrorState<'a> {
    error: &'a str,
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

pub struct InvadersSession {
    player_x: i32,
    bullets: Vec<Bullet>,
    aliens: Vec<Alien>,
    alien_dir: i32,
    alien_ticks: u32,
    alien_shoot_ticks: u32,
    score: u32,
    lives: u32,
    is_over: bool,
    won: bool,
    rng_state: u64,
    initialized: bool,
}

impl Default for InvadersSession {
    fn default() -> Self {
        Self {
            player_x: 0,
            bullets: vec![],
            aliens: vec![],
            alien_dir: 1,
            alien_ticks: 0,
            alien_shoot_ticks: 0,
            score: 0,
            lives: 3,
            is_over: false,
            won: false,
            rng_state: 0,
            initialized: false,
        }
    }
}

impl InvadersSession {
    fn init(&mut self) {
        let seed = get_random();
        self.rng_state = if seed == 0 { 0xdeadbeef_cafebabe } else { seed };
        self.player_x = W / 2 - PLAYER_W / 2;
        self.spawn_aliens();
        self.initialized = true;
    }

    fn player_y() -> i32 {
        H - 30
    }

    fn spawn_aliens(&mut self) {
        self.aliens.clear();
        for r in 0..ROWS {
            for c in 0..COLS {
                self.aliens.push(Alien {
                    x: 30 + c as i32 * (ALIEN_W + ALIEN_GAP),
                    y: 50 + r as i32 * (ALIEN_H + ALIEN_GAP),
                    alive: true,
                    row: r,
                });
            }
        }
    }

    // --- RNG ---

    fn rand(&mut self) -> u64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        self.rng_state
    }

    // --- Helpers ---

    fn alive_count(&self) -> usize {
        self.aliens.iter().filter(|a| a.alive).count()
    }

    fn killed_count(&self) -> u32 {
        (ROWS * COLS - self.alive_count()) as u32
    }

    fn alien_move_interval(&self) -> u32 {
        u32::max(5, 20u32.saturating_sub(self.killed_count() / 2))
    }

    fn alien_shoot_interval(&self) -> u32 {
        u32::max(8, 30u32.saturating_sub(self.killed_count()))
    }

    // --- Player actions ---

    fn move_left(&mut self) {
        self.player_x = (self.player_x - PLAYER_STEP).max(0);
    }

    fn move_right(&mut self) {
        self.player_x = (self.player_x + PLAYER_STEP).min(W - PLAYER_W);
    }

    fn fire(&mut self) {
        if self.bullets.iter().filter(|b| !b.alien).count() >= 3 {
            return;
        }
        self.bullets.push(Bullet {
            x: self.player_x + PLAYER_W / 2,
            y: Self::player_y() - BULLET_H,
            vy: -8,
            alien: false,
        });
    }

    // --- Tick ---

    /// Advance one game tick (after processing input).
    /// Returns `true` when the game has ended (won or lost).
    fn advance_tick(&mut self) -> bool {
        // Move all bullets.
        for b in &mut self.bullets {
            b.y += b.vy;
        }
        self.bullets
            .retain(|b| b.y > -BULLET_H && b.y < H + BULLET_H);

        // Move aliens on interval.
        self.alien_ticks += 1;
        if self.alien_ticks >= self.alien_move_interval() {
            self.alien_ticks = 0;
            let mut hit_wall = false;
            for a in self.aliens.iter_mut().filter(|a| a.alive) {
                a.x += self.alien_dir * ALIEN_SPEED;
                if a.x < 10 || a.x + ALIEN_W > W - 10 {
                    hit_wall = true;
                }
            }
            if hit_wall {
                self.alien_dir *= -1;
                let py = Self::player_y();
                for a in self.aliens.iter_mut().filter(|a| a.alive) {
                    a.y += ALIEN_DROP;
                }
                // If any alien has reached the player line → game over.
                if self.aliens.iter().any(|a| a.alive && a.y + ALIEN_H >= py) {
                    self.is_over = true;
                    return true;
                }
            }
        }

        // Alien shooting.
        self.alien_shoot_ticks += 1;
        if self.alien_shoot_ticks >= self.alien_shoot_interval() {
            self.alien_shoot_ticks = 0;
            let alive_indices: Vec<usize> = self
                .aliens
                .iter()
                .enumerate()
                .filter(|(_, a)| a.alive)
                .map(|(i, _)| i)
                .collect();
            if !alive_indices.is_empty() {
                let rng = self.rand();
                let ai = alive_indices[(rng as usize) % alive_indices.len()];
                let bx = self.aliens[ai].x + ALIEN_W / 2;
                let by = self.aliens[ai].y + ALIEN_H;
                self.bullets.push(Bullet {
                    x: bx,
                    y: by,
                    vy: 3,
                    alien: true,
                });
            }
        }

        // Collision: player bullets vs aliens.
        let mut killed_aliens: Vec<usize> = vec![];
        let mut killed_bullets: Vec<usize> = vec![];
        let mut score_gain = 0u32;

        for (bi, b) in self.bullets.iter().enumerate() {
            if b.alien {
                continue;
            }
            for (ai, a) in self.aliens.iter().enumerate() {
                if !a.alive || killed_aliens.contains(&ai) {
                    continue;
                }
                // Overlap check (point-in-rect: bullet tip vs alien AABB).
                if b.x >= a.x && b.x <= a.x + ALIEN_W && b.y >= a.y && b.y <= a.y + ALIEN_H {
                    killed_aliens.push(ai);
                    killed_bullets.push(bi);
                    score_gain += (ROWS - a.row) as u32 * 10;
                    break;
                }
            }
        }

        for &ai in &killed_aliens {
            self.aliens[ai].alive = false;
        }
        self.score += score_gain;

        // Remove hit bullets (in reverse index order to preserve indices).
        killed_bullets.sort_unstable();
        for &bi in killed_bullets.iter().rev() {
            self.bullets.remove(bi);
        }

        // Collision: alien bullets vs player.
        let px = self.player_x;
        let py = Self::player_y();
        let mut player_hit = false;
        self.bullets.retain(|b| {
            if b.alien && b.x >= px && b.x <= px + PLAYER_W && b.y >= py && b.y <= py + PLAYER_H {
                player_hit = true;
                false
            } else {
                true
            }
        });
        if player_hit {
            self.lives = self.lives.saturating_sub(1);
            if self.lives == 0 {
                self.is_over = true;
                return true;
            }
        }

        // Win: all aliens dead.
        if self.aliens.iter().all(|a| !a.alive) {
            self.won = true;
            self.is_over = true;
            return true;
        }

        false
    }

    fn render_json(&self) -> String {
        let state = InvadersState {
            player_x: self.player_x,
            bullets: &self.bullets,
            aliens: &self.aliens,
            score: self.score,
            lives: self.lives,
            is_over: self.is_over,
            won: self.won,
            width: W,
            height: H,
        };
        serde_json::to_string(&state)
            .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
    }

    /// Main entry point called by the `universe_exports!` macro.
    pub fn process(&mut self, input: &str) -> String {
        if !self.initialized {
            self.init();
        }

        // Parse optional action.
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
                    let err = ErrorState {
                        error: &format!("invalid input: {e}"),
                    };
                    return serde_json::to_string(&err)
                        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string());
                }
            }
        };

        if !self.is_over {
            match action.as_str() {
                "left" => self.move_left(),
                "right" => self.move_right(),
                "fire" => self.fire(),
                _ => {} // bare tick — no extra input action
            }
            self.advance_tick();
        }

        self.render_json()
    }
}

universe_sdk::universe_exports!(InvadersSession);

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn new_session() -> InvadersSession {
        let mut s = InvadersSession::default();
        s.rng_state = 42; // deterministic
        s.player_x = W / 2 - PLAYER_W / 2;
        s.spawn_aliens();
        s.initialized = true;
        s
    }

    // --- Initial state ---

    #[test]
    fn initial_state_not_over() {
        let s = new_session();
        assert!(!s.is_over);
        assert!(!s.won);
        assert_eq!(s.score, 0);
        assert_eq!(s.lives, 3);
    }

    #[test]
    fn initial_aliens_count() {
        let s = new_session();
        assert_eq!(s.aliens.len(), ROWS * COLS);
        assert_eq!(s.alive_count(), ROWS * COLS);
    }

    #[test]
    fn initial_alien_positions() {
        let s = new_session();
        // First alien: row 0, col 0
        assert_eq!(s.aliens[0].x, 30);
        assert_eq!(s.aliens[0].y, 50);
        assert_eq!(s.aliens[0].row, 0);
        // Second alien: row 0, col 1
        assert_eq!(s.aliens[1].x, 30 + (ALIEN_W + ALIEN_GAP));
    }

    // --- Player movement ---

    #[test]
    fn player_moves_left() {
        let mut s = new_session();
        let before = s.player_x;
        s.move_left();
        assert_eq!(s.player_x, before - PLAYER_STEP);
    }

    #[test]
    fn player_moves_right() {
        let mut s = new_session();
        let before = s.player_x;
        s.move_right();
        assert_eq!(s.player_x, before + PLAYER_STEP);
    }

    #[test]
    fn player_clamped_at_left_wall() {
        let mut s = new_session();
        s.player_x = 0;
        s.move_left();
        assert_eq!(s.player_x, 0);
    }

    #[test]
    fn player_clamped_at_right_wall() {
        let mut s = new_session();
        s.player_x = W - PLAYER_W;
        s.move_right();
        assert_eq!(s.player_x, W - PLAYER_W);
    }

    // --- Shooting ---

    #[test]
    fn fire_adds_player_bullet() {
        let mut s = new_session();
        let before = s.bullets.iter().filter(|b| !b.alien).count();
        s.fire();
        assert_eq!(s.bullets.iter().filter(|b| !b.alien).count(), before + 1);
    }

    #[test]
    fn fire_bullet_has_correct_vy() {
        let mut s = new_session();
        s.fire();
        let b = s.bullets.iter().find(|b| !b.alien).unwrap();
        assert_eq!(b.vy, -8);
        assert!(!b.alien);
    }

    #[test]
    fn fire_bullet_centered_on_player() {
        let mut s = new_session();
        s.fire();
        let b = s.bullets.iter().find(|b| !b.alien).unwrap();
        assert_eq!(b.x, s.player_x + PLAYER_W / 2);
    }

    #[test]
    fn max_three_player_bullets() {
        let mut s = new_session();
        for _ in 0..5 {
            s.fire();
        }
        assert!(s.bullets.iter().filter(|b| !b.alien).count() <= 3);
    }

    // --- Alien movement ---

    #[test]
    fn alien_move_interval_decreases_as_aliens_die() {
        let mut s = new_session();
        let initial = s.alien_move_interval();
        // Kill half the aliens.
        for a in s.aliens.iter_mut().take(ROWS * COLS / 2) {
            a.alive = false;
        }
        let after = s.alien_move_interval();
        assert!(
            after <= initial,
            "interval should decrease or stay when aliens die"
        );
    }

    #[test]
    fn alien_shoot_interval_decreases_as_aliens_die() {
        let mut s = new_session();
        let initial = s.alien_shoot_interval();
        for a in s.aliens.iter_mut().take(20) {
            a.alive = false;
        }
        let after = s.alien_shoot_interval();
        assert!(after <= initial);
    }

    #[test]
    fn aliens_move_horizontally_on_tick() {
        let mut s = new_session();
        // Force enough ticks to trigger at least one alien move.
        let interval = s.alien_move_interval();
        let initial_x = s.aliens[0].x;
        for _ in 0..interval {
            s.alien_ticks += 1;
        }
        // Manually trigger the move logic via advance_tick.
        s.advance_tick();
        // With alien_ticks == interval at start of advance, the move fires once
        // and resets to 0. Aliens should have moved.
        let moved_x = s.aliens[0].x;
        // They started at initial_x + (interval ticks of advance_tick increments).
        // After the tick that triggers, they should move ALIEN_SPEED.
        assert_ne!(moved_x, initial_x, "aliens should have moved");
    }

    // --- Collision detection ---

    #[test]
    fn player_bullet_kills_alien() {
        let mut s = new_session();
        // Bullets move by vy=-8 before collision check.
        // Place the bullet 8 pixels below the alien so after moving it lands
        // inside the alien AABB.
        let ax = s.aliens[0].x;
        let ay = s.aliens[0].y;
        s.bullets.push(Bullet {
            x: ax + 1,
            y: ay + 9, // after vy=-8 → y becomes ay+1, inside AABB [ay, ay+ALIEN_H]
            vy: -8,
            alien: false,
        });
        s.advance_tick();
        assert!(!s.aliens[0].alive, "alien should be killed");
        assert!(s.score > 0, "score should increase");
    }

    #[test]
    fn alien_bullet_hits_player_reduces_lives() {
        let mut s = new_session();
        let lives_before = s.lives;
        // Alien bullets move by vy=+3 before collision check.
        // Place bullet 3 pixels above the player rect so after movement
        // it lands inside [py, py+PLAYER_H].
        let px = s.player_x;
        let py = InvadersSession::player_y();
        s.bullets.push(Bullet {
            x: px + 1,
            y: py - 2, // after vy=+3 → py+1, inside [py, py+PLAYER_H=10]
            vy: 3,
            alien: true,
        });
        s.advance_tick();
        assert_eq!(s.lives, lives_before - 1);
    }

    #[test]
    fn losing_all_lives_ends_game() {
        let mut s = new_session();
        s.lives = 1;
        let px = s.player_x;
        let py = InvadersSession::player_y();
        s.bullets.push(Bullet {
            x: px + 1,
            y: py - 2, // after vy=+3 → py+1, inside player rect
            vy: 3,
            alien: true,
        });
        s.advance_tick();
        assert!(s.is_over);
        assert!(!s.won);
    }

    #[test]
    fn killing_all_aliens_wins() {
        let mut s = new_session();
        for a in &mut s.aliens {
            a.alive = false;
        }
        let ended = s.advance_tick();
        assert!(ended);
        assert!(s.won);
        assert!(s.is_over);
    }

    #[test]
    fn score_by_row_top_row_worth_more() {
        let mut s = new_session();
        // Shoot a row-0 alien (top row = ROWS - row = 4 × 10 = 40 pts).
        // Bullets move by vy=-8 before collision, so offset y by +9 so
        // after movement the bullet is at ay+1 (inside AABB).
        let ax = s.aliens[0].x; // row 0, col 0
        let ay = s.aliens[0].y;
        let row = s.aliens[0].row;
        assert_eq!(row, 0);
        s.bullets.push(Bullet {
            x: ax + 1,
            y: ay + 9,
            vy: -8,
            alien: false,
        });
        s.advance_tick();
        // (ROWS - 0) * 10 = 40
        assert_eq!(s.score, 40);
    }

    #[test]
    fn score_by_row_bottom_row_worth_less() {
        let mut s = new_session();
        // Row 3 (bottom) alien.
        let bottom_ai = (ROWS - 1) * COLS;
        let ax = s.aliens[bottom_ai].x;
        let ay = s.aliens[bottom_ai].y;
        let row = s.aliens[bottom_ai].row;
        assert_eq!(row, ROWS - 1);
        s.bullets.push(Bullet {
            x: ax + 1,
            y: ay + 9, // after vy=-8 → ay+1, inside AABB
            vy: -8,
            alien: false,
        });
        s.advance_tick();
        // (ROWS - 3) * 10 = 10
        assert_eq!(s.score, 10);
    }

    // --- Process / JSON ---

    #[test]
    fn process_empty_returns_valid_json() {
        let mut s = new_session();
        let out = s.process("");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert!(v["player_x"].is_number());
        assert!(v["bullets"].is_array());
        assert!(v["aliens"].is_array());
        assert_eq!(v["aliens"].as_array().unwrap().len(), ROWS * COLS);
        assert_eq!(v["score"], 0);
        assert_eq!(v["lives"], 3);
        assert!(!v["is_over"].as_bool().unwrap());
        assert!(!v["won"].as_bool().unwrap());
        assert_eq!(v["width"], W);
        assert_eq!(v["height"], H);
    }

    #[test]
    fn process_fire_action() {
        let mut s = new_session();
        let out = s.process(r#"{"action":"fire"}"#);
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert!(v["bullets"].is_array());
    }

    #[test]
    fn process_invalid_json_returns_error_session_alive() {
        let mut s = new_session();
        let out = s.process("{invalid}");
        let v: serde_json::Value = serde_json::from_str(&out).expect("error is valid json");
        assert!(v["error"].is_string());
        assert!(!s.is_over);
    }

    #[test]
    fn process_initializes_on_first_call() {
        let mut s = InvadersSession::default();
        assert!(!s.initialized);
        let out = s.process("");
        assert!(s.initialized);
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(v["aliens"].as_array().unwrap().len(), ROWS * COLS);
    }

    #[test]
    fn actions_ignored_when_game_is_over() {
        let mut s = new_session();
        s.is_over = true;
        let score_before = s.score;
        s.process(r#"{"action":"fire"}"#);
        s.process(r#"{"action":"left"}"#);
        assert_eq!(s.score, score_before);
    }
}
