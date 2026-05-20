//! Snake universe — pure-Rust WASM game, no game_core dependency.
//!
//! Grid: 40×20. Border tiles (x=0, x=39, y=0, y=19) are walls.
//! Snake starts at (20,10) going right: [(20,10),(19,10),(18,10)].
//! Food starts at (10,5).
//!
//! Input JSON:  `{"action":"up"|"down"|"left"|"right"|"quit"}`
//! Output JSON: `{"snake":[[x,y],…],"food":[x,y],"width":40,"height":20,"score":<n>,"is_over":<bool>}`
//! Error JSON:  `{"error":"<message>"}`

use serde::{Deserialize, Serialize};
use universe_sdk::universe_exports;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const WIDTH: i32 = 40;
const HEIGHT: i32 = 20;

// ---------------------------------------------------------------------------
// Input / Output types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Input {
    action: Option<String>,
}

#[derive(Serialize)]
struct GameState<'a> {
    snake: &'a [[i32; 2]],
    food: [i32; 2],
    width: i32,
    height: i32,
    score: u32,
    is_over: bool,
}

#[derive(Serialize)]
struct ErrorState<'a> {
    error: &'a str,
}

// ---------------------------------------------------------------------------
// SnakeSession
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Default)]
pub struct SnakeSession {
    /// Body positions, head-first. Empty = uninitialized.
    snake: Vec<[i32; 2]>,
    food: [i32; 2],
    /// Current movement direction as [dx, dy].
    dir: [i32; 2],
    /// Buffered next direction (applied at start of tick).
    next_dir: [i32; 2],
    score: u32,
    is_over: bool,
}

impl SnakeSession {
    fn initialized(&self) -> bool {
        !self.snake.is_empty()
    }

    fn init(&mut self) {
        self.snake = vec![[20, 10], [19, 10], [18, 10]];
        self.food = [10, 5];
        self.dir = [1, 0];
        self.next_dir = [1, 0];
        self.score = 0;
        self.is_over = false;
    }

    /// Place food at the first free cell in row-major order, skipping the snake body.
    fn place_food(&mut self) {
        for y in 1..(HEIGHT - 1) {
            for x in 1..(WIDTH - 1) {
                let candidate = [x, y];
                if !self.snake.contains(&candidate) {
                    self.food = candidate;
                    return;
                }
            }
        }
        // Grid is full — keep food where it is.
    }

