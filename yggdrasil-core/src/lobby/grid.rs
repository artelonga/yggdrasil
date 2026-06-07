/// Largura do mapa do lobby em tiles.
pub const LOBBY_WIDTH: usize = 40;
/// Altura do mapa do lobby em tiles.
pub const LOBBY_HEIGHT: usize = 20;

/// Posição (x, y) de cada portal no grid.
pub mod pos {
    pub const SNAKE: (usize, usize) = (10, 8);
    pub const TETRIS: (usize, usize) = (30, 8);
    pub const INVADERS: (usize, usize) = (10, 12);
    pub const POKER: (usize, usize) = (30, 12);
    /// Neuro — atlas 3D de anatomia (YG-85). Portal central.
    pub const NEURO: (usize, usize) = (20, 10);
    /// Comunicação — salas de léxico. Portal central-inferior.
    pub const COMUNICACAO: (usize, usize) = (20, 15);
}
