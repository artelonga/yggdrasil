//! Lobby Yggdrasil — universo central com portais para os jogos.
//!
//! O lobby é um Universe 40x20 com 4 portais dispostos em grid 2x2:
//!
//! ```text
//!   (10, 8) snake          (30, 8) tetris
//!   (10,12) invaders       (30,12) poker
//! ```
//!
//! Cada portal usa `Tile::Portal(slug)` onde `slug` é o identificador do
//! jogo destino. A transição é feita via `Session::teleport_to(slug)`.

pub mod grid;
pub mod portals;

pub use grid::pos;
pub use grid::{LOBBY_HEIGHT, LOBBY_WIDTH};
pub use portals::slug;

use game_core::engine::{Objective, Tile, Universe};

/// Constrói o universo do lobby Yggdrasil.
///
/// Retorna um `Universe` 40x20 com bordas de parede (já adicionadas pelo
/// `Map::new` do engine) e 4 portais nas posições documentadas em [`pos`].
pub fn lobby() -> Universe {
    let mut u = Universe::new("yggdrasil".to_string(), LOBBY_WIDTH, LOBBY_HEIGHT);
    u.objective = Objective {
        description: "Escolha um universo para entrar".to_string(),
    };

    let _ = u
        .map
        .set_tile(pos::SNAKE.0, pos::SNAKE.1, Tile::Portal(slug::SNAKE.into()));
    let _ = u.map.set_tile(
        pos::TETRIS.0,
        pos::TETRIS.1,
        Tile::Portal(slug::TETRIS.into()),
    );
    let _ = u.map.set_tile(
        pos::INVADERS.0,
        pos::INVADERS.1,
        Tile::Portal(slug::INVADERS.into()),
    );
    let _ = u
        .map
        .set_tile(pos::POKER.0, pos::POKER.1, Tile::Portal(slug::POKER.into()));

    u
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::engine::Session;

    fn portal_slug(tile: Option<&Tile>) -> Option<&str> {
        match tile {
            Some(Tile::Portal(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    #[test]
    fn lobby_dimensions_are_40x20() {
        let u = lobby();
        assert_eq!(u.map.width, LOBBY_WIDTH);
        assert_eq!(u.map.height, LOBBY_HEIGHT);
    }

    #[test]
    fn lobby_objective_is_in_portuguese() {
        let u = lobby();
        assert_eq!(u.objective.description, "Escolha um universo para entrar");
    }

    #[test]
    fn lobby_has_snake_portal_at_documented_pos() {
        let u = lobby();
        assert_eq!(
            portal_slug(u.map.get_tile(pos::SNAKE.0, pos::SNAKE.1)),
            Some(slug::SNAKE)
        );
    }

    #[test]
    fn lobby_has_tetris_portal_at_documented_pos() {
        let u = lobby();
        assert_eq!(
            portal_slug(u.map.get_tile(pos::TETRIS.0, pos::TETRIS.1)),
            Some(slug::TETRIS)
        );
    }

    #[test]
    fn lobby_has_invaders_portal_at_documented_pos() {
        let u = lobby();
        assert_eq!(
            portal_slug(u.map.get_tile(pos::INVADERS.0, pos::INVADERS.1)),
            Some(slug::INVADERS)
        );
    }

    #[test]
    fn lobby_has_poker_portal_at_documented_pos() {
        let u = lobby();
        assert_eq!(
            portal_slug(u.map.get_tile(pos::POKER.0, pos::POKER.1)),
            Some(slug::POKER)
        );
    }

    #[test]
    fn lobby_does_not_contain_pointset_portal() {
        let u = lobby();
        let any_pointset = (0..u.map.height).any(|y| {
            (0..u.map.width).any(|x| portal_slug(u.map.get_tile(x, y)) == Some("pointset"))
        });
        assert!(!any_pointset, "lobby Yggdrasil não deve conter pointset");
    }

    #[test]
    fn session_teleport_from_lobby_records_transition() {
        let _ = lobby();
        let mut session = Session::new("yggdrasil".to_string());
        assert_eq!(session.current_universe, "yggdrasil");
        assert!(session.history.is_empty());

        session.teleport_to(slug::SNAKE.to_string());

        assert_eq!(session.current_universe, slug::SNAKE);
        assert_eq!(session.history.len(), 1);
        assert_eq!(session.history[0].from, "yggdrasil");
        assert_eq!(session.history[0].to, slug::SNAKE);
    }
}
