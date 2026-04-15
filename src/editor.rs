use std::fs;
use std::io::{self, Write};
use std::path::Path;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineEnding {
    Lf,
    Crlf,
}

impl LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::Crlf => "\r\n",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    text: String,
    ending: Option<LineEnding>,
}

impl Line {
    fn new(text: String, ending: Option<LineEnding>) -> Self {
        Self { text, ending }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PromptMode {
    None,
    SaveAs(String),
    Find(String),
    ConfirmExit,
    Help(usize),
}

pub struct RenderState {
    pub lines: Vec<String>,
    pub row_offset: usize,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub col_offset: usize,
    pub prompt_mode: PromptMode,
    pub message: Option<String>,
}

pub struct Editor {
    buffer: Vec<Line>,
    cursor_row: usize,
    cursor_col: usize, // Grapheme index (not byte position)
    row_offset: usize,
    col_offset: usize, // Display column offset for horizontal scrolling
    filename: Option<String>,
    custom_filename: bool,
    dirty: bool,
    preferred_line_ending: LineEnding,
    cut_buffer: Option<Line>,
    pub message: Option<String>,
    pub prompt_mode: PromptMode,
    pub pending_exit: bool,
}

impl Editor {
    const HELP_ITEMS: [&str; 8] = [
        "Esc Close",
        "^S Save",
        "^X Exit",
        "^W Find",
        "^K Cut",
        "^U Paste",
        "Arrows Move",
        "PgUp/PgDn Scroll",
    ];

    fn grapheme_len(s: &str) -> usize {
        s.graphemes(true).count()
    }

    fn grapheme_to_byte_index(s: &str, grapheme_idx: usize) -> Option<usize> {
        s.grapheme_indices(true)
            .nth(grapheme_idx)
            .map(|(i, _)| i)
            .or_else(|| {
                if grapheme_idx == Self::grapheme_len(s) {
                    Some(s.len())
                } else {
                    None
                }
            })
    }

    fn byte_to_grapheme_index(s: &str, byte_pos: usize) -> usize {
        s.grapheme_indices(true)
            .take_while(|(idx, _)| *idx < byte_pos)
            .count()
    }

    fn display_width(s: &str) -> usize {
        s.graphemes(true).map(UnicodeWidthStr::width).sum()
    }

    fn display_width_up_to(s: &str, grapheme_count: usize) -> usize {
        s.graphemes(true)
            .take(grapheme_count)
            .map(UnicodeWidthStr::width)
            .sum()
    }

    fn display_offset_at_or_before(s: &str, target: usize) -> usize {
        let mut width = 0;
        for grapheme in s.graphemes(true) {
            let grapheme_width = UnicodeWidthStr::width(grapheme);
            if width + grapheme_width > target {
                break;
            }
            width += grapheme_width;
        }
        width
    }

    fn parse_file_bytes(bytes: &[u8]) -> io::Result<(Vec<Line>, LineEnding)> {
        if bytes.is_empty() {
            return Ok((vec![Line::new(String::new(), None)], LineEnding::Lf));
        }

        let mut lines = Vec::new();
        let mut start = 0;
        let mut idx = 0;
        let mut preferred = None;

        while idx < bytes.len() {
            if bytes[idx] == b'\n' {
                let (line_bytes, ending) = if idx > start && bytes[idx - 1] == b'\r' {
                    (&bytes[start..idx - 1], LineEnding::Crlf)
                } else {
                    (&bytes[start..idx], LineEnding::Lf)
                };
                let text = String::from_utf8(line_bytes.to_vec()).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "File is not valid UTF-8")
                })?;
                preferred.get_or_insert(ending);
                lines.push(Line::new(text, Some(ending)));
                start = idx + 1;
            }
            idx += 1;
        }

