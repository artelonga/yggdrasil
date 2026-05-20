//! Universo Vim — emulador modal Rust + 10 níveis progressivos.
//!
//! Esta crate é usada em dois contextos:
//! - Nativa: importada pelo `yggdrasil-web` para as rotas HTTP.
//! - WASM: compilada com target `wasm32-unknown-unknown` para o artefato
//!   `embedded/vim.wasm` (gerado pelo `build-universes.sh` — YG-61).
//!   As exportações WASM ficam por trás de `#[cfg(target_arch = "wasm32")]`.

pub mod editor;
pub mod levels;

pub use editor::{CommandResult, Editor, GameState, Mode};
pub use levels::{Level, all_levels, base_reward};

use std::collections::HashMap;

/// Sessão de jogo (usada pelo servidor HTTP nativo).
pub struct VimSession {
    pub editor: Editor,
    pub level: usize,
    pub score: u64,
    pub hint_count: usize,
    pub completed_levels: Vec<usize>,
    pub bonus_claimed: bool,
}

impl VimSession {
    /// Cria uma nova sessão começando no nível 1.
    pub fn new() -> Self {
        let levels = all_levels();
        let l = &levels[0];
        VimSession {
            editor: Editor::new(l.initial_text, l.cursor_start),
            level: 1,
            score: 0,
            hint_count: 0,
            completed_levels: Vec::new(),
            bonus_claimed: false,
        }
    }

    /// Processa uma tecla e retorna o GameState resultante.
    pub fn send_key(&mut self, key: &str) -> GameState {
        let result = self.editor.send_key(key);
        let mut completed_level = None;
        let mut game_completed = false;

        let level_number = self.level;

        if matches!(result, CommandResult::Save | CommandResult::SaveAndQuit) {
            // Verifica se o nível está completo
            let levels = all_levels();
            if level_number >= 1 && level_number <= levels.len() {
                let l = &levels[level_number - 1];
                let success = (l.success_fn)(&self.editor.lines, self.editor.cursor);
                if success && !self.completed_levels.contains(&level_number) {
                    completed_level = Some(level_number);
                    self.completed_levels.push(level_number);

                    // Calcula recompensa
                    let mut reward = base_reward(level_number);
                    if self.hint_count == 0 {
                        reward += reward / 2; // +50% sem hints
                    }
                    self.score += reward;

                    // Bônus de conclusão completa
                    if self.completed_levels.len() == 10 && !self.bonus_claimed {
                        self.score += 100;
                        self.bonus_claimed = true;
                        game_completed = true;
                    }

                    // Avança para o próximo nível
                    if level_number < 10 {
                        self.level = level_number + 1;
                        self.hint_count = 0;
                        let next_l = &levels[self.level - 1];
                        self.editor = Editor::new(next_l.initial_text, next_l.cursor_start);
                    } else {
                        game_completed = true;
                    }
                }
            }
        }

        if matches!(result, CommandResult::Quit | CommandResult::SaveAndQuit) {
            // :q finaliza o jogo
            game_completed = true;
        }

        self.build_state(completed_level, game_completed)
    }

    fn build_state(&self, completed_level: Option<usize>, game_completed: bool) -> GameState {
        let selection = self.editor.visual_selection();
        let mode_name = self.editor.mode.name().to_string();
        let status = if let Some(ref err) = self.editor.error {
            err.clone()
        } else {
            self.editor.status_line.clone()
        };

        let levels = all_levels();
        let hint = if self.level >= 1 && self.level <= levels.len() {
            Some(levels[self.level - 1].hint.to_string())
        } else {
            None
        };

        GameState {
            lines: self.editor.lines.clone(),
            cursor: self.editor.cursor,
            mode: mode_name,
            selection,
            status_line: status,
            hint,
            level: self.level,
            completed_level,
            score: self.score,
            game_completed,
        }
    }

    /// Retorna o estado atual sem processar tecla.
    pub fn state(&self) -> GameState {
        self.build_state(None, false)
    }
}

impl Default for VimSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Store simples de sessões em memória (alias para uso no servidor).
pub type SessionMap = HashMap<String, VimSession>;

