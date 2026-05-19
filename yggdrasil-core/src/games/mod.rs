pub mod invaders;
pub mod poker;
pub mod snake;
pub mod tetris;

// Backward-compat aliases so external crates can still use the flat paths.
pub use poker::bot as poker_bot;
pub use poker::game as poker_game;
pub use poker::lobby as poker_lobby;

pub use invaders::YggInvaders;
pub use poker::YggPoker;
pub use snake::{YggGame, YggSnake};
pub use tetris::YggTetris;