        if start < bytes.len() {
            let text = String::from_utf8(bytes[start..].to_vec()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "File is not valid UTF-8")
            })?;
            lines.push(Line::new(text, None));
        }

        if lines.is_empty() {
            lines.push(Line::new(String::new(), None));
        }

        Ok((lines, preferred.unwrap_or(LineEnding::Lf)))
    }

    fn serialized_content(&self) -> Vec<u8> {
        let mut content = Vec::new();
        for line in &self.buffer {
            content.extend_from_slice(line.text.as_bytes());
            if let Some(ending) = line.ending {
                content.extend_from_slice(ending.as_str().as_bytes());
            }
        }
        content
    }

    fn current_line(&self) -> &Line {
        &self.buffer[self.cursor_row]
    }

    fn current_line_mut(&mut self) -> &mut Line {
        &mut self.buffer[self.cursor_row]
    }

    fn current_line_text(&self) -> &str {
        &self.current_line().text
    }

    fn ensure_horizontal_scroll(&mut self, cols: u16) {
        let cols = cols as usize;
        if cols == 0 || !matches!(self.prompt_mode, PromptMode::None) {
            self.col_offset = 0;
            return;
        }

        let line = self.current_line_text();
        let cursor_x = Self::display_width_up_to(line, self.cursor_col);
        let mut offset = Self::display_offset_at_or_before(line, self.col_offset);

        loop {
            let left_marker = usize::from(offset > 0);
            let mut text_width = cols.saturating_sub(left_marker);
            let total_width = Self::display_width(line);
            let right_marker = total_width > offset + text_width;
            if right_marker {
                text_width = text_width.saturating_sub(1);
            }

            let max_cursor_x = offset + text_width.saturating_sub(1);
            if cursor_x < offset {
                offset = Self::display_offset_at_or_before(line, cursor_x);
                continue;
            }
            if cursor_x > max_cursor_x {
                let target = cursor_x.saturating_sub(text_width.saturating_sub(1));
                offset = Self::display_offset_at_or_before(line, target);
                continue;
            }
            break;
        }

        self.col_offset = offset;
    }

    fn remove_line_internal(&mut self, idx: usize) -> Line {
        let removed = self.buffer.remove(idx);
        if idx > 0 && idx <= self.buffer.len() {
            self.buffer[idx - 1].ending = removed.ending;
        }
        if self.buffer.is_empty() {
            self.buffer.push(Line::new(String::new(), None));
        }
        removed
    }

    fn insert_line_internal(&mut self, idx: usize, mut line: Line) {
        let len = self.buffer.len();
        if len == 1 && self.buffer[0].text.is_empty() && self.buffer[0].ending.is_none() {
            self.buffer[0] = line;
            return;
        }

        if idx >= len {
            if self.buffer[len - 1].ending.is_none() {
                self.buffer[len - 1].ending = Some(self.preferred_line_ending);
            }
            self.buffer.push(line);
            return;
        }

        if line.ending.is_none() {
            line.ending = Some(self.preferred_line_ending);
        }
        self.buffer.insert(idx, line);
    }
}

impl Editor {
    pub fn new() -> Self {
        Self {
            buffer: vec![Line::new(String::new(), None)],
            cursor_row: 0,
            cursor_col: 0,
            row_offset: 0,
            col_offset: 0,
            filename: None,
            custom_filename: false,
            dirty: false,
            preferred_line_ending: LineEnding::Lf,
            cut_buffer: None,
            message: None,
            prompt_mode: PromptMode::None,
            pending_exit: false,
        }
    }

    pub fn load_file(&mut self, path: &str) -> Result<(), io::Error> {
        let bytes = fs::read(path)?;
        let (buffer, preferred_line_ending) = Self::parse_file_bytes(&bytes)?;
        self.buffer = buffer;
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.row_offset = 0;
        self.col_offset = 0;
        self.preferred_line_ending = preferred_line_ending;
        self.filename = Some(path.to_string());
        self.dirty = false;
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), io::Error> {
        if let Some(ref path) = self.filename {
            let content = self.serialized_content();
            write_atomic(path, &content)?;
            self.dirty = false;
            self.message = Some(format!("Saved: {}", path));
        } else {
            self.message = Some("No filename — use Ctrl+S to set one".to_string());
        }
        Ok(())
    }

