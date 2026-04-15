use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crossterm::{
    cursor::MoveTo,
    style::Print,
    terminal::ClearType,
    QueueableCommand,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Write content to a file atomically using a temp file + rename.
/// Prevents file corruption if the process is interrupted during a write.
fn write_atomic(path: &str, content: &[u8]) -> io::Result<()> {
    let path = Path::new(path);
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp_name = format!(
        ".tmp.{}.{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    );
    let tmp_path = dir.join(tmp_name);
    let mut file = fs::File::create(&tmp_path)?;
    file.write_all(content)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp_path, path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        e
    })
}

/// Truncate a string to fit within `max_cols` terminal display columns.
fn truncate_to_width(s: &str, max_cols: usize) -> String {
    let mut width = 0;
    s.graphemes(true)
        .take_while(|g| {
            let w = UnicodeWidthStr::width(*g);
            if width + w <= max_cols {
                width += w;
                true
            } else {
                false
            }
        })
        .collect()
}

#[derive(Clone, PartialEq)]
pub enum PromptMode {
    None,
    SaveAs(String),
    Find(String),
    ConfirmExit,
}

pub struct RenderState {
    pub lines: Vec<String>,
    pub row_offset: usize,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub prompt_mode: PromptMode,
    pub message: Option<String>,
}

pub struct Editor {
    buffer: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,  // Grapheme index (not byte position)
    row_offset: usize,  // for scrolling
    filename: Option<String>,
    custom_filename: bool,  // true if user explicitly set filename via SaveAs
    dirty: bool,
    cut_buffer: Option<String>,
    pub message: Option<String>,  // for save feedback
    pub prompt_mode: PromptMode,
    pub pending_exit: bool,  // exit after the next successful save
}

/// Helper functions for Unicode-aware string operations
impl Editor {
    /// Get the number of graphemes in a string
    fn grapheme_len(s: &str) -> usize {
        s.graphemes(true).count()
    }

    /// Convert a grapheme index to a byte index
    /// Returns None if the index is out of bounds
    fn grapheme_to_byte_index(s: &str, grapheme_idx: usize) -> Option<usize> {
        s.grapheme_indices(true)
            .nth(grapheme_idx)
            .map(|(i, _)| i)
            .or_else(|| {
                // If index equals grapheme count, return string length (for appending)
                if grapheme_idx == Self::grapheme_len(s) {
                    Some(s.len())
                } else {
                    None
                }
            })
    }

}

impl Editor {
    pub fn new() -> Self {
        Editor {
            buffer: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            row_offset: 0,
            filename: None,
            custom_filename: false,
            dirty: false,
            cut_buffer: None,
            message: None,
            prompt_mode: PromptMode::None,
            pending_exit: false,
        }
    }

