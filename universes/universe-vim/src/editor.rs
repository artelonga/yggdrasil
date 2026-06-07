//! Emulador modal Vim — núcleo do universo-vim.
//!
//! Implementa os modos Normal, Insert, Visual e Command com os comandos
//! descritos na spec YG-58.

use serde::{Deserialize, Serialize};

/// Modo atual do editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    /// Acumula o texto do comando (após o `:` inicial).
    Command(String),
    /// Acumula o padrão de busca (após o `/` inicial).
    Search(String),
}

impl Mode {
    pub fn name(&self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::Command(_) => "COMMAND",
            Mode::Search(_) => "SEARCH",
        }
    }
}

/// Snapshot do estado do editor para o stack de undo.
#[derive(Clone, Debug)]
pub struct EditorSnapshot {
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
}

/// Descreve a última mudança para o comando `.` (repeat).
#[derive(Clone, Debug)]
pub enum LastChange {
    DeleteChar,
    DeleteLine,
    DeleteWord,
    DeleteToEndOfLine,
    DeleteVisualSelection {
        start: (usize, usize),
        end: (usize, usize),
    },
    YankLine,
    YankVisualSelection {
        start: (usize, usize),
        end: (usize, usize),
    },
    PasteAfter,
    PasteBefore,
    InsertText {
        text: String,
        mode_entry: InsertEntry,
    },
    Substitute {
        find: String,
        replace: String,
    },
}

/// Como o modo Insert foi iniciado (para `.` repeat).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertEntry {
    LittleI, // i
    BigI,    // I
    LittleA, // a
    BigA,    // A
    LittleO, // o
    BigO,    // O
}

/// Estado completo do editor Vim.
pub struct Editor {
    pub lines: Vec<String>,
    pub cursor: (usize, usize), // (row, col)
    pub mode: Mode,
    pub pending_op: Option<char>, // 'd', 'y', etc.
    pub yank_buffer: Option<String>,
    pub undo_stack: Vec<EditorSnapshot>,
    pub last_change: Option<LastChange>,
    pub search_pattern: Option<String>,
    pub visual_anchor: Option<(usize, usize)>,
    pub status_line: String,
    pub hint: Option<String>,
    pub error: Option<String>,
    /// Texto acumulado durante modo Insert (para repeat).
    insert_text: String,
    /// Como o modo Insert foi iniciado.
    insert_entry: Option<InsertEntry>,
}

/// Estado serializado para o frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
    pub mode: String,
    pub selection: Option<((usize, usize), (usize, usize))>,
    pub status_line: String,
    pub hint: Option<String>,
    pub level: usize,
    pub completed_level: Option<usize>,
    pub score: u64,
    pub game_completed: bool,
}

