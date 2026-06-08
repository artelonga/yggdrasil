//! universe-pointset — SameGame-style flood-fill tile removal game.
//!
//! Grid: 8 columns × 6 rows, colors 1-5.
//! Player clicks a tile; if its connected group (4-directional, same color) has
//! size ≥ 2, all tiles in the group are removed, tiles above fall down, and
//! score += group_size².  Game over when grid is empty or no valid group exists.

use serde::{Deserialize, Serialize};
use universe_sdk::{Universe, UniverseManifest, random_seed};

const COLS: usize = 8;
const ROWS: usize = 6;
const MIN_GROUP: usize = 2;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
struct PointsetSession {
    /// Row-major: grid[row][col].  Row 0 = top.  0 = empty cell.
    grid: [[u8; COLS]; ROWS],
    score: u32,
    is_over: bool,
    initialized: bool,
}

// ---------------------------------------------------------------------------
// xorshift64 RNG (no std deps)
// ---------------------------------------------------------------------------

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

// ---------------------------------------------------------------------------
// Game logic helpers
// ---------------------------------------------------------------------------

/// Flood-fill from (row, col) collecting all connected cells of the same color.
fn find_group(grid: &[[u8; COLS]; ROWS], row: usize, col: usize) -> Vec<(usize, usize)> {
    let color = grid[row][col];
    if color == 0 {
        return vec![];
    }

    let mut visited = [[false; COLS]; ROWS];
    let mut stack = vec![(row, col)];
    let mut group = vec![];

    while let Some((r, c)) = stack.pop() {
        if visited[r][c] {
            continue;
        }
        if grid[r][c] != color {
            continue;
        }
        visited[r][c] = true;
        group.push((r, c));

        // 4-directional neighbors
        if r > 0 {
            stack.push((r - 1, c));
        }
        if r + 1 < ROWS {
            stack.push((r + 1, c));
        }
        if c > 0 {
            stack.push((r, c - 1));
        }
        if c + 1 < COLS {
            stack.push((r, c + 1));
        }
    }

    group
}

/// Apply gravity: in each column, pack non-zero tiles to the bottom.
fn apply_gravity(grid: &mut [[u8; COLS]; ROWS]) {
    // Work column by column using explicit indices so we can borrow individual cells.
    #[allow(clippy::needless_range_loop)]
    for col in 0..COLS {
        // Collect non-empty tiles reading from bottom to top.
        let tiles: Vec<u8> = (0..ROWS)
            .rev()
            .map(|r| grid[r][col])
            .filter(|&v| v != 0)
            .collect();

        // Write back: bottom rows get tiles, upper rows become empty.
        let mut tile_idx = 0;
        for row in (0..ROWS).rev() {
            if tile_idx < tiles.len() {
                grid[row][col] = tiles[tile_idx];
                tile_idx += 1;
            } else {
                grid[row][col] = 0;
            }
        }
    }
}

/// Return true if the grid has at least one connected group of size ≥ MIN_GROUP.
fn has_valid_group(grid: &[[u8; COLS]; ROWS]) -> bool {
    for r in 0..ROWS {
        for c in 0..COLS {
            if grid[r][c] != 0 {
                let group = find_group(grid, r, c);
                if group.len() >= MIN_GROUP {
                    return true;
                }
            }
        }
    }
    false
}

/// Return true if all cells are empty.
fn grid_is_empty(grid: &[[u8; COLS]; ROWS]) -> bool {
    grid.iter().all(|row| row.iter().all(|&v| v == 0))
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

impl PointsetSession {
    fn initialize(&mut self) {
        let mut rng = random_seed();
        if rng == 0 {
            rng = 0xdeadbeef_cafebabe;
        }

        // Keep trying until we have a grid with at least one valid group.
        loop {
            for row in self.grid.iter_mut() {
                for cell in row.iter_mut() {
                    *cell = (xorshift64(&mut rng) % 5 + 1) as u8;
                }
            }
            if has_valid_group(&self.grid) {
                break;
            }
        }

        self.score = 0;
        self.is_over = false;
        self.initialized = true;
    }

    fn check_game_over(&mut self) {
        if grid_is_empty(&self.grid) || !has_valid_group(&self.grid) {
            self.is_over = true;
        }
    }
}

// ---------------------------------------------------------------------------
// Output JSON builder (no macro, hand-rolled for no_std compat)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OutputState<'a> {
    grid: &'a [[u8; COLS]; ROWS],
    score: u32,
    is_over: bool,
    width: usize,
    height: usize,
    error_msg: Option<&'static str>,
}

