use game_core::{
    engine::{Direction, Entity, EntityType, Input, Map, Tile, Universe},
    games::{Game, GameAction, SnakeGame},
};

/// Trait for Yggdrasil-side game adapters.
pub trait YggGame {
    fn tick(&mut self, input: Input) -> GameAction;
    fn render_json(&self) -> String;
    fn score(&self) -> u32;
    fn is_over(&self) -> bool;
}

/// Yggdrasil adapter wrapping [`game_core::SnakeGame`].
///
/// Adds real snake movement logic (body, food, collision) on top of the
/// game-core session/universe bookkeeping.
/// Opções de inicialização que vêm de uma variante do registro de universos.
/// `None` = comportamento padrão (root).
#[derive(Default, Debug, Clone)]
pub struct SnakeOptions {
    /// Posições adicionais de paredes internas. Usado pela variante `snake/walls`.
    pub walls: Vec<(usize, usize)>,
}

pub struct YggSnake {
    pub inner: SnakeGame,
    snake_body: Vec<(usize, usize)>,
    food: (usize, usize),
    /// Current heading as (dx, dy).
    direction: (i32, i32),
    next_direction: (i32, i32),
    pub score: u32,
    pub game_over: bool,
    width: usize,
    height: usize,
    /// Paredes internas adicionadas por variantes (YG-37).
    walls: Vec<(usize, usize)>,
}

impl YggSnake {
    pub fn new(universe: Universe) -> Self {
        Self::with_options(universe, SnakeOptions::default())
    }

    pub fn with_options(universe: Universe, opts: SnakeOptions) -> Self {
        let width = universe.map.width;
        let height = universe.map.height;
        let inner = SnakeGame::new(universe);

        let mid_x = width / 2;
        let mid_y = height / 2;
        let snake_body = vec![
            (mid_x, mid_y),
            (mid_x.saturating_sub(1), mid_y),
            (mid_x.saturating_sub(2), mid_y),
        ];

        let food = (width / 4, height / 4);

        Self {
            inner,
            snake_body,
            food,
            direction: (1, 0),
            next_direction: (1, 0),
            score: 0,
            game_over: false,
            width,
            height,
            walls: opts.walls,
        }
    }

    /// Helper: gera um padrão determinístico de paredes internas para a
    /// variante `snake/walls`. 3 colunas verticais de paredes a 1/4, 2/4, 3/4
    /// da largura, deixando passagem nas extremidades.
    pub fn walls_pattern(width: usize, height: usize) -> Vec<(usize, usize)> {
        let mut walls = Vec::new();
        for col_frac in [4, 2, 4] {
            // 4 → width/4, 2 → width/2, 4 → 3*width/4 (calculado abaixo)
            let _ = col_frac;
        }
        for &x in &[width / 4, width / 2, 3 * width / 4] {
            if x == 0 || x >= width.saturating_sub(1) {
                continue;
            }
            // passa pela metade da coluna, deixando 2 tiles de fresta no topo e base
            let start_y = 3;
            let end_y = height.saturating_sub(3);
            for y in start_y..end_y {
                walls.push((x, y));
            }
        }
        walls
    }

    fn place_food(&mut self) {
        for y in 1..(self.height - 1) {
            for x in 1..(self.width - 1) {
                let pos = (x, y);
                if !self.snake_body.contains(&pos) && !self.walls.contains(&pos) {
                    self.food = pos;
                    return;
                }
            }
        }
    }

    fn build_map(&self) -> Map {
        let mut map = Map::new(self.width, self.height);

        // Internal walls from variant (YG-37).
        for &(x, y) in &self.walls {
            let _ = map.set_tile(x, y, Tile::Wall);
        }

        let _ = map.set_tile(
            self.food.0,
            self.food.1,
            Tile::Entity(Entity {
                id: "food".to_string(),
                entity_type: EntityType::Item,
                display: '*',
            }),
        );

        for (i, &(x, y)) in self.snake_body.iter().enumerate() {
            let tile = if i == 0 {
                Tile::Entity(Entity {
                    id: "snake_head".to_string(),
                    entity_type: EntityType::Player,
                    display: '@',
                })
            } else {
                Tile::Entity(Entity {
                    id: format!("snake_{i}"),
                    entity_type: EntityType::Obstacle,
                    display: 'o',
                })
            };
            let _ = map.set_tile(x, y, tile);
        }

        map
    }
}

