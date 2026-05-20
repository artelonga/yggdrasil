use game_core::{
    engine::{Direction, Input, Universe},
    games::{Game, GameAction, InvadersGame},
};
use serde::Serialize;

use super::YggGame;

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

#[derive(Serialize, Clone)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Bullet {
    pub x: i32,
    pub y: i32,
    pub vy: i32,
    pub alien: bool,
}

#[derive(Serialize, Clone)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Alien {
    pub x: i32,
    pub y: i32,
    pub alive: bool,
    pub row: usize,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InvadersRender {
    pub player_x: i32,
    pub bullets: Vec<Bullet>,
    pub aliens: Vec<Alien>,
    pub score: u32,
    pub lives: u32,
    pub game_over: bool,
    pub won: bool,
}

/// Opções de inicialização que vêm de uma variante do registro de universos.
/// `None` = comportamento padrão (root). YG-37.
#[derive(Default, Debug, Clone)]
pub struct InvadersOptions {
    /// Override do número inicial de vidas. Default: 3.
    pub lives: Option<u32>,
}

/// Yggdrasil adapter wrapping [`game_core::InvadersGame`].
///
/// Implements a 4×10 alien grid with movement, alien shooting, player bullet
/// collision, scoring by row, and 3 lives.
pub struct YggInvaders {
    pub inner: InvadersGame,
    player_x: i32,
    bullets: Vec<Bullet>,
    aliens: Vec<Alien>,
    alien_dir: i32,
    alien_ticks: u32,
    alien_shoot_ticks: u32,
    pub score: u32,
    lives: u32,
    pub game_over: bool,
    pub won: bool,
    rng_state: u64,
}

impl YggInvaders {
    pub fn new(universe: Universe) -> Self {
        Self::with_options(universe, InvadersOptions::default())
    }

    pub fn with_options(universe: Universe, opts: InvadersOptions) -> Self {
        let mut game = Self {
            inner: InvadersGame::new(universe),
            player_x: W / 2 - PLAYER_W / 2,
            bullets: vec![],
            aliens: vec![],
            alien_dir: 1,
            alien_ticks: 0,
            alien_shoot_ticks: 0,
            score: 0,
            lives: opts.lives.unwrap_or(3),
            game_over: false,
            won: false,
            rng_state: 42,
        };
        game.spawn_aliens();
        game
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

    fn alive_count(&self) -> usize {
        self.aliens.iter().filter(|a| a.alive).count()
    }

    fn alien_move_interval(&self) -> u32 {
        let fallen = (ROWS * COLS - self.alive_count()) as u32;
        u32::max(5, 20 - fallen / 2)
    }

    fn rand(&mut self) -> u64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        self.rng_state
    }

    fn shoot_player(&mut self) {
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

    fn advance_tick(&mut self) -> GameAction {
        // Move bullets and remove those off-screen
        for b in &mut self.bullets {
            b.y += b.vy;
        }
        self.bullets
            .retain(|b| b.y > -BULLET_H && b.y < H + BULLET_H);

        // Move aliens on interval
        self.alien_ticks += 1;
        if self.alien_ticks >= self.alien_move_interval() {
            self.alien_ticks = 0;
            let mut hit_wall = false;
            for a in self.aliens.iter_mut().filter(|a| a.alive) {
                a.x += self.alien_dir * 8;
                if a.x <= 0 || a.x + ALIEN_W >= W {
                    hit_wall = true;
                }
            }
            if hit_wall {
                self.alien_dir *= -1;
                let py = Self::player_y();
                for a in self.aliens.iter_mut().filter(|a| a.alive) {
                    a.y += 16;
                }
                if self.aliens.iter().any(|a| a.alive && a.y + ALIEN_H >= py) {
                    self.game_over = true;
                    return GameAction::Quit;
                }
            }
        }

        // Alien shooting every 72 ticks
        self.alien_shoot_ticks += 1;
        if self.alien_shoot_ticks >= 72 {
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
                    vy: 4,
                    alien: true,
                });
            }
        }

        // Collision: player bullets vs aliens
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

        killed_bullets.sort_unstable();
        for &bi in killed_bullets.iter().rev() {
            self.bullets.remove(bi);
        }

        // Collision: alien bullets vs player
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
            if self.lives > 0 {
                self.lives -= 1;
            }
            if self.lives == 0 {
                self.game_over = true;
                return GameAction::Quit;
            }
        }

        // Win: all aliens destroyed
        if self.aliens.iter().all(|a| !a.alive) {
            self.won = true;
            return GameAction::Quit;
        }

        let _ = self.inner.tick(Input::None);
        GameAction::Continue
    }
}