// ---------------------------------------------------------------------------
// Session trait impl
// ---------------------------------------------------------------------------

impl Default for PointsetSession {
    fn default() -> Self {
        Self {
            grid: [[0u8; COLS]; ROWS],
            score: 0,
            is_over: false,
            initialized: false,
        }
    }
}

impl PointsetSession {
    fn process(&mut self, input: &str) -> String {
        if !self.initialized {
            self.initialize();
        }

        let mut error_msg: Option<&'static str> = None;

        // Parse input
        let parsed: Option<serde_json::Value> = serde_json::from_str(input).ok();
        if let Some(ref v) = parsed
            && let Some(action) = v.get("action").and_then(|a| a.as_str())
            && action == "click"
            && !self.is_over
        {
            let x = v.get("x").and_then(|n| n.as_u64()).map(|n| n as usize);
            let y = v.get("y").and_then(|n| n.as_u64()).map(|n| n as usize);

            if let (Some(col), Some(row)) = (x, y)
                && col < COLS
                && row < ROWS
            {
                let group = find_group(&self.grid, row, col);
                if group.len() >= MIN_GROUP {
                    // Remove group tiles
                    for (r, c) in &group {
                        self.grid[*r][*c] = 0;
                    }
                    let sz = group.len() as u32;
                    self.score += sz * sz;
                    apply_gravity(&mut self.grid);
                    self.check_game_over();
                } else {
                    error_msg = Some("Grupo muito pequeno");
                }
            }
        }

        let out = OutputState {
            grid: &self.grid,
            score: self.score,
            is_over: self.is_over,
            width: COLS,
            height: ROWS,
            error_msg,
        };

        serde_json::to_string(&out).unwrap_or_else(|_| "{}".to_string())
    }
}

impl Universe for PointsetSession {
    fn create(_params: &str) -> Self {
        PointsetSession::default()
    }

    fn tick(&mut self, input: &str) -> String {
        self.process(input)
    }

    fn manifest() -> UniverseManifest {
        UniverseManifest {
            name: "pointset".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            api_version: 1,
            max_players: 1,
            capabilities: vec![],
        }
    }
}