    pub fn load_file(&mut self, path: &str) -> Result<(), io::Error> {
        let content = fs::read_to_string(path)?;
        self.buffer = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(|s| s.to_string()).collect()
        };
        self.filename = Some(path.to_string());
        self.dirty = false;
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), io::Error> {
        if let Some(ref path) = self.filename {
            let mut content = self.buffer.join("\n");
            content.push('\n');
            write_atomic(path, content.as_bytes())?;
            self.dirty = false;
            self.message = Some(format!("Saved: {}", path));
        } else {
            self.message = Some("No filename — use Ctrl+S to set one".to_string());
        }
        Ok(())
    }

    pub fn save_as(&mut self, path: &str) -> Result<(), io::Error> {
        let mut content = self.buffer.join("\n");
        content.push('\n');
        write_atomic(path, content.as_bytes())?;
        self.filename = Some(path.to_string());
        self.custom_filename = true;
        self.dirty = false;
        self.message = Some(format!("Saved: {}", path));
        Ok(())
    }

    pub fn has_custom_filename(&self) -> bool {
        self.custom_filename
    }

    pub fn get_filename(&self) -> Option<&str> {
        self.filename.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_filename(&mut self, path: &str) {
        self.filename = Some(path.to_string());
        self.custom_filename = true;
    }

    /// 1-indexed. Returns None for index 0 or past end.
    pub fn get_line(&self, n: usize) -> Option<&str> {
        if n == 0 { return None; }
        self.buffer.get(n - 1).map(String::as_str)
    }

    pub fn line_count(&self) -> usize {
        self.buffer.len()
    }

    /// 1-indexed. No-op if n is 0 or past end.
    pub fn set_line(&mut self, n: usize, text: String) {
        if n == 0 { return; }
        if let Some(line) = self.buffer.get_mut(n - 1) {
            let text = text.replace('\n', "").replace('\r', "");
            *line = text;
            self.dirty = true;
        }
    }

    /// Inserts `text` before 1-indexed line n. No-op if n == 0. Appends if n > len+1.
    pub fn insert_line(&mut self, n: usize, text: String) {
        if n == 0 { return; }
        let idx = (n - 1).min(self.buffer.len());
        self.buffer.insert(idx, text);
        if idx <= self.cursor_row {
            self.cursor_row += 1;
        }
        self.dirty = true;
    }

    /// 1-indexed. No-op if n is 0 or past end. Preserves at least one line.
    pub fn delete_line(&mut self, n: usize) {
        if n == 0 || self.buffer.len() <= 1 { return; }
        if let Some(idx) = n.checked_sub(1) {
            if idx < self.buffer.len() {
                self.buffer.remove(idx);
                if self.cursor_row >= self.buffer.len() {
                    self.cursor_row = self.buffer.len() - 1;
                } else if idx < self.cursor_row {
                    self.cursor_row -= 1;
                }
                self.clamp_cursor_col();
                self.dirty = true;
            }
        }
    }

    pub fn start_confirm_exit_prompt(&mut self) {
        self.prompt_mode = PromptMode::ConfirmExit;
    }

    pub fn start_save_as_and_exit_prompt(&mut self) {
        self.pending_exit = true;
        self.start_save_as_prompt();
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    pub fn start_save_as_prompt(&mut self) {
        let default = self.get_filename().unwrap_or("").to_string();
        self.prompt_mode = PromptMode::SaveAs(default);
    }

    pub fn start_find_prompt(&mut self) {
        self.prompt_mode = PromptMode::Find(String::new());
    }

    pub fn prompt_insert_char(&mut self, c: char) {
        match &mut self.prompt_mode {
            PromptMode::SaveAs(ref mut input) => input.push(c),
            PromptMode::Find(ref mut input) => input.push(c),
            PromptMode::None | PromptMode::ConfirmExit => {}
        }
    }

    pub fn prompt_backspace(&mut self) {
        match &mut self.prompt_mode {
            PromptMode::SaveAs(ref mut input) => { input.pop(); }
            PromptMode::Find(ref mut input) => { input.pop(); }
            PromptMode::None | PromptMode::ConfirmExit => {}
        }
    }

    pub fn prompt_cancel(&mut self) {
        self.prompt_mode = PromptMode::None;
        self.pending_exit = false;
    }

    /// Returns true if prompt was completed (Enter pressed)
    pub fn prompt_submit(&mut self) -> bool {
        match self.prompt_mode.clone() {
            PromptMode::SaveAs(input) => {
                self.prompt_mode = PromptMode::None;
                let path = if input.is_empty() {
                    self.get_filename().unwrap_or("").to_string()
                } else {
                    input
                };
                if !path.is_empty() {
                    if let Err(e) = self.save_as(&path) {
                        self.message = Some(format!("Error saving: {}", e));
                        self.pending_exit = false;
                    }
                } else {
                    self.pending_exit = false;
                }
                true
            }
            PromptMode::Find(query) => {
                self.prompt_mode = PromptMode::None;
                if !query.is_empty() {
                    self.find(&query);
                }
                true
            }
            PromptMode::None | PromptMode::ConfirmExit => false,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if let Some(line) = self.buffer.get_mut(self.cursor_row) {
            let grapheme_count = Self::grapheme_len(line);
            if self.cursor_col <= grapheme_count {
                // Convert grapheme index to byte index for insertion
                if let Some(byte_idx) = Self::grapheme_to_byte_index(line, self.cursor_col) {
                    line.insert(byte_idx, c);
                    self.cursor_col += 1;  // Move by 1 grapheme
                    self.dirty = true;
                }
            }
        }
    }

    pub fn insert_newline(&mut self) {
        let current_line = self.buffer.get(self.cursor_row).cloned().unwrap_or_default();

        // Use Unicode-aware splitting
        let (before, after) = if let Some(byte_idx) = Self::grapheme_to_byte_index(&current_line, self.cursor_col) {
            let before = current_line[..byte_idx].to_string();
            let after = current_line[byte_idx..].to_string();
            (before, after)
        } else {
            (current_line, String::new())
        };

        self.buffer[self.cursor_row] = before;
        self.buffer.insert(self.cursor_row + 1, after);
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.dirty = true;
    }

    pub fn delete_char(&mut self) {
        if let Some(line) = self.buffer.get_mut(self.cursor_row) {
            let grapheme_count = Self::grapheme_len(line);
            if self.cursor_col < grapheme_count {
                // Find the byte range of the grapheme to delete
                let graphemes: Vec<_> = line.grapheme_indices(true).collect();
                if let Some((start, g)) = graphemes.get(self.cursor_col) {
                    let end = start + g.len();
                    line.replace_range(*start..end, "");
                    self.dirty = true;
                }
            }
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
            if let Some(line) = self.buffer.get_mut(self.cursor_row) {
                // Find and remove the grapheme at cursor_col (after decrement)
                let graphemes: Vec<_> = line.grapheme_indices(true).collect();
                if let Some((start, g)) = graphemes.get(self.cursor_col) {
                    let end = start + g.len();
                    line.replace_range(*start..end, "");
                    self.dirty = true;
                }
            }
        } else if self.cursor_row > 0 {
            // Merge with previous line
            let current_line = self.buffer.remove(self.cursor_row);
            self.cursor_row -= 1;
            let prev_line_len = Self::grapheme_len(&self.buffer[self.cursor_row]);
            self.buffer[self.cursor_row].push_str(&current_line);
            self.cursor_col = prev_line_len;
            self.dirty = true;
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_cursor_col();
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor_row < self.buffer.len() - 1 {
            self.cursor_row += 1;
            self.clamp_cursor_col();
        }
    }

    pub fn cursor_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.buffer.get(self.cursor_row)
                .map(|l| Self::grapheme_len(l))
                .unwrap_or(0);
        }
    }

    pub fn cursor_right(&mut self) {
        let line_len = self.buffer.get(self.cursor_row)
            .map(|l| Self::grapheme_len(l))
            .unwrap_or(0);
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_row < self.buffer.len() - 1 {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    pub fn page_up(&mut self, rows: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(rows);
        self.clamp_cursor_col();
    }

    pub fn page_down(&mut self, rows: usize) {
        self.cursor_row = (self.cursor_row + rows).min(self.buffer.len() - 1);
        self.clamp_cursor_col();
    }

    pub fn cursor_home(&mut self) {
        self.cursor_col = 0;
    }

    pub fn cursor_end(&mut self) {
        self.cursor_col = self.buffer.get(self.cursor_row)
            .map(|l| Self::grapheme_len(l))
            .unwrap_or(0);
    }

    fn clamp_cursor_col(&mut self) {
        let line_len = self.buffer.get(self.cursor_row)
            .map(|l| Self::grapheme_len(l))
            .unwrap_or(0);
        self.cursor_col = self.cursor_col.min(line_len);
    }

    pub fn cut_line(&mut self) {
        if self.cursor_row < self.buffer.len() {
            self.cut_buffer = Some(self.buffer[self.cursor_row].clone());
            if self.buffer.len() > 1 {
                self.buffer.remove(self.cursor_row);
                if self.cursor_row >= self.buffer.len() {
                    self.cursor_row = self.buffer.len() - 1;
                }
            } else {
                self.buffer[0] = String::new();
            }
            self.cursor_col = 0;
            self.clamp_cursor_col();
            self.dirty = true;
        }
    }

    pub fn paste(&mut self) {
        if let Some(ref text) = self.cut_buffer {
            self.buffer.insert(self.cursor_row, text.clone());
            self.cursor_row += 1;
            self.cursor_col = 0;
            self.dirty = true;
        }
    }

    /// Convert a byte position to a grapheme index
    fn byte_to_grapheme_index(s: &str, byte_pos: usize) -> usize {
        s.grapheme_indices(true)
            .take_while(|(idx, _)| *idx < byte_pos)
            .count()
    }

    pub fn find(&mut self, query: &str) {
        // Search from current position to end
        for (row_idx, line) in self.buffer.iter().enumerate().skip(self.cursor_row) {
            let start_byte = if row_idx == self.cursor_row {
                Self::grapheme_to_byte_index(line, self.cursor_col).unwrap_or(0)
            } else {
                0
            };

            if let Some(byte_pos) = line[start_byte..].find(query) {
                let absolute_byte_pos = start_byte + byte_pos;
                self.cursor_row = row_idx;
                self.cursor_col = Self::byte_to_grapheme_index(line, absolute_byte_pos);
                return;
            }
        }
        // Wrap around to beginning
        for (row_idx, line) in self.buffer.iter().enumerate().take(self.cursor_row + 1) {
            if let Some(byte_pos) = line.find(query) {
                self.cursor_row = row_idx;
                self.cursor_col = Self::byte_to_grapheme_index(line, byte_pos);
                self.message = Some(format!("Search wrapped: {}", query));
                return;
            }
        }
        self.message = Some(format!("Not found: {}", query));
    }

    /// Clamps scroll, then snapshots render state.
    pub fn render_state(&mut self, visible_rows: usize) -> RenderState {
        // Scroll clamping (mirrors what render() does)
        if self.cursor_row < self.row_offset {
            self.row_offset = self.cursor_row;
        } else if visible_rows > 0 && self.cursor_row >= self.row_offset + visible_rows {
            self.row_offset = self.cursor_row - visible_rows + 1;
        }
        RenderState {
            lines: self.buffer.clone(),
            row_offset: self.row_offset,
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            prompt_mode: self.prompt_mode.clone(),
            message: self.message.clone(),
        }
    }

    #[cfg(test)]
    pub fn render<W: Write>(&mut self, stdout: &mut W, cols: u16, rows: u16) -> io::Result<()> {
        // Calculate visible rows (reserve bottom line for messages/prompts)
        let visible_rows = rows.saturating_sub(1) as usize;

        // Adjust row_offset for scrolling
        if self.cursor_row < self.row_offset {
            self.row_offset = self.cursor_row;
        } else if self.cursor_row >= self.row_offset + visible_rows {
            self.row_offset = self.cursor_row - visible_rows + 1;
        }

        // Clear screen
        stdout.queue(crossterm::terminal::Clear(ClearType::All))?;

        // Render visible lines
        for (i, line_idx) in (self.row_offset..self.buffer.len()).enumerate() {
            if i >= visible_rows {
                break;
            }
            stdout.queue(MoveTo(0, i as u16))?;

            // Truncate by display width, not char count, to handle wide characters
            let line = &self.buffer[line_idx];
            let max_cols = cols as usize;
            let mut display_width = 0;
            let truncated: String = line.graphemes(true)
                .take_while(|g| {
                    let w = UnicodeWidthStr::width(*g);
                    if display_width + w <= max_cols {
                        display_width += w;
                        true
                    } else {
                        false
                    }
                })
                .collect();
            stdout.queue(Print(truncated))?;
        }

        // Show prompt or message on bottom line
        stdout.queue(MoveTo(0, rows - 1))?;
        match &self.prompt_mode {
            PromptMode::SaveAs(input) => {
                let prompt = format!("Save as: {}", input);
                let truncated = truncate_to_width(&prompt, cols as usize);
                stdout.queue(Print(truncated))?;
            }
            PromptMode::Find(input) => {
                let prompt = format!("Find: {}", input);
                let truncated = truncate_to_width(&prompt, cols as usize);
                stdout.queue(Print(truncated))?;
            }
            PromptMode::ConfirmExit => {
                stdout.queue(Print("Save changes? (Y/n): "))?;
            }
            PromptMode::None => {
                if let Some(ref msg) = self.message {
                    let truncated = truncate_to_width(msg, cols as usize);
                    stdout.queue(Print(truncated))?;
                }
            }
        }

        // Position cursor - in prompt mode, put cursor at end of prompt line
        match &self.prompt_mode {
            PromptMode::SaveAs(input) => {
                let prompt = format!("Save as: {}", input);
                let prompt_display_width = UnicodeWidthStr::width(prompt.as_str()) as u16;
                stdout.queue(MoveTo(prompt_display_width.min(cols.saturating_sub(1)), rows - 1))?;
            }
            PromptMode::Find(input) => {
                let prompt = format!("Find: {}", input);
                let prompt_display_width = UnicodeWidthStr::width(prompt.as_str()) as u16;
                stdout.queue(MoveTo(prompt_display_width.min(cols.saturating_sub(1)), rows - 1))?;
            }
            PromptMode::ConfirmExit => {
                stdout.queue(MoveTo(20, rows - 1))?;
            }
            PromptMode::None => {
                let cursor_screen_row = (self.cursor_row - self.row_offset) as u16;
                // Compute display column as sum of widths of graphemes left of cursor
                let line = self.buffer.get(self.cursor_row).map(String::as_str).unwrap_or("");
                let cursor_display_col: usize = line.graphemes(true)
                    .take(self.cursor_col)
                    .map(UnicodeWidthStr::width)
                    .sum();
                stdout.queue(MoveTo(cursor_display_col as u16, cursor_screen_row))?;
            }
        }

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grapheme_len_ascii() {
        assert_eq!(Editor::grapheme_len("hello"), 5);
        assert_eq!(Editor::grapheme_len(""), 0);
    }

    #[test]
    fn test_grapheme_len_unicode() {
        // Each emoji is one grapheme cluster
        assert_eq!(Editor::grapheme_len("👨‍👩‍👧‍👦"), 1); // Family emoji (ZWJ sequence)
        assert_eq!(Editor::grapheme_len("🇺🇸"), 1); // Flag emoji (regional indicators)
        assert_eq!(Editor::grapheme_len("café"), 4); // e with combining acute accent is 1 grapheme
        assert_eq!(Editor::grapheme_len("naïve"), 5);
    }

    #[test]
    fn test_grapheme_to_byte_index_ascii() {
        assert_eq!(Editor::grapheme_to_byte_index("hello", 0), Some(0));
        assert_eq!(Editor::grapheme_to_byte_index("hello", 3), Some(3));
        assert_eq!(Editor::grapheme_to_byte_index("hello", 5), Some(5)); // End of string
        assert_eq!(Editor::grapheme_to_byte_index("hello", 6), None); // Out of bounds
    }

    #[test]
    fn test_grapheme_to_byte_index_unicode() {
        // "héllo" - é is 2 bytes
        let s = "héllo";
        assert_eq!(Editor::grapheme_to_byte_index(s, 0), Some(0)); // h
        assert_eq!(Editor::grapheme_to_byte_index(s, 1), Some(1)); // é starts at byte 1
        assert_eq!(Editor::grapheme_to_byte_index(s, 2), Some(3)); // l starts at byte 3
        assert_eq!(Editor::grapheme_to_byte_index(s, 5), Some(6)); // End of string
    }

    #[test]
    fn test_byte_to_grapheme_index() {
        let s = "héllo"; // h=byte 0, é=bytes 1-2, l=byte 3, l=byte 4, o=byte 5
        // byte_to_grapheme_index counts graphemes that START BEFORE the byte position
        assert_eq!(Editor::byte_to_grapheme_index(s, 0), 0); // Nothing before byte 0
        assert_eq!(Editor::byte_to_grapheme_index(s, 1), 1); // 'h' starts before byte 1
        assert_eq!(Editor::byte_to_grapheme_index(s, 2), 2); // 'h' and 'é' start before byte 2
        assert_eq!(Editor::byte_to_grapheme_index(s, 3), 2); // 'h' and 'é' start before byte 3
        assert_eq!(Editor::byte_to_grapheme_index(s, 4), 3); // 'h', 'é', 'l' start before byte 4
    }

    #[test]
    fn test_insert_char_ascii() {
        let mut editor = Editor::new();
        editor.insert_char('a');
        editor.insert_char('b');
        editor.insert_char('c');
        assert_eq!(editor.buffer[0], "abc");
        assert_eq!(editor.cursor_col, 3);
    }

    #[test]
    fn test_insert_char_unicode() {
        let mut editor = Editor::new();
        editor.insert_char('h');
        editor.insert_char('é');
        editor.insert_char('l');
        editor.insert_char('l');
        editor.insert_char('o');
        assert_eq!(editor.buffer[0], "héllo");
        assert_eq!(editor.cursor_col, 5); // 5 graphemes
    }

    #[test]
    fn test_insert_char_emoji() {
        let mut editor = Editor::new();
        editor.insert_char('a');
        editor.insert_char('🎉');
        editor.insert_char('b');
        assert_eq!(editor.buffer[0], "a🎉b");
        assert_eq!(editor.cursor_col, 3); // 3 graphemes
    }

    #[test]
    fn test_cursor_movement_unicode() {
        let mut editor = Editor::new();
        editor.buffer[0] = "a🎉b".to_string();
        editor.cursor_col = 0;

        editor.cursor_right();
        assert_eq!(editor.cursor_col, 1); // After 'a'

        editor.cursor_right();
        assert_eq!(editor.cursor_col, 2); // After emoji

        editor.cursor_right();
        assert_eq!(editor.cursor_col, 3); // After 'b'

        editor.cursor_left();
        assert_eq!(editor.cursor_col, 2); // Back before 'b'

        editor.cursor_left();
        assert_eq!(editor.cursor_col, 1); // Back before emoji
    }

    #[test]
    fn test_backspace_unicode() {
        let mut editor = Editor::new();
        editor.buffer[0] = "a🎉b".to_string();
        editor.cursor_col = 3;

        editor.backspace(); // Delete 'b'
        assert_eq!(editor.buffer[0], "a🎉");
        assert_eq!(editor.cursor_col, 2);

        editor.backspace(); // Delete emoji
        assert_eq!(editor.buffer[0], "a");
        assert_eq!(editor.cursor_col, 1);
    }

    #[test]
    fn test_delete_char_unicode() {
        let mut editor = Editor::new();
        editor.buffer[0] = "a🎉b".to_string();
        editor.cursor_col = 0;

        editor.delete_char(); // Delete 'a'
        assert_eq!(editor.buffer[0], "🎉b");
        assert_eq!(editor.cursor_col, 0);

        editor.delete_char(); // Delete emoji
        assert_eq!(editor.buffer[0], "b");
        assert_eq!(editor.cursor_col, 0);
    }

    #[test]
    fn test_insert_newline_unicode() {
        let mut editor = Editor::new();
        editor.buffer[0] = "a🎉b".to_string();
        editor.cursor_col = 2; // After emoji

        editor.insert_newline();
        assert_eq!(editor.buffer.len(), 2);
        assert_eq!(editor.buffer[0], "a🎉");
        assert_eq!(editor.buffer[1], "b");
        assert_eq!(editor.cursor_row, 1);
        assert_eq!(editor.cursor_col, 0);
    }

    #[test]
    fn test_find_unicode() {
        let mut editor = Editor::new();
        editor.buffer = vec!["hello".to_string(), "wörld".to_string(), "test".to_string()];
        editor.cursor_row = 0;
        editor.cursor_col = 0;

        editor.find("ö");
        assert_eq!(editor.cursor_row, 1);
        assert_eq!(editor.cursor_col, 1); // Grapheme position of ö
    }

    #[test]
    fn test_cut_paste() {
        let mut editor = Editor::new();
        editor.buffer = vec!["line1".to_string(), "line2".to_string(), "line3".to_string()];
        editor.cursor_row = 1;

        editor.cut_line();
        assert_eq!(editor.buffer.len(), 2);
        assert_eq!(editor.cut_buffer, Some("line2".to_string()));

        editor.paste();
        assert_eq!(editor.buffer.len(), 3);
        assert_eq!(editor.buffer[1], "line2");
    }

    #[test]
    fn test_cursor_end_unicode() {
        let mut editor = Editor::new();
        editor.buffer[0] = "a🎉b".to_string();
        editor.cursor_col = 0;

        editor.cursor_end();
        assert_eq!(editor.cursor_col, 3); // 3 graphemes
    }

    #[test]
    fn test_clamp_cursor_col() {
        let mut editor = Editor::new();
        editor.buffer[0] = "ab".to_string();
        editor.cursor_col = 10; // Out of bounds

        editor.clamp_cursor_col();
        assert_eq!(editor.cursor_col, 2);
    }

    #[test]
    fn test_get_line_one_indexed() {
        let mut e = Editor::new();
        e.buffer = vec!["alpha".to_string(), "beta".to_string()];
        assert_eq!(e.get_line(1), Some("alpha"));
        assert_eq!(e.get_line(2), Some("beta"));
        assert_eq!(e.get_line(0), None);  // 0 is out of range
        assert_eq!(e.get_line(3), None);  // past end
    }

    #[test]
    fn test_line_count() {
        let mut e = Editor::new();
        e.buffer = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(e.line_count(), 3);
    }

    #[test]
    fn test_set_line_one_indexed() {
        let mut e = Editor::new();
        e.buffer = vec!["hello".to_string(), "world".to_string()];
        e.set_line(1, "goodbye".to_string());
        assert_eq!(e.buffer[0], "goodbye");
        e.set_line(0, "noop".to_string()); // out of range — no-op
        assert_eq!(e.buffer[0], "goodbye");
    }

    #[test]
    fn test_insert_line_one_indexed() {
        let mut e = Editor::new();
        e.buffer = vec!["a".to_string(), "c".to_string()];
        e.insert_line(2, "b".to_string()); // insert before line 2
        assert_eq!(e.buffer, vec!["a", "b", "c"]);
        e.insert_line(0, "noop".to_string()); // out of range — no-op
        assert_eq!(e.buffer, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_delete_line_one_indexed() {
        let mut e = Editor::new();
        e.buffer = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        e.delete_line(2);
        assert_eq!(e.buffer, vec!["a", "c"]);
        e.delete_line(0); // out of range — no-op
        assert_eq!(e.buffer, vec!["a", "c"]);
    }
}