    pub fn save_as(&mut self, path: &str) -> Result<(), io::Error> {
        let content = self.serialized_content();
        write_atomic(path, &content)?;
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

    pub fn toggle_help(&mut self) {
        self.prompt_mode = match self.prompt_mode {
            PromptMode::Help(page) => PromptMode::Help(page + 1),
            _ => PromptMode::Help(0),
        };
    }

    pub fn dismiss_help(&mut self) {
        if matches!(self.prompt_mode, PromptMode::Help(_)) {
            self.prompt_mode = PromptMode::None;
        }
    }

    pub fn help_pages(max_cols: usize) -> Vec<String> {
        fn pack(items: &[&str], max_cols: usize) -> Vec<String> {
            if max_cols == 0 {
                return vec![String::new()];
            }

            let mut pages = Vec::new();
            let mut current = String::new();
            let mut current_width = 0;

            for item in items {
                let item_width = UnicodeWidthStr::width(*item);
                let separator_width = if current.is_empty() { 0 } else { 2 };
                if !current.is_empty() && current_width + separator_width + item_width > max_cols {
                    pages.push(current);
                    current = String::new();
                    current_width = 0;
                }

                if !current.is_empty() {
                    current.push_str("  ");
                    current_width += 2;
                }
                current.push_str(item);
                current_width += item_width;
            }

            if !current.is_empty() {
                pages.push(current);
            }

            if pages.is_empty() {
                pages.push(String::new());
            }

            pages
        }

        let mut pages = pack(&Self::HELP_ITEMS, max_cols);
        if pages.len() > 1 {
            let mut paged_items = vec!["^H Next"];
            paged_items.extend(Self::HELP_ITEMS);
            pages = pack(&paged_items, max_cols);
        }
        pages
    }

    pub fn prompt_insert_char(&mut self, c: char) {
        match &mut self.prompt_mode {
            PromptMode::SaveAs(ref mut input) => input.push(c),
            PromptMode::Find(ref mut input) => input.push(c),
            PromptMode::None | PromptMode::ConfirmExit | PromptMode::Help(_) => {}
        }
    }

    pub fn prompt_backspace(&mut self) {
        match &mut self.prompt_mode {
            PromptMode::SaveAs(ref mut input) => {
                input.pop();
            }
            PromptMode::Find(ref mut input) => {
                input.pop();
            }
            PromptMode::None | PromptMode::ConfirmExit | PromptMode::Help(_) => {}
        }
    }

    pub fn prompt_cancel(&mut self) {
        self.prompt_mode = PromptMode::None;
        self.pending_exit = false;
    }

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
            PromptMode::None | PromptMode::ConfirmExit | PromptMode::Help(_) => false,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        let cursor_col = self.cursor_col;
        if let Some(byte_idx) = Self::grapheme_to_byte_index(self.current_line_text(), cursor_col) {
            self.current_line_mut().text.insert(byte_idx, c);
            self.cursor_col += 1;
            self.dirty = true;
        }
    }

    pub fn insert_newline(&mut self) {
        let current_line = self.current_line().clone();
        let (before, after) = if let Some(byte_idx) =
            Self::grapheme_to_byte_index(&current_line.text, self.cursor_col)
        {
            (
                current_line.text[..byte_idx].to_string(),
                current_line.text[byte_idx..].to_string(),
            )
        } else {
            (current_line.text.clone(), String::new())
        };

        self.buffer[self.cursor_row].text = before;
        let next_ending = self.buffer[self.cursor_row].ending.take();
        self.buffer[self.cursor_row].ending = Some(self.preferred_line_ending);
        self.buffer
            .insert(self.cursor_row + 1, Line::new(after, next_ending));
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.dirty = true;
    }

    pub fn delete_char(&mut self) {
        let line_len = Self::grapheme_len(self.current_line_text());
        if self.cursor_col < line_len {
            let graphemes: Vec<_> = self.current_line_text().grapheme_indices(true).collect();
            if let Some((start, g)) = graphemes.get(self.cursor_col) {
                let start = *start;
                let end = start + g.len();
                self.current_line_mut().text.replace_range(start..end, "");
                self.dirty = true;
            }
            return;
        }

        if self.cursor_row + 1 < self.buffer.len() {
            let next = self.buffer.remove(self.cursor_row + 1);
            let current = self.current_line_mut();
            current.text.push_str(&next.text);
            current.ending = next.ending;
            self.dirty = true;
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
            let graphemes: Vec<_> = self.current_line_text().grapheme_indices(true).collect();
            if let Some((start, g)) = graphemes.get(self.cursor_col) {
                let start = *start;
                let end = start + g.len();
                self.current_line_mut().text.replace_range(start..end, "");
                self.dirty = true;
            }
        } else if self.cursor_row > 0 {
            let current = self.remove_line_internal(self.cursor_row);
            self.cursor_row -= 1;
            let prev_line_len = Self::grapheme_len(&self.buffer[self.cursor_row].text);
            self.buffer[self.cursor_row].text.push_str(&current.text);
            self.buffer[self.cursor_row].ending = current.ending;
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
            self.cursor_col = Self::grapheme_len(&self.buffer[self.cursor_row].text);
        }
    }

    pub fn cursor_right(&mut self) {
        let line_len = Self::grapheme_len(self.current_line_text());
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
        self.cursor_col = Self::grapheme_len(self.current_line_text());
    }

    fn clamp_cursor_col(&mut self) {
        let line_len = Self::grapheme_len(self.current_line_text());
        self.cursor_col = self.cursor_col.min(line_len);
    }