universe_sdk::universe_exports!(PointsetSession);

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Build a session with a known grid for deterministic tests.
    fn session_with_grid(grid: [[u8; COLS]; ROWS]) -> PointsetSession {
        PointsetSession {
            grid,
            score: 0,
            is_over: false,
            initialized: true,
        }
    }

    #[test]
    fn test_initial_grid_is_non_empty() {
        let mut s = PointsetSession::default();
        s.initialize();
        // At least one non-empty cell
        assert!(s.grid.iter().any(|row| row.iter().any(|&v| v != 0)));
        // All cells are valid colors (1-5)
        for row in &s.grid {
            for &cell in row {
                assert!((1..=5).contains(&cell), "cell value out of range: {cell}");
            }
        }
    }

    #[test]
    fn test_has_valid_group_after_init() {
        let mut s = PointsetSession::default();
        s.initialize();
        assert!(has_valid_group(&s.grid));
    }

    #[test]
    fn test_click_on_group_removes_tiles() {
        // Place a 2x2 block of color 1 at rows 4-5, cols 0-1 (bottom-left).
        // Remaining cells are color 2.
        let mut grid = [[2u8; COLS]; ROWS];
        grid[4][0] = 1;
        grid[4][1] = 1;
        grid[5][0] = 1;
        grid[5][1] = 1;

        let mut s = session_with_grid(grid);
        let out = s.process(r#"{"action":"click","x":0,"y":4}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();

        // Score = 4*4 = 16
        assert_eq!(v["score"].as_u64().unwrap(), 16);
        // The group cells should now be empty (or shifted by gravity)
        // After gravity the four empty rows are on top — check row 5 cols 0 and 1
        assert_eq!(v["grid"][5][0].as_u64().unwrap(), 2);
        assert_eq!(v["grid"][5][1].as_u64().unwrap(), 2);
    }

    #[test]
    fn test_isolated_tile_click_is_noop() {
        // A lone tile of color 3 at (row=0, col=7) — all others are color 2.
        let mut grid = [[2u8; COLS]; ROWS];
        grid[0][7] = 3;

        let mut s = session_with_grid(grid);
        let original_score = s.score;
        let out = s.process(r#"{"action":"click","x":7,"y":0}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();

        // Score unchanged
        assert_eq!(v["score"].as_u64().unwrap(), original_score as u64);
        // Error message present
        assert_eq!(v["error_msg"].as_str().unwrap(), "Grupo muito pequeno");
        // Grid unchanged at that position
        assert_eq!(v["grid"][0][7].as_u64().unwrap(), 3);
    }

    #[test]
    fn test_tiles_fall_after_removal() {
        // Column 0: top rows empty, then tile 1 at row 2, then color 2 at rows 3-5.
        // Remove the pair at rows 3-4 col 0 (color 2), leaving tile 1 at row 2.
        // After gravity tile 1 should be at row 5.
        let mut grid = [[0u8; COLS]; ROWS];
        grid[2][0] = 1;
        grid[3][0] = 2;
        grid[4][0] = 2;
        grid[5][0] = 2;
        // Need at least one more group of 2 elsewhere so has_valid_group stays true for other cols
        // but we only care about col 0 behavior here.

        let mut s = session_with_grid(grid);
        // Click on the group of three color-2 tiles (x=0, y=3)
        let out = s.process(r#"{"action":"click","x":0,"y":3}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();

        // Tile 1 should have fallen to row 5 col 0
        assert_eq!(v["grid"][5][0].as_u64().unwrap(), 1);
        // Rows 0-4 col 0 should be empty
        for r in 0..5usize {
            assert_eq!(
                v["grid"][r][0].as_u64().unwrap(),
                0,
                "row {r} col 0 should be empty"
            );
        }
        // Score = 3*3 = 9
        assert_eq!(v["score"].as_u64().unwrap(), 9);
    }

    #[test]
    fn test_gravity_packs_to_bottom() {
        // Simple gravity test on an isolated grid
        let mut grid = [[0u8; COLS]; ROWS];
        grid[0][0] = 5;
        grid[2][0] = 3;
        grid[4][0] = 1;
        apply_gravity(&mut grid);
        // Should be packed: row5=1, row4=3, row3=5, rows 0-2 empty
        assert_eq!(grid[5][0], 1);
        assert_eq!(grid[4][0], 3);
        assert_eq!(grid[3][0], 5);
        assert_eq!(grid[0][0], 0);
        assert_eq!(grid[1][0], 0);
        assert_eq!(grid[2][0], 0);
    }

    #[test]
    fn test_game_over_when_no_valid_groups() {
        // A grid where every tile is isolated (checkerboard of alternating colors).
        // With 5 colors and 8 cols × 6 rows, a carefully designed grid has no group of 2.
        // Simplest: make each adjacent pair a different color — checkerboard of 1 and 2
        // but a checkerboard with exactly 2 colors still forms diagonal isolation.
        // We need 4-directional isolation: use column offset trick.
        let mut grid = [[0u8; COLS]; ROWS];
        for r in 0..ROWS {
            for c in 0..COLS {
                // Every tile is different from its left/right/up/down neighbors.
                // Use (r + c) mod 5 + 1 — with 5 colors on 8×6 grid, adjacent tiles differ.
                grid[r][c] = ((r + c) % 5 + 1) as u8;
            }
        }
        // Verify no group of ≥ 2 exists
        for r in 0..ROWS {
            for c in 0..COLS {
                let g = find_group(&grid, r, c);
                assert!(g.len() < 2, "unexpected group at ({r},{c}): {:?}", g);
            }
        }
        let s = session_with_grid(grid);
        // Grid is full but has no valid groups → game_over check should detect it
        assert!(!has_valid_group(&s.grid));
    }

    #[test]
    fn test_game_over_when_grid_is_empty() {
        let grid = [[0u8; COLS]; ROWS];
        assert!(grid_is_empty(&grid));
        // is_over flag should be set
        let mut s = session_with_grid(grid);
        s.check_game_over();
        assert!(s.is_over);
    }

    #[test]
    fn test_find_group_single_cell() {
        let mut grid = [[0u8; COLS]; ROWS];
        grid[3][3] = 2;
        let g = find_group(&grid, 3, 3);
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn test_find_group_connected() {
        let mut grid = [[0u8; COLS]; ROWS];
        grid[0][0] = 1;
        grid[0][1] = 1;
        grid[1][0] = 1;
        let g = find_group(&grid, 0, 0);
        assert_eq!(g.len(), 3);
    }

    #[test]
    fn test_output_has_correct_dimensions() {
        let mut s = PointsetSession::default();
        s.initialize();
        let out = s.process("");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["width"].as_u64().unwrap(), 8);
        assert_eq!(v["height"].as_u64().unwrap(), 6);
        assert_eq!(v["grid"].as_array().unwrap().len(), 6);
        assert_eq!(v["grid"][0].as_array().unwrap().len(), 8);
    }
}