// ── Exports WASM (apenas quando compilado para wasm32) ───────────────────────

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use std::sync::Mutex;

    static SESSION: Mutex<Option<VimSession>> = Mutex::new(None);

    #[unsafe(no_mangle)]
    pub extern "C" fn new_session() {
        let mut lock = SESSION.lock().unwrap();
        *lock = Some(VimSession::new());
    }

    /// Envia uma tecla para a sessão WASM. Retorna 0 em sucesso.
    #[unsafe(no_mangle)]
    pub extern "C" fn send_key_wasm(ptr: *const u8, len: usize) -> i32 {
        let key = unsafe {
            let slice = std::slice::from_raw_parts(ptr, len);
            match std::str::from_utf8(slice) {
                Ok(s) => s,
                Err(_) => return -1,
            }
        };
        let mut lock = SESSION.lock().unwrap();
        if let Some(ref mut session) = *lock {
            session.send_key(key);
            0
        } else {
            -1
        }
    }

    /// Serializa o estado da sessão como JSON na memória linear.
    /// Retorna o tamanho do JSON ou 0 em caso de erro.
    #[unsafe(no_mangle)]
    pub extern "C" fn get_state(out_ptr: *mut u8, out_len: usize) -> usize {
        let lock = SESSION.lock().unwrap();
        if let Some(ref session) = *lock {
            let state = session.state();
            let json = match serde_json::to_string(&state) {
                Ok(s) => s,
                Err(_) => return 0,
            };
            let bytes = json.as_bytes();
            let write_len = bytes.len().min(out_len);
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, write_len);
            }
            write_len
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new_starts_level1() {
        let s = VimSession::new();
        assert_eq!(s.level, 1);
        assert_eq!(s.score, 0);
    }

    #[test]
    fn test_session_state_initial() {
        let s = VimSession::new();
        let state = s.state();
        assert_eq!(state.level, 1);
        assert_eq!(state.score, 0);
        assert!(!state.game_completed);
    }

    #[test]
    fn test_send_key_h_moves_left_but_stays() {
        let mut s = VimSession::new();
        let state = s.send_key("h");
        // cursor at (0,0) — h stays put
        assert_eq!(state.cursor, (0, 0));
    }

    #[test]
    fn test_level1_completion() {
        let mut s = VimSession::new();
        // Level 1: navigate to (2, 4) then :w
        // j j = row 2
        s.send_key("j");
        s.send_key("j");
        // l l l l = col 4
        s.send_key("l");
        s.send_key("l");
        s.send_key("l");
        s.send_key("l");
        // :w
        s.send_key(":");
        s.send_key("w");
        let state = s.send_key("Enter");
        assert_eq!(state.completed_level, Some(1));
        assert_eq!(state.level, 2);
    }

    #[test]
    fn test_level1_no_completion_wrong_position() {
        let mut s = VimSession::new();
        s.send_key(":");
        s.send_key("w");
        let state = s.send_key("Enter");
        assert_eq!(state.completed_level, None);
        assert_eq!(state.level, 1);
    }

    #[test]
    fn test_score_increases_on_level_completion() {
        let mut s = VimSession::new();
        s.send_key("j");
        s.send_key("j");
        s.send_key("l");
        s.send_key("l");
        s.send_key("l");
        s.send_key("l");
        s.send_key(":");
        s.send_key("w");
        let state = s.send_key("Enter");
        // Level 1 reward = 10, no hints = +50% = 15
        assert!(state.score > 0);
    }

    #[test]
    fn test_level2_completion() {
        let mut s = VimSession::new();
        // Skip to level 2 by completing level 1
        s.send_key("j");
        s.send_key("j");
        s.send_key("l");
        s.send_key("l");
        s.send_key("l");
        s.send_key("l");
        s.send_key(":");
        s.send_key("w");
        s.send_key("Enter");
        assert_eq!(s.level, 2);

        // Level 2: delete the 'x' on line 0 col 14
        // Navigate to position of 'x'
        s.send_key("$"); // go to end of line
        // Last char should be 'x'
        s.send_key("x"); // delete it

        s.send_key(":");
        s.send_key("w");
        let state = s.send_key("Enter");
        assert_eq!(state.completed_level, Some(2));
    }

    #[test]
    fn test_quit_ends_game() {
        let mut s = VimSession::new();
        s.send_key(":");
        s.send_key("q");
        let state = s.send_key("Enter");
        assert!(state.game_completed);
    }

    #[test]
    fn test_hint_in_state() {
        let s = VimSession::new();
        let state = s.state();
        assert!(state.hint.is_some());
    }
}