impl YggGame for YggInvaders {
    type State = InvadersRender;

    fn render(&self) -> InvadersRender {
        InvadersRender {
            player_x: self.player_x,
            bullets: self.bullets.clone(),
            aliens: self.aliens.clone(),
            score: self.score,
            lives: self.lives,
            game_over: self.game_over,
            won: self.won,
        }
    }

    fn tick(&mut self, input: Input) -> GameAction {
        if self.game_over || self.won {
            return GameAction::Quit;
        }

        match input {
            Input::Quit => {
                self.game_over = true;
                return GameAction::Quit;
            }
            Input::Move(Direction::Left) => {
                self.player_x = (self.player_x - 4).max(0);
            }
            Input::Move(Direction::Right) => {
                self.player_x = (self.player_x + 4).min(W - PLAYER_W);
            }
            Input::Action => {
                self.shoot_player();
            }
            _ => {}
        }

        self.advance_tick()
    }

    fn score(&self) -> u32 {
        self.score
    }

    fn is_over(&self) -> bool {
        self.game_over || self.won
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_game() -> YggInvaders {
        YggInvaders::new(Universe::invaders())
    }

    #[test]
    fn new_game_not_over_and_score_zero() {
        let g = make_game();
        assert!(!g.is_over());
        assert_eq!(g.score(), 0);
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
        assert_eq!(g.tick(Input::None), GameAction::Quit);
    }

    #[test]
    fn tick_returns_continue() {
        let mut g = make_game();
        assert_eq!(g.tick(Input::None), GameAction::Continue);
    }

    #[test]
    fn render_has_expected_fields() {
        let g = make_game();
        let v = serde_json::to_value(g.render()).expect("serializable");
        assert!(v["player_x"].is_number());
        assert!(v["aliens"].is_array());
        assert_eq!(v["aliens"].as_array().unwrap().len(), ROWS * COLS);
        assert_eq!(v["score"], 0);
        assert_eq!(v["lives"], 3);
        assert!(!v["game_over"].as_bool().unwrap());
        assert!(!v["won"].as_bool().unwrap());
    }

    #[test]
    fn player_moves_left() {
        let mut g = make_game();
        let initial_x = g.player_x;
        g.tick(Input::Move(Direction::Left));
        assert!(g.player_x < initial_x);
    }

    #[test]
    fn player_moves_right() {
        let mut g = make_game();
        let initial_x = g.player_x;
        g.tick(Input::Move(Direction::Right));
        assert!(g.player_x > initial_x);
    }

    #[test]
    fn player_cannot_go_below_zero() {
        let mut g = make_game();
        g.player_x = 0;
        g.tick(Input::Move(Direction::Left));
        assert_eq!(g.player_x, 0);
    }

    #[test]
    fn player_cannot_go_past_right_bound() {
        let mut g = make_game();
        g.player_x = W - PLAYER_W;
        g.tick(Input::Move(Direction::Right));
        assert_eq!(g.player_x, W - PLAYER_W);
    }

    #[test]
    fn shoot_adds_player_bullet() {
        let mut g = make_game();
        let before = g.bullets.iter().filter(|b| !b.alien).count();
        g.tick(Input::Action);
        let after = g.bullets.iter().filter(|b| !b.alien).count();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn max_three_player_bullets() {
        let mut g = make_game();
        for _ in 0..5 {
            g.tick(Input::Action);
        }
        assert!(g.bullets.iter().filter(|b| !b.alien).count() <= 3);
    }

    #[test]
    fn all_aliens_alive_on_start() {
        let g = make_game();
        assert_eq!(g.alive_count(), ROWS * COLS);
    }

    #[test]
    fn killing_all_aliens_wins() {
        let mut g = make_game();
        for a in &mut g.aliens {
            a.alive = false;
        }
        let action = g.tick(Input::None);
        assert_eq!(action, GameAction::Quit);
        assert!(g.won);
    }
}