impl YggGame for YggSnake {
    fn tick(&mut self, input: Input) -> GameAction {
        if self.game_over {
            return GameAction::Quit;
        }

        match input {
            Input::Quit => {
                self.game_over = true;
                return GameAction::Quit;
            }
            Input::Move(Direction::Up) if self.direction != (0, 1) => {
                self.next_direction = (0, -1);
            }
            Input::Move(Direction::Down) if self.direction != (0, -1) => {
                self.next_direction = (0, 1);
            }
            Input::Move(Direction::Left) if self.direction != (1, 0) => {
                self.next_direction = (-1, 0);
            }
            Input::Move(Direction::Right) if self.direction != (-1, 0) => {
                self.next_direction = (1, 0);
            }
            _ => {}
        }

        self.direction = self.next_direction;
        let head = self.snake_body[0];
        let new_x = head.0 as i32 + self.direction.0;
        let new_y = head.1 as i32 + self.direction.1;

        // Wall collision: border tiles (0 and width/height - 1) are walls
        if new_x <= 0
            || new_x >= self.width as i32 - 1
            || new_y <= 0
            || new_y >= self.height as i32 - 1
        {
            self.game_over = true;
            return GameAction::Quit;
        }

        let new_head = (new_x as usize, new_y as usize);

        // Snake body collision OR internal wall (YG-37 variant).
        if self.snake_body.contains(&new_head) || self.walls.contains(&new_head) {
            self.game_over = true;
            return GameAction::Quit;
        }

        self.snake_body.insert(0, new_head);

        if new_head == self.food {
            self.score += 10;
            self.place_food();
        } else {
            self.snake_body.pop();
        }

        // Delegate to inner game-core instance for session bookkeeping
        let _ = self.inner.tick(Input::None);

        GameAction::Continue
    }

    fn render_json(&self) -> String {
        let map = self.build_map();
        serde_json::to_string(&map).unwrap_or_default()
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

    fn make_game() -> YggSnake {
        YggSnake::new(Universe::snake())
    }

    #[test]
    fn new_game_not_over_and_score_zero() {
        let g = make_game();
        assert!(!g.is_over());
        assert_eq!(g.score(), 0);
    }

    #[test]
    fn quit_input_ends_game_with_quit_action() {
        let mut g = make_game();
        let action = g.tick(Input::Quit);
        assert_eq!(action, GameAction::Quit);
        assert!(g.is_over());
    }

    #[test]
    fn tick_after_game_over_returns_quit() {
        let mut g = make_game();
        g.tick(Input::Quit);
        let action = g.tick(Input::Move(Direction::Right));
        assert_eq!(action, GameAction::Quit);
    }

    #[test]
    fn continue_on_valid_move() {
        let mut g = make_game();
        let action = g.tick(Input::Move(Direction::Right));
        assert_eq!(action, GameAction::Continue);
        assert!(!g.is_over());
    }

    #[test]
    fn render_json_is_valid_map() {
        let g = make_game();
        let json = g.render_json();
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(v["width"], 40);
        assert_eq!(v["height"], 20);
        assert!(v["tiles"].is_array());
    }

    #[test]
    fn wall_collision_ends_game() {
        let mut g = make_game();
        // Snake starts at (20, 10) going right; right wall is x=39
        let mut action = GameAction::Continue;
        for _ in 0..25 {
            action = g.tick(Input::Move(Direction::Right));
            if action == GameAction::Quit {
                break;
            }
        }
        assert_eq!(action, GameAction::Quit);
        assert!(g.is_over());
    }

    #[test]
    fn no_reverse_180_turn() {
        let mut g = make_game();
        // Going right, trying to go left is ignored
        g.tick(Input::Move(Direction::Right));
        let action = g.tick(Input::Move(Direction::Left));
        // Should still be going right, not crash immediately
        assert_eq!(action, GameAction::Continue);
    }
}