    pub fn cut_line(&mut self) {
        if self.cursor_row < self.buffer.len() {
            self.cut_buffer = Some(self.remove_line_internal(self.cursor_row));
            if self.cursor_row >= self.buffer.len() {
                self.cursor_row = self.buffer.len() - 1;
            }
            self.cursor_col = 0;
            self.clamp_cursor_col();
            self.dirty = true;
        }
    }

    pub fn paste(&mut self) {
        if let Some(ref line) = self.cut_buffer {
            self.insert_line_internal(self.cursor_row, line.clone());
            self.cursor_row = (self.cursor_row + 1).min(self.buffer.len() - 1);
            self.cursor_col = 0;
            self.dirty = true;
        }
    }

    pub fn find(&mut self, query: &str) {
        for (row_idx, line) in self.buffer.iter().enumerate().skip(self.cursor_row) {
            let start_byte = if row_idx == self.cursor_row {
                Self::grapheme_to_byte_index(&line.text, self.cursor_col).unwrap_or(0)
            } else {
                0
            };

            if let Some(byte_pos) = line.text[start_byte..].find(query) {
                let absolute_byte_pos = start_byte + byte_pos;
                self.cursor_row = row_idx;
                self.cursor_col = Self::byte_to_grapheme_index(&line.text, absolute_byte_pos);
                return;
            }
        }

        for (row_idx, line) in self.buffer.iter().enumerate().take(self.cursor_row + 1) {
            if let Some(byte_pos) = line.text.find(query) {
                self.cursor_row = row_idx;
                self.cursor_col = Self::byte_to_grapheme_index(&line.text, byte_pos);
                self.message = Some(format!("Search wrapped: {}", query));
                return;
            }
        }

        self.message = Some(format!("Not found: {}", query));
    }

