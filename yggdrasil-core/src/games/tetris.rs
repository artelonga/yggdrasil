use game_core::{
    engine::{Direction, Input, Universe},
    games::{Game, GameAction, TetrisGame},
};
use serde::Serialize;

use super::YggGame;

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

/// Yggdrasil adapter wrapping [`game_core::TetrisGame`].
///
/// Manages a 10×20 Tetris board with full game logic: piece spawning,
/// gravity, collision, line clearing, scoring, and level progression.
pub struct YggTetris {
    pub inner: TetrisGame,
    board: Vec<Vec<u8>>,
    piece_type: u8,
    piece_shape: Vec<Vec<u8>>,
    piece_x: i32,
    piece_y: i32,
    pub score: u32,
    level: u32,
    lines_cleared: u32,
    pub game_over: bool,
    rng_state: u64,
    /// YG-37: variante `tetris/sprint-40` encerra a mão ao atingir N linhas.
    /// `None` = modo marathon (sem limite).
    sprint_limit: Option<u32>,
}

/// Opções de inicialização que vêm de uma variante do registro de universos.
/// `None` = comportamento padrão (root). YG-37.
#[derive(Default, Debug, Clone)]
pub struct TetrisOptions {
    /// `Some(N)` = sprint mode, encerra ao limpar N linhas.
    pub sprint_lines: Option<u32>,
}

impl YggTetris {
    pub fn new(universe: Universe) -> Self {
        Self::with_options(universe, TetrisOptions::default())
    }

    pub fn with_options(universe: Universe, opts: TetrisOptions) -> Self {
        let mut game = Self {
            inner: TetrisGame::new(universe),
            board: vec![vec![0u8; 10]; 20],
            piece_type: 0,
            piece_shape: vec![],
            piece_x: 0,
            piece_y: 0,
            score: 0,
            level: 1,
            lines_cleared: 0,
            game_over: false,
            rng_state: 12345,
            sprint_limit: opts.sprint_lines,
        };
        game.spawn_piece();
        game
    }

    fn next_piece_type(&mut self) -> u8 {
        // xorshift64 — deterministic, no external deps needed
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        (self.rng_state % 7 + 1) as u8
    }