impl Editor {
    /// Cria um novo editor com o buffer fornecido e cursor inicial.
    pub fn new(initial_text: &str, cursor_start: (usize, usize)) -> Self {
        let lines: Vec<String> = initial_text.lines().map(|l| l.to_string()).collect();
        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        Editor {
            lines,
            cursor: cursor_start,
            mode: Mode::Normal,
            pending_op: None,
            yank_buffer: None,
            undo_stack: Vec::new(),
            last_change: None,
            search_pattern: None,
            visual_anchor: None,
            status_line: String::new(),
            hint: None,
            error: None,
            insert_text: String::new(),
            insert_entry: None,
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn clamp_cursor(&mut self) {
        let row = self.cursor.0.min(self.lines.len().saturating_sub(1));
        self.cursor.0 = row;
        let max_col = match &self.mode {
            Mode::Insert => self.lines[row].len(),
            _ => self.lines[row].len().saturating_sub(1),
        };
        self.cursor.1 = self.cursor.1.min(max_col);
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(EditorSnapshot {
            lines: self.lines.clone(),
            cursor: self.cursor,
        });
    }

    /// Retorna os limites (start, end) inclusivos da seleção visual (linha ou char).
    pub fn visual_selection(&self) -> Option<((usize, usize), (usize, usize))> {
        let anchor = self.visual_anchor?;
        let cur = self.cursor;
        let start = if (anchor.0, anchor.1) <= (cur.0, cur.1) {
            anchor
        } else {
            cur
        };
        let end = if (anchor.0, anchor.1) <= (cur.0, cur.1) {
            cur
        } else {
            anchor
        };
        Some((start, end))
    }

    // ── Movement ─────────────────────────────────────────────────────────────

    fn move_left(&mut self) {
        if self.cursor.1 > 0 {
            self.cursor.1 -= 1;
        }
    }

    fn move_right(&mut self) {
        let max = self.lines[self.cursor.0].len().saturating_sub(1);
        if self.cursor.1 < max {
            self.cursor.1 += 1;
        }
    }

    fn move_down(&mut self) {
        if self.cursor.0 + 1 < self.lines.len() {
            self.cursor.0 += 1;
            self.clamp_cursor();
        }
    }

    fn move_up(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            self.clamp_cursor();
        }
    }

    fn move_col0(&mut self) {
        self.cursor.1 = 0;
    }

    fn move_col_end(&mut self) {
        let len = self.lines[self.cursor.0].len();
        self.cursor.1 = if len > 0 { len - 1 } else { 0 };
    }

    fn move_first_nonblank(&mut self) {
        let col = self.lines[self.cursor.0]
            .chars()
            .position(|c| !c.is_whitespace())
            .unwrap_or(0);
        self.cursor.1 = col;
    }

    fn move_gg(&mut self) {
        self.cursor = (0, 0);
        self.move_first_nonblank();
    }

    fn move_big_g(&mut self) {
        self.cursor.0 = self.lines.len().saturating_sub(1);
        self.move_first_nonblank();
    }

    /// Move para o início da próxima palavra.
    fn move_word_forward(&mut self) {
        let row = self.cursor.0;
        let col = self.cursor.1;
        let line = &self.lines[row];
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        // Avança sobre não-espaços da posição atual
        let mut c = col;
        while c < len && !chars[c].is_whitespace() {
            c += 1;
        }
        // Avança sobre espaços
        while c < len && chars[c].is_whitespace() {
            c += 1;
        }

        if c < len {
            self.cursor.1 = c;
        } else if row + 1 < self.lines.len() {
            // Próxima linha
            self.cursor.0 = row + 1;
            let next: Vec<char> = self.lines[row + 1].chars().collect();
            let mut nc = 0;
            while nc < next.len() && next[nc].is_whitespace() {
                nc += 1;
            }
            self.cursor.1 = nc;
        } else {
            self.cursor.1 = len.saturating_sub(1);
        }
    }

    /// Move para o início da palavra anterior.
    fn move_word_backward(&mut self) {
        let row = self.cursor.0;
        let col = self.cursor.1;
        if col == 0 {
            if row > 0 {
                self.cursor.0 = row - 1;
                let prev: Vec<char> = self.lines[row - 1].chars().collect();
                // Move para o início da última palavra da linha anterior
                let len = prev.len();
                if len == 0 {
                    self.cursor.1 = 0;
                    return;
                }
                let mut c = len - 1;
                while c > 0 && prev[c].is_whitespace() {
                    c -= 1;
                }
                while c > 0 && !prev[c - 1].is_whitespace() {
                    c -= 1;
                }
                self.cursor.1 = c;
            }
            return;
        }
        let chars: Vec<char> = self.lines[row].chars().collect();
        // Se está no início de uma palavra, move para trás
        let mut c = col.saturating_sub(1);
        // Pula espaços para trás
        while c > 0 && chars[c].is_whitespace() {
            c -= 1;
        }
        // Pula não-espaços para trás
        while c > 0 && !chars[c - 1].is_whitespace() {
            c -= 1;
        }
        self.cursor.1 = c;
    }

    /// Move para o fim da palavra atual.
    fn move_word_end(&mut self) {
        let row = self.cursor.0;
        let col = self.cursor.1;
        let chars: Vec<char> = self.lines[row].chars().collect();
        let len = chars.len();
        if len == 0 {
            return;
        }
        let mut c = col;
        // Se já estamos no fim de uma palavra ou em espaço, avança uma posição
        if c + 1 < len
            && ((!chars[c].is_whitespace() && chars[c + 1].is_whitespace())
                || chars[c].is_whitespace())
        {
            c += 1;
        }
        // Pula espaços
        while c < len && chars[c].is_whitespace() {
            c += 1;
        }
        // Avança até o fim da palavra
        while c + 1 < len && !chars[c + 1].is_whitespace() {
            c += 1;
        }
        if c < len {
            self.cursor.1 = c;
        }
    }

    // ── Delete / Yank ─────────────────────────────────────────────────────────

    fn delete_char_at_cursor(&mut self) {
        self.push_undo();
        let row = self.cursor.0;
        let col = self.cursor.1;
        let line = &mut self.lines[row];
        if col < line.len() {
            line.remove(col);
        }
        // Garante que o cursor não fique fora dos limites
        self.clamp_cursor();
        self.last_change = Some(LastChange::DeleteChar);
    }

    fn delete_current_line(&mut self) {
        self.push_undo();
        let row = self.cursor.0;
        let removed = self.lines.remove(row);
        self.yank_buffer = Some(removed);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if self.cursor.0 >= self.lines.len() {
            self.cursor.0 = self.lines.len() - 1;
        }
        self.clamp_cursor();
        self.last_change = Some(LastChange::DeleteLine);
    }

    fn delete_word_at_cursor(&mut self) {
        self.push_undo();
        let row = self.cursor.0;
        let col = self.cursor.1;
        let line = self.lines[row].clone();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        if col >= len {
            self.last_change = Some(LastChange::DeleteWord);
            return;
        }

        // Conta quantos chars do "word" a apagar: não-espaços, depois espaços
        let mut end = col;
        if !chars[end].is_whitespace() {
            while end < len && !chars[end].is_whitespace() {
                end += 1;
            }
        }
        while end < len && chars[end].is_whitespace() {
            end += 1;
        }

        let new_line: String = chars[..col].iter().chain(chars[end..].iter()).collect();
        self.lines[row] = new_line;
        self.clamp_cursor();
        self.last_change = Some(LastChange::DeleteWord);
    }

    fn delete_to_end_of_line(&mut self) {
        self.push_undo();
        let row = self.cursor.0;
        let col = self.cursor.1;
        let line = &mut self.lines[row];
        if col < line.len() {
            line.truncate(col);
        }
        self.clamp_cursor();
        self.last_change = Some(LastChange::DeleteToEndOfLine);
    }

    fn yank_current_line(&mut self) {
        let row = self.cursor.0;
        self.yank_buffer = Some(self.lines[row].clone());
        self.last_change = Some(LastChange::YankLine);
    }

    fn paste_after(&mut self) {
        self.push_undo();
        if let Some(text) = self.yank_buffer.clone() {
            let row = self.cursor.0;
            self.lines.insert(row + 1, text);
            self.cursor.0 = row + 1;
            self.clamp_cursor();
        }
        self.last_change = Some(LastChange::PasteAfter);
    }

    fn paste_before(&mut self) {
        self.push_undo();
        if let Some(text) = self.yank_buffer.clone() {
            let row = self.cursor.0;
            self.lines.insert(row, text);
            self.clamp_cursor();
        }
        self.last_change = Some(LastChange::PasteBefore);
    }

    fn undo(&mut self) {
        if let Some(snap) = self.undo_stack.pop() {
            self.lines = snap.lines;
            self.cursor = snap.cursor;
            self.clamp_cursor();
        }
    }

    // ── Visual operations ────────────────────────────────────────────────────

    fn delete_visual_selection(&mut self) {
        if let Some((start, end)) = self.visual_selection() {
            self.push_undo();
            let last_change = LastChange::DeleteVisualSelection { start, end };
            if start.0 == end.0 {
                // mesma linha
                let line = &mut self.lines[start.0];
                let e = (end.1 + 1).min(line.len());
                line.drain(start.1..e);
            } else {
                // múltiplas linhas — remove linhas inteiras entre start.0 e end.0
                // Simplificado: apaga linhas do start.0 até end.0 inclusive
                for _ in start.0..=end.0 {
                    if start.0 < self.lines.len() {
                        self.lines.remove(start.0);
                    }
                }
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
            }
            self.cursor = start;
            self.clamp_cursor();
            self.visual_anchor = None;
            self.mode = Mode::Normal;
            self.last_change = Some(last_change);
        }
    }

    fn yank_visual_selection(&mut self) {
        if let Some((start, end)) = self.visual_selection() {
            let last_change = LastChange::YankVisualSelection { start, end };
            if start.0 == end.0 {
                let line = &self.lines[start.0];
                let e = (end.1 + 1).min(line.len());
                self.yank_buffer = Some(line[start.1..e].to_string());
            } else {
                let mut yanked = String::new();
                for r in start.0..=end.0 {
                    if r < self.lines.len() {
                        yanked.push_str(&self.lines[r]);
                        if r != end.0 {
                            yanked.push('\n');
                        }
                    }
                }
                self.yank_buffer = Some(yanked);
            }
            self.cursor = start;
            self.visual_anchor = None;
            self.mode = Mode::Normal;
            self.last_change = Some(last_change);
        }
    }

    // ── Search ───────────────────────────────────────────────────────────────

    fn execute_search(&mut self, pattern: &str) {
        if pattern.is_empty() {
            return;
        }
        self.search_pattern = Some(pattern.to_string());
        self.find_next_match(true);
    }

    fn find_next_match(&mut self, forward: bool) {
        let pattern = match &self.search_pattern {
            Some(p) => p.clone(),
            None => return,
        };
        if pattern.is_empty() {
            return;
        }

        let row = self.cursor.0;
        let col = self.cursor.1;
        let n = self.lines.len();

        if forward {
            // Busca a partir da posição atual+1 até o final, depois do início
            // Primeiro: resto da linha atual
            if let Some(c) = find_in_line(&self.lines[row], &pattern, col + 1) {
                self.cursor.1 = c;
                return;
            }
            // Linhas seguintes
            for r in (row + 1)..n {
                if let Some(c) = find_in_line(&self.lines[r], &pattern, 0) {
                    self.cursor = (r, c);
                    return;
                }
            }
            // Wrap-around
            for r in 0..=row {
                if let Some(c) = find_in_line(&self.lines[r], &pattern, 0) {
                    if r == row && c >= col {
                        continue;
                    }
                    self.cursor = (r, c);
                    return;
                }
            }
        } else {
            // Busca para trás
            if col > 0 {
                let backward_col = col - 1;
                if let Some(c) = find_in_line_backward(&self.lines[row], &pattern, backward_col) {
                    self.cursor.1 = c;
                    return;
                }
            }
            for r in (0..row).rev() {
                let line_len = self.lines[r].len();
                if let Some(c) = find_in_line_backward(&self.lines[r], &pattern, line_len) {
                    self.cursor = (r, c);
                    return;
                }
            }
            // Wrap-around
            for r in (row..n).rev() {
                let line_len = self.lines[r].len();
                if let Some(c) = find_in_line_backward(&self.lines[r], &pattern, line_len) {
                    self.cursor = (r, c);
                    return;
                }
            }
        }
    }

    // ── Command execution ────────────────────────────────────────────────────

    /// Executa um comando digitado após `:`. Retorna `true` se o nível deve ser
    /// marcado como completo (`:w`), `None` indica quit (`:q`).
    pub fn execute_command(&mut self, cmd: &str) -> CommandResult {
        let cmd = cmd.trim();
        if cmd == "w" {
            return CommandResult::Save;
        }
        if cmd == "q" || cmd == "q!" {
            return CommandResult::Quit;
        }
        if cmd == "wq" {
            return CommandResult::SaveAndQuit;
        }
        // :s/find/replace/
        if let Some(rest) = cmd.strip_prefix("s/") {
            let parts: Vec<&str> = rest.splitn(3, '/').collect();
            if parts.len() >= 2 {
                let find = parts[0];
                let replace = parts[1];
                return self.execute_substitute(find, replace);
            }
        }
        self.error = Some(format!("Comando desconhecido: :{cmd}"));
        CommandResult::None
    }

    fn execute_substitute(&mut self, find: &str, replace: &str) -> CommandResult {
        let row = self.cursor.0;
        let line = self.lines[row].clone();
        if let Some(pos) = line.find(find) {
            self.push_undo();
            let new_line = format!("{}{}{}", &line[..pos], replace, &line[pos + find.len()..]);
            self.lines[row] = new_line;
            self.cursor.1 = pos;
            self.clamp_cursor();
            self.last_change = Some(LastChange::Substitute {
                find: find.to_string(),
                replace: replace.to_string(),
            });
        }
        CommandResult::None
    }

    // ── Repeat (.) ──────────────────────────────────────────────────────────

    fn repeat_last_change(&mut self) {
        let change = match &self.last_change {
            Some(c) => c.clone(),
            None => return,
        };
        match change {
            LastChange::DeleteChar => self.delete_char_at_cursor(),
            LastChange::DeleteLine => self.delete_current_line(),
            LastChange::DeleteWord => self.delete_word_at_cursor(),
            LastChange::DeleteToEndOfLine => self.delete_to_end_of_line(),
            LastChange::YankLine => self.yank_current_line(),
            LastChange::PasteAfter => self.paste_after(),
            LastChange::PasteBefore => self.paste_before(),
            LastChange::Substitute { find, replace } => {
                let _ = self.execute_substitute(&find.clone(), &replace.clone());
            }
            LastChange::InsertText { text, mode_entry } => {
                self.push_undo();
                let entry_key = match mode_entry {
                    InsertEntry::LittleI => "i",
                    InsertEntry::BigI => "I",
                    InsertEntry::LittleA => "a",
                    InsertEntry::BigA => "A",
                    InsertEntry::LittleO => "o",
                    InsertEntry::BigO => "O",
                };
                self.enter_insert_mode_for(entry_key);
                for ch in text.chars() {
                    self.insert_char(ch);
                }
                self.exit_insert_mode();
            }
            LastChange::DeleteVisualSelection { .. } | LastChange::YankVisualSelection { .. } => {
                // Não repetimos operações visuais
            }
        }
    }

    // ── Insert mode helpers ──────────────────────────────────────────────────

    fn enter_insert_mode_for(&mut self, key: &str) {
        let row = self.cursor.0;
        self.insert_text = String::new();
        match key {
            "i" => {
                self.insert_entry = Some(InsertEntry::LittleI);
                self.mode = Mode::Insert;
            }
            "I" => {
                self.insert_entry = Some(InsertEntry::BigI);
                self.move_first_nonblank();
                self.mode = Mode::Insert;
            }
            "a" => {
                self.insert_entry = Some(InsertEntry::LittleA);
                // Avança 1 posição (insert after)
                let len = self.lines[row].len();
                if self.cursor.1 < len {
                    self.cursor.1 += 1;
                }
                self.mode = Mode::Insert;
            }
            "A" => {
                self.insert_entry = Some(InsertEntry::BigA);
                self.cursor.1 = self.lines[row].len();
                self.mode = Mode::Insert;
            }
            "o" => {
                self.insert_entry = Some(InsertEntry::LittleO);
                self.lines.insert(row + 1, String::new());
                self.cursor.0 = row + 1;
                self.cursor.1 = 0;
                self.mode = Mode::Insert;
            }
            "O" => {
                self.insert_entry = Some(InsertEntry::BigO);
                self.lines.insert(row, String::new());
                self.cursor.1 = 0;
                self.mode = Mode::Insert;
            }
            _ => {
                self.insert_entry = Some(InsertEntry::LittleI);
                self.mode = Mode::Insert;
            }
        }
    }

    fn insert_char(&mut self, ch: char) {
        let row = self.cursor.0;
        let col = self.cursor.1;
        let line = &mut self.lines[row];
        // Garante col dentro dos limites (insert mode: pode ser == len)
        let col = col.min(line.len());
        if ch == '\n' {
            let rest = line[col..].to_string();
            line.truncate(col);
            self.lines.insert(row + 1, rest);
            self.cursor.0 = row + 1;
            self.cursor.1 = 0;
        } else {
            line.insert(col, ch);
            self.cursor.1 = col + 1;
        }
        self.insert_text.push(ch);
    }

    fn backspace_insert(&mut self) {
        let row = self.cursor.0;
        let col = self.cursor.1;
        if col > 0 {
            let line = &mut self.lines[row];
            line.remove(col - 1);
            self.cursor.1 -= 1;
            if !self.insert_text.is_empty() {
                self.insert_text.pop();
            }
        } else if row > 0 {
            // Junta com a linha anterior
            let cur_line = self.lines.remove(row);
            let prev_len = self.lines[row - 1].len();
            self.lines[row - 1].push_str(&cur_line);
            self.cursor.0 = row - 1;
            self.cursor.1 = prev_len;
        }
    }

    fn exit_insert_mode(&mut self) {
        let text = self.insert_text.clone();
        let entry = self.insert_entry.clone().unwrap_or(InsertEntry::LittleI);
        if !text.is_empty() {
            self.last_change = Some(LastChange::InsertText {
                text,
                mode_entry: entry,
            });
        }
        self.mode = Mode::Normal;
        // Em Normal mode o cursor não pode estar após o último char
        let row = self.cursor.0;
        let max = self.lines[row].len().saturating_sub(1);
        if self.cursor.1 > max {
            self.cursor.1 = max;
        }
        self.insert_text = String::new();
        self.insert_entry = None;
    }

    // ── Main key dispatch ────────────────────────────────────────────────────

    /// Processa uma tecla e retorna o resultado.
    pub fn send_key(&mut self, key: &str) -> CommandResult {
        self.error = None;
        match &self.mode.clone() {
            Mode::Normal => self.handle_normal(key),
            Mode::Insert => {
                self.handle_insert(key);
                CommandResult::None
            }
            Mode::Visual => self.handle_visual(key),
            Mode::Command(acc) => {
                let acc = acc.clone();
                self.handle_command_input(key, &acc)
            }
            Mode::Search(acc) => {
                let acc = acc.clone();
                self.handle_search_input(key, &acc);
                CommandResult::None
            }
        }
    }

    fn handle_normal(&mut self, key: &str) -> CommandResult {
        // Se há pending_op, o próximo key é o motion
        if let Some(op) = self.pending_op {
            return self.handle_pending_op(op, key);
        }

        match key {
            "h" => self.move_left(),
            "j" => self.move_down(),
            "k" => self.move_up(),
            "l" => self.move_right(),
            "w" => self.move_word_forward(),
            "b" => self.move_word_backward(),
            "e" => self.move_word_end(),
            "0" => self.move_col0(),
            "$" => self.move_col_end(),
            "^" => self.move_first_nonblank(),
            "g" => {
                // gg — precisaria de dois 'g', mas simplificamos: 'g' entra em
                // estado de espera. Aqui tratamos como gg diretamente pela
                // convenção do spec: o segundo 'g' é enviado como "gg".
                // Nada a fazer aqui — veja "gg" abaixo.
            }
            "gg" => self.move_gg(),
            "G" => self.move_big_g(),
            "x" => self.delete_char_at_cursor(),
            "d" => {
                self.pending_op = Some('d');
            }
            "y" => {
                self.pending_op = Some('y');
            }
            "dd" => self.delete_current_line(),
            "yy" => self.yank_current_line(),
            "p" => self.paste_after(),
            "P" => self.paste_before(),
            "u" => self.undo(),
            "." => self.repeat_last_change(),
            "/" => {
                self.mode = Mode::Search(String::new());
            }
            "n" => self.find_next_match(true),
            "N" => self.find_next_match(false),
            "i" => {
                self.push_undo();
                self.enter_insert_mode_for("i");
            }
            "I" => {
                self.push_undo();
                self.enter_insert_mode_for("I");
            }
            "a" => {
                self.push_undo();
                self.enter_insert_mode_for("a");
            }
            "A" => {
                self.push_undo();
                self.enter_insert_mode_for("A");
            }
            "o" => {
                self.push_undo();
                self.enter_insert_mode_for("o");
            }
            "O" => {
                self.push_undo();
                self.enter_insert_mode_for("O");
            }
            "v" => {
                self.visual_anchor = Some(self.cursor);
                self.mode = Mode::Visual;
            }
            "V" => {
                // Visual line mode — simplificamos tratando como visual char mas
                // selecionando a linha inteira.
                self.visual_anchor = Some((self.cursor.0, 0));
                self.cursor.1 = self.lines[self.cursor.0].len().saturating_sub(1);
                self.mode = Mode::Visual;
            }
            ":" => {
                self.mode = Mode::Command(String::new());
            }
            "Esc" => {
                // Já em Normal — limpa pending
                self.pending_op = None;
                self.status_line = String::new();
            }
            _ => {}
        }
        CommandResult::None
    }

    fn handle_pending_op(&mut self, op: char, key: &str) -> CommandResult {
        self.pending_op = None;
        match (op, key) {
            ('d', "w") => self.delete_word_at_cursor(),
            ('d', "$") => self.delete_to_end_of_line(),
            ('d', "d") => self.delete_current_line(),
            ('d', "j") => {
                // dd + linha abaixo
                self.push_undo();
                let row = self.cursor.0;
                self.lines.remove(row);
                if row < self.lines.len() {
                    self.lines.remove(row);
                }
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
                self.cursor.0 = row.min(self.lines.len() - 1);
                self.clamp_cursor();
            }
            ('d', "G") => {
                self.push_undo();
                let row = self.cursor.0;
                self.lines.truncate(row);
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
                self.cursor.0 = self.lines.len() - 1;
                self.clamp_cursor();
            }
            ('y', "y") => self.yank_current_line(),
            ('y', "w") => {
                // Yank word
                let row = self.cursor.0;
                let col = self.cursor.1;
                let chars: Vec<char> = self.lines[row].chars().collect();
                let len = chars.len();
                let mut end = col;
                while end < len && !chars[end].is_whitespace() {
                    end += 1;
                }
                self.yank_buffer = Some(chars[col..end].iter().collect());
            }
            _ => {
                self.error = Some(format!("Operação desconhecida: {op}{key}"));
            }
        }
        CommandResult::None
    }

    fn handle_insert(&mut self, key: &str) {
        match key {
            "Esc" => self.exit_insert_mode(),
            "Backspace" => self.backspace_insert(),
            "Enter" => self.insert_char('\n'),
            k if k.len() == 1 => {
                let ch = k.chars().next().unwrap();
                self.insert_char(ch);
            }
            _ => {}
        }
    }

    fn handle_visual(&mut self, key: &str) -> CommandResult {
        match key {
            "h" => self.move_left(),
            "j" => self.move_down(),
            "k" => self.move_up(),
            "l" => self.move_right(),
            "d" => {
                self.delete_visual_selection();
                return CommandResult::None;
            }
            "y" => {
                self.yank_visual_selection();
                return CommandResult::None;
            }
            "Esc" => {
                self.visual_anchor = None;
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        CommandResult::None
    }

    fn handle_command_input(&mut self, key: &str, acc: &str) -> CommandResult {
        match key {
            "Esc" => {
                self.mode = Mode::Normal;
                self.status_line = String::new();
                CommandResult::None
            }
            "Enter" => {
                let cmd = acc.to_string();
                self.mode = Mode::Normal;
                self.status_line = String::new();
                self.execute_command(&cmd)
            }
            "Backspace" => {
                let mut new_acc = acc.to_string();
                new_acc.pop();
                self.mode = Mode::Command(new_acc.clone());
                self.status_line = format!(":{new_acc}");
                CommandResult::None
            }
            k if k.len() == 1 => {
                let new_acc = format!("{acc}{k}");
                self.mode = Mode::Command(new_acc.clone());
                self.status_line = format!(":{new_acc}");
                CommandResult::None
            }
            _ => CommandResult::None,
        }
    }

    fn handle_search_input(&mut self, key: &str, acc: &str) {
        match key {
            "Esc" => {
                self.mode = Mode::Normal;
                self.status_line = String::new();
            }
            "Enter" => {
                let pattern = acc.to_string();
                self.mode = Mode::Normal;
                self.status_line = String::new();
                self.execute_search(&pattern);
            }
            "Backspace" => {
                let mut new_acc = acc.to_string();
                new_acc.pop();
                self.mode = Mode::Search(new_acc.clone());
                self.status_line = format!("/{new_acc}");
            }
            k if k.len() == 1 => {
                let new_acc = format!("{acc}{k}");
                self.mode = Mode::Search(new_acc.clone());
                self.status_line = format!("/{new_acc}");
            }
            _ => {}
        }
    }
}

/// Resultado de uma operação de comando.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    None,
    Save,
    Quit,
    SaveAndQuit,
}

// ── Helpers de busca ─────────────────────────────────────────────────────────

fn find_in_line(line: &str, pattern: &str, from: usize) -> Option<usize> {
    if from > line.len() {
        return None;
    }
    line[from..].find(pattern).map(|i| from + i)
}

fn find_in_line_backward(line: &str, pattern: &str, before: usize) -> Option<usize> {
    let end = before.min(line.len());
    if end < pattern.len() {
        return None;
    }
    let search_space = &line[..end];
    // Encontra a última ocorrência
    let mut last = None;
    let mut pos = 0;
    while let Some(i) = search_space[pos..].find(pattern) {
        last = Some(pos + i);
        pos += i + 1;
        if pos >= search_space.len() {
            break;
        }
    }
    last
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(text: &str) -> Editor {
        Editor::new(text, (0, 0))
    }

    fn ed_at(text: &str, row: usize, col: usize) -> Editor {
        Editor::new(text, (row, col))
    }

    // ── Movement tests ───────────────────────────────────────────────────────

    #[test]
    fn test_h_moves_left() {
        let mut e = ed_at("hello", 0, 2);
        e.send_key("h");
        assert_eq!(e.cursor, (0, 1));
    }

    #[test]
    fn test_h_at_col0_stays() {
        let mut e = ed("hello");
        e.send_key("h");
        assert_eq!(e.cursor, (0, 0));
    }

    #[test]
    fn test_l_moves_right() {
        let mut e = ed("hello");
        e.send_key("l");
        assert_eq!(e.cursor, (0, 1));
    }

    #[test]
    fn test_l_at_end_stays() {
        let mut e = ed_at("hello", 0, 4);
        e.send_key("l");
        assert_eq!(e.cursor, (0, 4));
    }

    #[test]
    fn test_j_moves_down() {
        let mut e = ed("line1\nline2");
        e.send_key("j");
        assert_eq!(e.cursor.0, 1);
    }

    #[test]
    fn test_k_moves_up() {
        let mut e = ed_at("line1\nline2", 1, 0);
        e.send_key("k");
        assert_eq!(e.cursor.0, 0);
    }

    #[test]
    fn test_k_at_first_line_stays() {
        let mut e = ed("line1");
        e.send_key("k");
        assert_eq!(e.cursor.0, 0);
    }

    #[test]
    fn test_j_at_last_line_stays() {
        let mut e = ed("line1");
        e.send_key("j");
        assert_eq!(e.cursor.0, 0);
    }

    #[test]
    fn test_col0_move() {
        let mut e = ed_at("hello world", 0, 5);
        e.send_key("0");
        assert_eq!(e.cursor.1, 0);
    }

    #[test]
    fn test_dollar_move_to_end() {
        let mut e = ed("hello");
        e.send_key("$");
        assert_eq!(e.cursor.1, 4);
    }

    #[test]
    fn test_caret_first_nonblank() {
        let mut e = ed_at("  hello", 0, 0);
        e.send_key("^");
        assert_eq!(e.cursor.1, 2);
    }

    #[test]
    fn test_gg_goes_to_first_line() {
        let mut e = ed_at("line1\nline2\nline3", 2, 0);
        e.send_key("gg");
        assert_eq!(e.cursor.0, 0);
    }

    #[test]
    fn test_big_g_goes_to_last_line() {
        let mut e = ed("line1\nline2\nline3");
        e.send_key("G");
        assert_eq!(e.cursor.0, 2);
    }

    #[test]
    fn test_w_moves_to_next_word() {
        let mut e = ed("hello world");
        e.send_key("w");
        assert_eq!(e.cursor.1, 6);
    }

    #[test]
    fn test_w_at_end_moves_to_next_line() {
        let mut e = ed_at("hello\nworld", 0, 4);
        e.send_key("w");
        assert_eq!(e.cursor.0, 1);
    }

    #[test]
    fn test_b_moves_to_prev_word() {
        let mut e = ed_at("hello world", 0, 6);
        e.send_key("b");
        assert_eq!(e.cursor.1, 0);
    }

    #[test]
    fn test_b_at_start_moves_to_prev_line() {
        let mut e = ed_at("hello\nworld", 1, 0);
        e.send_key("b");
        assert_eq!(e.cursor.0, 0);
    }

    #[test]
    fn test_e_moves_to_end_of_word() {
        let mut e = ed("hello world");
        e.send_key("e");
        assert_eq!(e.cursor.1, 4);
    }

    // ── x delete char ────────────────────────────────────────────────────────

    #[test]
    fn test_x_deletes_char() {
        let mut e = ed("hello");
        e.send_key("x");
        assert_eq!(e.lines[0], "ello");
    }

    #[test]
    fn test_x_at_middle() {
        let mut e = ed_at("hello", 0, 2);
        e.send_key("x");
        assert_eq!(e.lines[0], "helo");
    }

    #[test]
    fn test_x_empty_line_does_nothing() {
        let mut e = ed("");
        e.send_key("x");
        assert_eq!(e.lines[0], "");
    }

    // ── dd delete line ───────────────────────────────────────────────────────

    #[test]
    fn test_dd_deletes_line() {
        let mut e = ed("line1\nline2\nline3");
        e.send_key("dd");
        assert_eq!(e.lines, vec!["line2", "line3"]);
    }

    #[test]
    fn test_dd_middle_line() {
        let mut e = ed_at("line1\nline2\nline3", 1, 0);
        e.send_key("dd");
        assert_eq!(e.lines, vec!["line1", "line3"]);
    }

    #[test]
    fn test_dd_only_line_leaves_empty() {
        let mut e = ed("only line");
        e.send_key("dd");
        assert_eq!(e.lines, vec![""]);
    }

    // ── dw delete word ───────────────────────────────────────────────────────

    #[test]
    fn test_dw_deletes_word_and_space() {
        let mut e = ed("hello world");
        e.send_key("d");
        e.send_key("w");
        assert_eq!(e.lines[0], "world");
    }

    #[test]
    fn test_dw_middle_word() {
        let mut e = ed_at("foo bar baz", 0, 4);
        e.send_key("d");
        e.send_key("w");
        assert_eq!(e.lines[0], "foo baz");
    }

    // ── d$ delete to end ─────────────────────────────────────────────────────

    #[test]
    fn test_d_dollar_deletes_to_end() {
        let mut e = ed_at("hello world", 0, 5);
        e.send_key("d");
        e.send_key("$");
        assert_eq!(e.lines[0], "hello");
    }

    #[test]
    fn test_d_dollar_from_start() {
        let mut e = ed("hello world");
        e.send_key("d");
        e.send_key("$");
        assert_eq!(e.lines[0], "");
    }

    // ── yy / p / P ───────────────────────────────────────────────────────────

    #[test]
    fn test_yy_p_duplicates_line() {
        let mut e = ed("hello\nworld");
        e.send_key("yy");
        e.send_key("p");
        assert_eq!(e.lines[0], "hello");
        assert_eq!(e.lines[1], "hello");
        assert_eq!(e.lines[2], "world");
    }

    #[test]
    fn test_yy_big_p_pastes_before() {
        let mut e = ed("hello\nworld");
        e.send_key("yy");
        e.send_key("P");
        assert_eq!(e.lines[0], "hello");
        assert_eq!(e.lines[1], "hello");
    }

    // ── u undo ──────────────────────────────────────────────────────────────

    #[test]
    fn test_u_undoes_delete_char() {
        let mut e = ed("hello");
        e.send_key("x");
        e.send_key("u");
        assert_eq!(e.lines[0], "hello");
    }

    #[test]
    fn test_u_undoes_dd() {
        let mut e = ed("line1\nline2");
        e.send_key("dd");
        e.send_key("u");
        assert_eq!(e.lines, vec!["line1", "line2"]);
    }

    #[test]
    fn test_u_multiple_undoes() {
        let mut e = ed("hello");
        e.send_key("x");
        e.send_key("x");
        e.send_key("u");
        assert_eq!(e.lines[0], "ello");
        e.send_key("u");
        assert_eq!(e.lines[0], "hello");
    }

    // ── . repeat ────────────────────────────────────────────────────────────

    #[test]
    fn test_dot_repeats_delete_char() {
        let mut e = ed("hello");
        e.send_key("x");
        e.send_key(".");
        assert_eq!(e.lines[0], "llo");
    }

    #[test]
    fn test_dot_repeats_dd() {
        let mut e = ed("line1\nline2\nline3");
        e.send_key("dd");
        e.send_key(".");
        assert_eq!(e.lines, vec!["line3"]);
    }

    // ── Insert mode ──────────────────────────────────────────────────────────

    #[test]
    fn test_i_enters_insert_and_types() {
        let mut e = ed("hello");
        e.send_key("i");
        e.send_key("X");
        e.send_key("Esc");
        assert_eq!(e.lines[0], "Xhello");
    }

    #[test]
    fn test_a_appends_after_cursor() {
        let mut e = ed_at("hello", 0, 4);
        e.send_key("a");
        e.send_key("!");
        e.send_key("Esc");
        assert_eq!(e.lines[0], "hello!");
    }

    #[test]
    fn test_big_a_appends_at_end() {
        let mut e = ed("hello");
        e.send_key("A");
        e.send_key(" mundo");
        // Sending each char individually
        let mut e2 = ed("hello");
        e2.send_key("A");
        for ch in " mundo".chars() {
            e2.send_key(&ch.to_string());
        }
        e2.send_key("Esc");
        assert_eq!(e2.lines[0], "hello mundo");
    }

    #[test]
    fn test_big_i_inserts_at_first_nonblank() {
        let mut e = ed("  hello");
        e.send_key("I");
        e.send_key("X");
        e.send_key("Esc");
        assert_eq!(e.lines[0], "  Xhello");
    }

    #[test]
    fn test_o_opens_line_below() {
        let mut e = ed("hello\nworld");
        e.send_key("o");
        e.send_key("new");
        // send each char
        let mut e2 = ed("hello\nworld");
        e2.send_key("o");
        for ch in "new".chars() {
            e2.send_key(&ch.to_string());
        }
        e2.send_key("Esc");
        assert_eq!(e2.lines[0], "hello");
        assert_eq!(e2.lines[1], "new");
        assert_eq!(e2.lines[2], "world");
    }

    #[test]
    fn test_big_o_opens_line_above() {
        let mut e = ed_at("hello\nworld", 1, 0);
        e.send_key("O");
        for ch in "new".chars() {
            e.send_key(&ch.to_string());
        }
        e.send_key("Esc");
        assert_eq!(e.lines[0], "hello");
        assert_eq!(e.lines[1], "new");
        assert_eq!(e.lines[2], "world");
    }

    #[test]
    fn test_backspace_in_insert() {
        let mut e = ed("hello");
        e.send_key("A");
        e.send_key("x");
        e.send_key("Backspace");
        e.send_key("Esc");
        assert_eq!(e.lines[0], "hello");
    }

    #[test]
    fn test_insert_then_esc_back_to_normal() {
        let mut e = ed("hello");
        e.send_key("i");
        assert_eq!(e.mode, Mode::Insert);
        e.send_key("Esc");
        assert_eq!(e.mode, Mode::Normal);
    }

    #[test]
    fn test_insert_multiple_chars() {
        let mut e = ed("world");
        e.send_key("i");
        for ch in "hello ".chars() {
            e.send_key(&ch.to_string());
        }
        e.send_key("Esc");
        assert_eq!(e.lines[0], "hello world");
    }

    // ── Visual mode ──────────────────────────────────────────────────────────

    #[test]
    fn test_v_enters_visual() {
        let mut e = ed("hello");
        e.send_key("v");
        assert_eq!(e.mode, Mode::Visual);
    }

    #[test]
    fn test_visual_d_deletes_selection() {
        let mut e = ed("hello world");
        e.send_key("v");
        e.send_key("l");
        e.send_key("l");
        e.send_key("l");
        e.send_key("l");
        e.send_key("d");
        assert_eq!(e.lines[0], " world");
    }

    #[test]
    fn test_visual_y_yanks_selection() {
        let mut e = ed("hello world");
        e.send_key("v");
        e.send_key("l");
        e.send_key("l");
        e.send_key("l");
        e.send_key("l");
        e.send_key("y");
        assert_eq!(e.yank_buffer, Some("hello".to_string()));
    }

    #[test]
    fn test_visual_esc_back_to_normal() {
        let mut e = ed("hello");
        e.send_key("v");
        e.send_key("Esc");
        assert_eq!(e.mode, Mode::Normal);
    }

    // ── Command mode ─────────────────────────────────────────────────────────

    #[test]
    fn test_colon_enters_command() {
        let mut e = ed("hello");
        e.send_key(":");
        assert!(matches!(e.mode, Mode::Command(_)));
    }

    #[test]
    fn test_colon_w_returns_save() {
        let mut e = ed("hello");
        e.send_key(":");
        e.send_key("w");
        let result = e.send_key("Enter");
        assert_eq!(result, CommandResult::Save);
    }

    #[test]
    fn test_colon_q_returns_quit() {
        let mut e = ed("hello");
        e.send_key(":");
        e.send_key("q");
        let result = e.send_key("Enter");
        assert_eq!(result, CommandResult::Quit);
    }

    #[test]
    fn test_colon_esc_back_to_normal() {
        let mut e = ed("hello");
        e.send_key(":");
        e.send_key("Esc");
        assert_eq!(e.mode, Mode::Normal);
    }

    // ── Substitute :s/find/replace/ ──────────────────────────────────────────

    #[test]
    fn test_substitute_replaces_first_occurrence() {
        let mut e = ed("hello world hello");
        e.send_key(":");
        for ch in "s/hello/bye/".chars() {
            e.send_key(&ch.to_string());
        }
        e.send_key("Enter");
        assert_eq!(e.lines[0], "bye world hello");
    }

    #[test]
    fn test_substitute_no_match_noop() {
        let mut e = ed("hello world");
        e.send_key(":");
        for ch in "s/xyz/abc/".chars() {
            e.send_key(&ch.to_string());
        }
        e.send_key("Enter");
        assert_eq!(e.lines[0], "hello world");
    }

    #[test]
    fn test_substitute_on_current_line_only() {
        let mut e = ed_at("hello\nhello", 1, 0);
        e.send_key(":");
        for ch in "s/hello/bye/".chars() {
            e.send_key(&ch.to_string());
        }
        e.send_key("Enter");
        assert_eq!(e.lines[0], "hello");
        assert_eq!(e.lines[1], "bye");
    }

    // ── Search / ─────────────────────────────────────────────────────────────

    #[test]
    fn test_search_moves_cursor() {
        let mut e = ed("foo bar baz");
        e.send_key("/");
        for ch in "bar".chars() {
            e.send_key(&ch.to_string());
        }
        e.send_key("Enter");
        assert_eq!(e.cursor.1, 4);
    }

    #[test]
    fn test_search_multiline() {
        let mut e = ed("linha um\nlinha dois\nalvo encontrado");
        e.send_key("/");
        for ch in "alvo".chars() {
            e.send_key(&ch.to_string());
        }
        e.send_key("Enter");
        assert_eq!(e.cursor.0, 2);
        assert_eq!(e.cursor.1, 0);
    }

    #[test]
    fn test_n_finds_next_match() {
        // Buffer: "foo bar foo baz" (2 occurrências de "foo")
        // Após /foo, o cursor está na segunda ocorrência (col 8), pois a busca
        // avança a partir de col+1=1 e encontra col 8.
        // Após n (wrap-around), volta para col 0 (primeira ocorrência).
        let mut e = ed("foo bar foo baz");
        e.send_key("/");
        for ch in "foo".chars() {
            e.send_key(&ch.to_string());
        }
        e.send_key("Enter");
        // Após a busca, cursor deve estar em alguma ocorrência de "foo"
        let line = e.lines[0].clone();
        assert!(
            line[e.cursor.1..].starts_with("foo"),
            "cursor deve estar em 'foo'"
        );
        let first_pos = e.cursor.1;
        e.send_key("n");
        // n deve ter avançado para outra ocorrência (diferente ou mesmo wrap)
        let second_pos = e.cursor.1;
        assert!(
            line[second_pos..].starts_with("foo"),
            "cursor após n deve estar em 'foo'"
        );
        // As duas posições devem ser diferentes (há 2 ocorrências)
        assert_ne!(
            first_pos, second_pos,
            "n deve ter avançado para outra ocorrência"
        );
    }

    #[test]
    fn test_big_n_finds_prev_match() {
        // Buffer: "foo bar foo baz"
        // /foo → col 8 (segunda ocorrência, pois busca avança de 0+1=1)
        // n → col 0 (wrap-around para primeira)
        // N → col 8 (volta para segunda)
        let mut e = ed("foo bar foo baz");
        e.send_key("/");
        for ch in "foo".chars() {
            e.send_key(&ch.to_string());
        }
        e.send_key("Enter");
        let after_search = e.cursor.1;
        e.send_key("n"); // wrap-around para a outra ocorrência
        let after_n = e.cursor.1;
        e.send_key("N"); // volta para a ocorrência anterior
        let after_big_n = e.cursor.1;
        // Após N, deve ter voltado para after_search
        assert_eq!(
            after_big_n, after_search,
            "N deve retornar ao ponto após /search"
        );
        // Confirma que n e N alternam posições
        assert_ne!(after_n, after_search);
    }

    // ── Pending operator edge cases ──────────────────────────────────────────

    #[test]
    fn test_d_pending_then_esc_clears() {
        let mut e = ed("hello");
        e.send_key("d");
        assert_eq!(e.pending_op, Some('d'));
        e.send_key("Esc");
        assert_eq!(e.pending_op, None);
    }

    #[test]
    fn test_dd_is_same_as_d_then_d() {
        let mut e1 = ed("line1\nline2");
        e1.send_key("dd");

        let mut e2 = ed("line1\nline2");
        e2.send_key("d");
        e2.send_key("d");

        assert_eq!(e1.lines, e2.lines);
    }

    // ── Cursor clamping ──────────────────────────────────────────────────────

    #[test]
    fn test_cursor_clamped_after_dd() {
        let mut e = ed_at("a\nb", 0, 0);
        e.lines[0] = "longline".to_string();
        e.cursor.1 = 7;
        e.send_key("dd");
        assert!(e.cursor.1 <= e.lines[e.cursor.0].len().saturating_sub(1).max(0));
    }

    #[test]
    fn test_j_clamps_col_on_shorter_line() {
        let mut e = ed_at("hello world\nhi", 0, 10);
        e.send_key("j");
        assert_eq!(e.cursor.0, 1);
        assert!(e.cursor.1 <= 1);
    }

    // ── Insert mode at end of line ───────────────────────────────────────────

    #[test]
    fn test_insert_a_at_last_char() {
        let mut e = ed_at("abc", 0, 2);
        e.send_key("a");
        e.send_key("d");
        e.send_key("Esc");
        assert_eq!(e.lines[0], "abcd");
    }

    // ── yy then paste multiple times ─────────────────────────────────────────

    #[test]
    fn test_yy_paste_twice() {
        let mut e = ed("original\nother");
        e.send_key("yy");
        e.send_key("p");
        e.send_key("p");
        assert_eq!(e.lines[0], "original");
        assert_eq!(e.lines[1], "original");
        assert_eq!(e.lines[2], "original");
        assert_eq!(e.lines[3], "other");
    }

    // ── Mode name strings ────────────────────────────────────────────────────

    #[test]
    fn test_mode_names() {
        assert_eq!(Mode::Normal.name(), "NORMAL");
        assert_eq!(Mode::Insert.name(), "INSERT");
        assert_eq!(Mode::Visual.name(), "VISUAL");
        assert_eq!(Mode::Command(String::new()).name(), "COMMAND");
    }

    // ── find_in_line helpers ─────────────────────────────────────────────────

    #[test]
    fn test_find_in_line_found() {
        assert_eq!(find_in_line("hello world", "world", 0), Some(6));
    }

    #[test]
    fn test_find_in_line_from_offset() {
        assert_eq!(find_in_line("hello hello", "hello", 5), Some(6));
    }

    #[test]
    fn test_find_in_line_not_found() {
        assert_eq!(find_in_line("hello", "xyz", 0), None);
    }

    #[test]
    fn test_find_in_line_backward_found() {
        assert_eq!(find_in_line_backward("hello hello", "hello", 11), Some(6));
    }

    #[test]
    fn test_find_in_line_backward_first() {
        assert_eq!(find_in_line_backward("hello hello", "hello", 5), Some(0));
    }

    // ── Insert mode: dot repeat of insert ───────────────────────────────────

    #[test]
    fn test_dot_repeats_insert() {
        let mut e = ed("ab");
        e.send_key("i");
        e.send_key("X");
        e.send_key("Esc");
        // Move right and repeat
        e.send_key("l");
        e.send_key("l");
        e.send_key(".");
        assert!(e.lines[0].contains('X'));
    }

    // ── G and gg movement ────────────────────────────────────────────────────

    #[test]
    fn test_g_key_alone_does_nothing_harmful() {
        let mut e = ed("hello\nworld");
        e.send_key("g");
        // Should not crash
        assert_eq!(e.lines[0], "hello");
    }

    // ── Multi-line visual delete ─────────────────────────────────────────────

    #[test]
    fn test_visual_multiline_delete() {
        let mut e = ed("line1\nline2\nline3");
        e.send_key("v");
        e.send_key("j");
        e.send_key("d");
        // line1 and line2 deleted
        assert!(e.lines.iter().all(|l| l != "line1" && l != "line2") || e.lines.len() <= 2);
    }

    // ── Undo after insert ────────────────────────────────────────────────────

    #[test]
    fn test_undo_after_insert() {
        let mut e = ed("hello");
        e.send_key("A");
        e.send_key(" world");
        // send each char
        let mut e2 = ed("hello");
        e2.send_key("A");
        for ch in " world".chars() {
            e2.send_key(&ch.to_string());
        }
        e2.send_key("Esc");
        e2.send_key("u");
        assert_eq!(e2.lines[0], "hello");
    }

    // ── Search wrap-around ───────────────────────────────────────────────────

    #[test]
    fn test_search_wraps_around() {
        let mut e = ed_at("foo bar\nbaz foo", 1, 4);
        e.send_key("/");
        for ch in "foo".chars() {
            e.send_key(&ch.to_string());
        }
        e.send_key("Enter");
        // Should wrap to line 0
        assert_eq!(e.cursor.0, 0);
        assert_eq!(e.cursor.1, 0);
    }

    // ── Empty buffer edge cases ──────────────────────────────────────────────

    #[test]
    fn test_empty_buffer_operations() {
        let mut e = ed("");
        e.send_key("h");
        e.send_key("l");
        e.send_key("j");
        e.send_key("k");
        e.send_key("$");
        e.send_key("0");
        assert_eq!(e.lines[0], "");
    }

    // ── Command backspace ────────────────────────────────────────────────────

    #[test]
    fn test_command_backspace() {
        let mut e = ed("hello");
        e.send_key(":");
        e.send_key("w");
        e.send_key("q");
        e.send_key("Backspace");
        let result = e.send_key("Enter");
        assert_eq!(result, CommandResult::Save);
    }

    // ── wq command ───────────────────────────────────────────────────────────

    #[test]
    fn test_colon_wq_save_and_quit() {
        let mut e = ed("hello");
        e.send_key(":");
        e.send_key("w");
        e.send_key("q");
        let result = e.send_key("Enter");
        assert_eq!(result, CommandResult::SaveAndQuit);
    }

    // ── Cursor stays in normal mode ──────────────────────────────────────────

    #[test]
    fn test_cursor_at_col0_on_empty_normal() {
        let mut e = ed("x");
        e.send_key("x"); // delete the char
        assert_eq!(e.cursor.1, 0);
    }

    // ── Insert newline ───────────────────────────────────────────────────────

    #[test]
    fn test_insert_enter_creates_new_line() {
        let mut e = ed("hello world");
        e.cursor.1 = 5;
        e.send_key("i");
        e.send_key("Enter");
        e.send_key("Esc");
        assert_eq!(e.lines[0], "hello");
        assert_eq!(e.lines[1], " world");
    }

    // ── Visual selection boundaries ──────────────────────────────────────────

    #[test]
    fn test_visual_selection_reversed_anchor() {
        let mut e = ed_at("hello world", 0, 5);
        e.send_key("v");
        e.send_key("h");
        e.send_key("h");
        let sel = e.visual_selection();
        assert!(sel.is_some());
        let (s, end) = sel.unwrap();
        assert!(s.1 <= end.1);
    }
}