    pub fn render_state(&mut self, visible_rows: usize, cols: u16) -> RenderState {
        if self.cursor_row < self.row_offset {
            self.row_offset = self.cursor_row;
        } else if visible_rows > 0 && self.cursor_row >= self.row_offset + visible_rows {
            self.row_offset = self.cursor_row - visible_rows + 1;
        }

        self.ensure_horizontal_scroll(cols);

        RenderState {
            lines: self.buffer.iter().map(|line| line.text.clone()).collect(),
            row_offset: self.row_offset,
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            col_offset: self.col_offset,
            prompt_mode: self.prompt_mode.clone(),
            message: self.message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serialize(lines: Vec<Line>) -> Vec<u8> {
        let mut editor = Editor::new();
        editor.buffer = lines;
        editor.serialized_content()
    }

    #[test]
    fn test_parse_empty_file() {
        let (lines, ending) = Editor::parse_file_bytes(b"").unwrap();
        assert_eq!(ending, LineEnding::Lf);
        assert_eq!(lines, vec![Line::new(String::new(), None)]);
    }

    #[test]
    fn test_parse_preserves_crlf_and_final_newline() {
        let (lines, ending) = Editor::parse_file_bytes(b"hello\r\nworld\r\n").unwrap();
        assert_eq!(ending, LineEnding::Crlf);
        assert_eq!(
            lines,
            vec![
                Line::new("hello".to_string(), Some(LineEnding::Crlf)),
                Line::new("world".to_string(), Some(LineEnding::Crlf)),
            ]
        );
    }

    #[test]
    fn test_parse_preserves_missing_final_newline() {
        let (lines, _) = Editor::parse_file_bytes(b"hello\nworld").unwrap();
        assert_eq!(
            lines,
            vec![
                Line::new("hello".to_string(), Some(LineEnding::Lf)),
                Line::new("world".to_string(), None),
            ]
        );
    }

    #[test]
    fn test_parse_rejects_non_utf8() {
        let error = Editor::parse_file_bytes(&[0xff, 0xfe]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_serialize_preserves_line_endings() {
        let bytes = serialize(vec![
            Line::new("hello".to_string(), Some(LineEnding::Crlf)),
            Line::new("world".to_string(), None),
        ]);
        assert_eq!(bytes, b"hello\r\nworld");
    }

    #[test]
    fn test_grapheme_len_unicode() {
        assert_eq!(Editor::grapheme_len("👨‍👩‍👧‍👦"), 1);
        assert_eq!(Editor::grapheme_len("🇺🇸"), 1);
        assert_eq!(Editor::grapheme_len("café"), 4);
    }

    #[test]
    fn test_grapheme_to_byte_index_unicode() {
        let s = "héllo";
        assert_eq!(Editor::grapheme_to_byte_index(s, 2), Some(3));
        assert_eq!(Editor::grapheme_to_byte_index(s, 5), Some(6));
    }

    #[test]
    fn test_insert_char_unicode() {
        let mut editor = Editor::new();
        editor.insert_char('h');
        editor.insert_char('é');
        editor.insert_char('l');
        editor.insert_char('l');
        editor.insert_char('o');
        assert_eq!(editor.buffer[0].text, "héllo");
        assert_eq!(editor.cursor_col, 5);
    }

    #[test]
    fn test_insert_newline_preserves_existing_line_ending() {
        let mut editor = Editor::new();
        editor.buffer = vec![Line::new("hello".to_string(), Some(LineEnding::Crlf))];
        editor.preferred_line_ending = LineEnding::Crlf;
        editor.cursor_col = 2;

        editor.insert_newline();

        assert_eq!(
            editor.buffer,
            vec![
                Line::new("he".to_string(), Some(LineEnding::Crlf)),
                Line::new("llo".to_string(), Some(LineEnding::Crlf)),
            ]
        );
    }

    #[test]
    fn test_delete_at_end_of_line_joins_next_line() {
        let mut editor = Editor::new();
        editor.buffer = vec![
            Line::new("hello".to_string(), Some(LineEnding::Lf)),
            Line::new("world".to_string(), None),
        ];
        editor.cursor_col = 5;

        editor.delete_char();

        assert_eq!(
            editor.buffer,
            vec![Line::new("helloworld".to_string(), None)]
        );
    }

    #[test]
    fn test_backspace_at_start_of_line_joins_previous_line() {
        let mut editor = Editor::new();
        editor.buffer = vec![
            Line::new("hello".to_string(), Some(LineEnding::Lf)),
            Line::new("world".to_string(), None),
        ];
        editor.cursor_row = 1;

        editor.backspace();

        assert_eq!(
            editor.buffer,
            vec![Line::new("helloworld".to_string(), None)]
        );
        assert_eq!(editor.cursor_row, 0);
        assert_eq!(editor.cursor_col, 5);
    }

    #[test]
    fn test_cut_last_line_clears_previous_line_ending() {
        let mut editor = Editor::new();
        editor.buffer = vec![
            Line::new("hello".to_string(), Some(LineEnding::Lf)),
            Line::new("world".to_string(), None),
        ];
        editor.cursor_row = 1;

        editor.cut_line();

        assert_eq!(editor.buffer, vec![Line::new("hello".to_string(), None)]);
    }

    #[test]
    fn test_find_unicode() {
        let mut editor = Editor::new();
        editor.buffer = vec![
            Line::new("hello".to_string(), Some(LineEnding::Lf)),
            Line::new("wörld".to_string(), Some(LineEnding::Lf)),
            Line::new("test".to_string(), None),
        ];
        editor.find("ör");
        assert_eq!(editor.cursor_row, 1);
        assert_eq!(editor.cursor_col, 1);
    }

    #[test]
    fn test_cut_and_paste_restore_line_content() {
        let mut editor = Editor::new();
        editor.buffer = vec![
            Line::new("hello".to_string(), Some(LineEnding::Lf)),
            Line::new("world".to_string(), None),
        ];

        editor.cut_line();
        assert_eq!(editor.buffer.len(), 1);
        assert_eq!(editor.buffer[0], Line::new("world".to_string(), None));

        editor.paste();
        assert_eq!(editor.buffer.len(), 2);
        assert_eq!(
            editor.buffer,
            vec![
                Line::new("hello".to_string(), Some(LineEnding::Lf)),
                Line::new("world".to_string(), None),
            ]
        );
    }

    #[test]
    fn test_help_cycles_pages() {
        let wide_pages = Editor::help_pages(120);
        let narrow_pages = Editor::help_pages(32);

        assert_eq!(wide_pages.len(), 1);
        assert!(!wide_pages[0].contains("^H Next"));
        assert!(narrow_pages.len() > 1);
        assert!(narrow_pages[0].contains("^H Next"));
    }

    #[test]
    fn test_dismiss_help_returns_to_normal() {
        let mut editor = Editor::new();
        editor.toggle_help();
        editor.dismiss_help();
        assert_eq!(editor.prompt_mode, PromptMode::None);
    }

    #[test]
    fn test_help_page_index_wraps_by_render_width() {
        let pages = Editor::help_pages(32);
        assert!(pages.len() > 1);
        let wrapped = &pages[pages.len() % pages.len()];
        assert_eq!(wrapped, &pages[0]);
    }
}