    fn spawn_piece(&mut self) {
        self.piece_type = self.next_piece_type();
        self.piece_shape = piece_shape(self.piece_type);
        self.piece_x = (10 - self.piece_shape[0].len() as i32) / 2;
        self.piece_y = 0;
        let shape = self.piece_shape.clone();
        if self.collides_at(0, 0, &shape) {
            self.game_over = true;
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
                if !(0..10).contains(&nx) || ny >= 20 {
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
                if (0..20).contains(&ny) && (0..10).contains(&nx) {
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
            self.board.insert(0, vec![0u8; 10]);
        }
        if cleared > 0 {
            let pts = match cleared {
                1 => 100,
                2 => 300,
                3 => 500,
                4 => 800,
                _ => 0,
            } * self.level;
            self.score += pts;
            self.lines_cleared += cleared;
            self.level = self.lines_cleared / 10 + 1;
            // YG-37: sprint mode encerra ao atingir o limite.
            if let Some(limit) = self.sprint_limit
                && self.lines_cleared >= limit
            {
                self.game_over = true;
            }
        }
    }

    fn gravity_drop(&mut self) -> GameAction {
        let shape = self.piece_shape.clone();
        if !self.collides_at(0, 1, &shape) {
            self.piece_y += 1;
        } else {
            self.lock_piece();
            self.clear_lines();
            self.spawn_piece();
            if self.game_over {
                return GameAction::Quit;
            }
        }
        let _ = self.inner.tick(Input::None);
        GameAction::Continue
    }
}

#[derive(Serialize)]
pub struct ActivePiece {
    pub piece_type: u8,
    pub x: i32,
    pub y: i32,
    pub shape: Vec<Vec<u8>>,
}

#[derive(Serialize)]
pub struct TetrisRender {
    pub board: Vec<Vec<u8>>,
    pub piece: ActivePiece,
    pub score: u32,
    pub level: u32,
    pub game_over: bool,
}

impl YggGame for YggTetris {
    type State = TetrisRender;

    fn render(&self) -> TetrisRender {
        TetrisRender {
            board: self.board.clone(),
            piece: ActivePiece {
                piece_type: self.piece_type,
                x: self.piece_x,
                y: self.piece_y,
                shape: self.piece_shape.clone(),
            },
            score: self.score,
            level: self.level,
            game_over: self.game_over,
        }
    }

    fn tick(&mut self, input: Input) -> GameAction {
        if self.game_over {
            return GameAction::Quit;
        }

        match input {
            Input::Quit => {
                self.game_over = true;
                GameAction::Quit
            }
            Input::Move(Direction::Left) => {
                let shape = self.piece_shape.clone();
                if !self.collides_at(-1, 0, &shape) {
                    self.piece_x -= 1;
                }
                GameAction::Continue
            }
            Input::Move(Direction::Right) => {
                let shape = self.piece_shape.clone();
                if !self.collides_at(1, 0, &shape) {
                    self.piece_x += 1;
                }
                GameAction::Continue
            }
            Input::Move(Direction::Down) | Input::None => self.gravity_drop(),
            Input::Move(Direction::Up) => {
                let rotated = rotate_cw(&self.piece_shape);
                if !self.collides_at(0, 0, &rotated) {
                    self.piece_shape = rotated;
                }
                GameAction::Continue
            }
            Input::Action => {
                // Hard drop: sink piece to lowest valid row, then lock
                let shape = self.piece_shape.clone();
                while !self.collides_at(0, 1, &shape) {
                    self.piece_y += 1;
                }
                self.gravity_drop()
            }
            _ => GameAction::Continue,
        }
    }

    fn score(&self) -> u32 {
        self.score
    }

    fn is_over(&self) -> bool {
        self.game_over
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_game() -> YggTetris {
        YggTetris::new(Universe::tetris())
    }

    #[test]
    fn new_game_not_over_and_score_zero() {
        let g = make_game();
        assert!(!g.is_over());
        assert_eq!(g.score(), 0);
    }

    #[test]
    fn sprint_limit_encerra_jogo_ao_atingir_n_linhas() {
        // YG-37: variant tetris/sprint-40 sets sprint_lines=Some(40); ao
        // limpar 40+ linhas, game_over deve virar true automaticamente.
        let mut g = YggTetris::with_options(
            Universe::tetris(),
            TetrisOptions {
                sprint_lines: Some(3),
            },
        );
        // Força clear_lines diretamente preenchendo o board com 3 linhas cheias.
        for y in 17..20 {
            g.board[y] = vec![1u8; 10];
        }
        g.clear_lines();
        assert_eq!(g.lines_cleared, 3);
        assert!(
            g.game_over,
            "sprint deveria encerrar ao atingir o limite de linhas"
        );
    }

    #[test]
    fn marathon_mode_nao_encerra_ao_limpar_linhas() {
        let mut g = make_game(); // sprint_limit = None
        for y in 17..20 {
            g.board[y] = vec![1u8; 10];
        }
        g.clear_lines();
        assert_eq!(g.lines_cleared, 3);
        assert!(!g.game_over, "marathon não deve encerrar por linhas limpas");
    }

    #[test]
    fn quit_input_ends_game() {
        let mut g = make_game();
        let action = g.tick(Input::Quit);
        assert_eq!(action, GameAction::Quit);
        assert!(g.is_over());
    }

    #[test]
    fn tick_after_game_over_returns_quit() {
        let mut g = make_game();
        g.tick(Input::Quit);
        let action = g.tick(Input::None);
        assert_eq!(action, GameAction::Quit);
    }

    #[test]
    fn drop_tick_returns_continue() {
        let mut g = make_game();
        let action = g.tick(Input::None);
        assert_eq!(action, GameAction::Continue);
        assert!(!g.is_over());
    }

    #[test]
    fn render_has_expected_fields() {
        let g = make_game();
        let v = serde_json::to_value(g.render()).expect("serializable");
        assert!(v["board"].is_array());
        assert_eq!(v["board"].as_array().unwrap().len(), 20);
        assert!(v["piece"]["x"].is_number());
        assert!(v["piece"]["y"].is_number());
        assert_eq!(v["score"], 0);
        assert_eq!(v["level"], 1);
        assert!(!v["game_over"].as_bool().unwrap());
    }

    #[test]
    fn line_clear_increases_score() {
        let mut g = make_game();
        // Fill bottom row completely; lock it via clear_lines
        g.board[19] = vec![1u8; 10];
        g.clear_lines();
        assert_eq!(g.score, 100); // 100 pts × level 1
        assert_eq!(g.lines_cleared, 1);
        assert_eq!(g.level, 1);
    }

    #[test]
    fn four_line_clear_scores_800() {
        let mut g = make_game();
        for row in &mut g.board[16..20] {
            *row = vec![1u8; 10];
        }
        g.clear_lines();
        assert_eq!(g.score, 800);
        assert_eq!(g.lines_cleared, 4);
    }

    #[test]
    fn left_right_moves_return_continue() {
        let mut g = make_game();
        assert_eq!(g.tick(Input::Move(Direction::Left)), GameAction::Continue);
        assert_eq!(g.tick(Input::Move(Direction::Right)), GameAction::Continue);
        assert!(!g.is_over());
    }
}