    /// Apply an action string and advance the game by one tick.
    /// Returns the JSON-encoded game state.
    pub fn process(&mut self, input: &str) -> String {
        // Lazy init on first call.
        if !self.initialized() {
            self.init();
        }

        // Parse action (lenient: missing or unknown → gravity tick).
        if !input.is_empty() {
            match serde_json::from_str::<Input>(input) {
                Ok(inp) => {
                    if let Some(action) = inp.action.as_deref() {
                        match action {
                            "quit" => {
                                self.is_over = true;
                            }
                            "up" => {
                                // Prevent 180° reversal: cannot go up if currently going down.
                                if self.dir != [0, 1] {
                                    self.next_dir = [0, -1];
                                }
                            }
                            "down" => {
                                if self.dir != [0, -1] {
                                    self.next_dir = [0, 1];
                                }
                            }
                            "left" => {
                                if self.dir != [1, 0] {
                                    self.next_dir = [-1, 0];
                                }
                            }
                            "right" => {
                                if self.dir != [-1, 0] {
                                    self.next_dir = [1, 0];
                                }
                            }
                            _ => {
                                // Unknown action: treat as gravity tick (no direction change).
                            }
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("invalid input JSON: {e}");
                    return serde_json::to_string(&ErrorState { error: &msg })
                        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string());
                }
            }
        }

        // If the game is over, return current state without advancing.
        if self.is_over {
            return self.render();
        }

        // Commit buffered direction.
        self.dir = self.next_dir;

        // Compute new head position.
        let head = self.snake[0];
        let new_x = head[0] + self.dir[0];
        let new_y = head[1] + self.dir[1];

        // Wall collision: border rows/columns (0 and WIDTH-1 / HEIGHT-1) are solid walls.
        if new_x <= 0 || new_x >= WIDTH - 1 || new_y <= 0 || new_y >= HEIGHT - 1 {
            self.is_over = true;
            return self.render();
        }

        let new_head = [new_x, new_y];

        // Self-collision.
        if self.snake.contains(&new_head) {
            self.is_over = true;
            return self.render();
        }

        // Move: prepend new head.
        self.snake.insert(0, new_head);

        if new_head == self.food {
            // Eat food: grow and place new food.
            self.score += 10;
            self.place_food();
        } else {
            // Normal move: remove tail.
            self.snake.pop();
        }

        self.render()
    }

    fn render(&self) -> String {
        let state = GameState {
            snake: &self.snake,
            food: self.food,
            width: WIDTH,
            height: HEIGHT,
            score: self.score,
            is_over: self.is_over,
        };
        serde_json::to_string(&state)
            .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
    }
}

// ---------------------------------------------------------------------------
// WASM exports
// ---------------------------------------------------------------------------

universe_exports!(SnakeSession);

// ---------------------------------------------------------------------------
// Unit tests (native only)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn new_session() -> SnakeSession {
        let mut s = SnakeSession::default();
        s.init();
        s
    }

    fn parse_state(json: &str) -> serde_json::Value {
        serde_json::from_str(json).expect("valid JSON output")
    }

    // --- Initialization ---

    #[test]
    fn initial_state_not_over_score_zero() {
        let s = new_session();
        assert!(!s.is_over);
        assert_eq!(s.score, 0);
        assert_eq!(s.snake.len(), 3);
        assert_eq!(s.snake[0], [20, 10]);
    }

    #[test]
    fn process_empty_input_returns_valid_state() {
        let mut s = SnakeSession::default();
        let out = parse_state(&s.process(""));
        assert_eq!(out["width"], 40);
        assert_eq!(out["height"], 20);
        assert_eq!(out["score"], 0);
        assert_eq!(out["is_over"], false);
    }

    // --- Movement ---

    #[test]
    fn move_right_advances_head() {
        let mut s = new_session();
        let out = parse_state(&s.process(r#"{"action":"right"}"#));
        // head was [20,10], moves to [21,10]
        let snake = &out["snake"];
        assert_eq!(snake[0][0], 21);
        assert_eq!(snake[0][1], 10);
        assert!(!out["is_over"].as_bool().unwrap());
    }

    #[test]
    fn move_up_changes_direction() {
        let mut s = new_session();
        // First move up (currently going right, so up is valid)
        let out = parse_state(&s.process(r#"{"action":"up"}"#));
        // head was [20,10], moves to [20,9]
        let snake = &out["snake"];
        assert_eq!(snake[0][0], 20);
        assert_eq!(snake[0][1], 9);
    }

    // --- 180° reversal prevention ---

    #[test]
    fn no_reverse_180_right_to_left() {
        let mut s = new_session();
        // Going right, trying left — should be ignored, snake continues right.
        let out = parse_state(&s.process(r#"{"action":"left"}"#));
        let snake = &out["snake"];
        // head moves to [21,10], not to [19,10]
        assert_eq!(snake[0][0], 21);
        assert!(!out["is_over"].as_bool().unwrap());
    }

    // --- Wall collision ---

    #[test]
    fn wall_collision_ends_game() {
        let mut s = new_session();
        // Snake starts at [20,10] going right; right wall is x=39 (wall at 39).
        // After 18 right moves the head reaches [38,10]; next move → x=39 = wall → game over.
        let mut out = serde_json::Value::Null;
        for _ in 0..19 {
            out = parse_state(&s.process(r#"{"action":"right"}"#));
        }
        assert!(out["is_over"].as_bool().unwrap());
    }

    // --- Quit ---

    #[test]
    fn quit_action_sets_game_over() {
        let mut s = new_session();
        let out = parse_state(&s.process(r#"{"action":"quit"}"#));
        assert!(out["is_over"].as_bool().unwrap());
    }

    #[test]
    fn process_after_quit_does_not_advance() {
        let mut s = new_session();
        s.process(r#"{"action":"quit"}"#);
        let before = parse_state(&s.process(""));
        let after = parse_state(&s.process(r#"{"action":"right"}"#));
        assert_eq!(before["snake"], after["snake"]);
    }

    // --- Food / scoring ---

    #[test]
    fn food_eaten_increases_score_and_grows_snake() {
        let mut s = new_session();
        // Place food directly in front of the snake (right of head at [20,10]).
        s.food = [21, 10];
        let initial_len = s.snake.len();
        let out = parse_state(&s.process(r#"{"action":"right"}"#));
        assert_eq!(out["score"], 10);
        // Snake should have grown by 1.
        assert_eq!(out["snake"].as_array().unwrap().len(), initial_len + 1);
    }

    #[test]
    fn food_not_eaten_keeps_score_zero() {
        let mut s = new_session();
        // Default food is at [10,5], not in front of the head.
        let out = parse_state(&s.process(r#"{"action":"right"}"#));
        assert_eq!(out["score"], 0);
    }

    // --- Self-collision ---

    #[test]
    fn self_collision_ends_game() {
        let mut s = new_session();
        // Force self-collision by jamming the snake back on itself.
        // Manually craft a state where moving right hits the body.
        // Snake going right at [5,10]: body occupies [6,10].
        s.snake = vec![[5, 10], [6, 10], [7, 10]];
        s.dir = [0, 1]; // going down
        s.next_dir = [0, 1];
        // Move right: head=[5,10]+[1,0]=[6,10] which is body[1] → collision.
        s.dir = [0, 1];
        s.next_dir = [1, 0]; // queue right
        let out = parse_state(&s.process(r#"{"action":"right"}"#));
        assert!(out["is_over"].as_bool().unwrap());
    }

    // --- Invalid JSON ---

    #[test]
    fn invalid_json_returns_error() {
        let mut s = new_session();
        let out = parse_state(&s.process("not json"));
        assert!(out.get("error").is_some());
    }

    // --- Output structure ---

    #[test]
    fn output_contains_all_required_fields() {
        let mut s = SnakeSession::default();
        let out = parse_state(&s.process(""));
        assert!(out.get("snake").is_some());
        assert!(out.get("food").is_some());
        assert!(out.get("width").is_some());
        assert!(out.get("height").is_some());
        assert!(out.get("score").is_some());
        assert!(out.get("is_over").is_some());
    }
}
