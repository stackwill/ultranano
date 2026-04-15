use std::io::Write;

use crossterm::{cursor::MoveTo, style::Print, terminal::ClearType, QueueableCommand};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::editor::{PromptMode, RenderState};

pub fn render_frame<W: Write>(
    state: &RenderState,
    stdout: &mut W,
    cols: u16,
    rows: u16,
) -> std::io::Result<()> {
    let visible_rows = rows.saturating_sub(1) as usize;
    let status_row = rows - 1;
    let max_cols = cols as usize;

    stdout.queue(crossterm::terminal::Clear(ClearType::All))?;

    for (i, line_idx) in (state.row_offset..state.lines.len()).enumerate() {
        if i >= visible_rows {
            break;
        }
        stdout.queue(MoveTo(0, i as u16))?;
        let line = &state.lines[line_idx];
        let truncated = visible_line(line, state.col_offset, max_cols);
        stdout.queue(Print(truncated))?;
    }

    stdout.queue(MoveTo(0, status_row))?;
    if let Some(prompt) = status_text(&state.prompt_mode) {
        stdout.queue(Print(truncate_to_width(&prompt, max_cols)))?;
    } else if let Some(ref msg) = state.message {
        stdout.queue(Print(truncate_to_width(msg, max_cols)))?;
    }

    match &state.prompt_mode {
        PromptMode::SaveAs(_) | PromptMode::Find(_) => {
            let prompt = prompt_text(&state.prompt_mode).expect("prompt mode");
            let w = UnicodeWidthStr::width(prompt.as_str()) as u16;
            stdout.queue(MoveTo(w.min(cols.saturating_sub(1)), status_row))?;
        }
        PromptMode::ConfirmExit => {
            stdout.queue(MoveTo(20, status_row))?;
        }
        PromptMode::None => {
            let screen_row = (state.cursor_row - state.row_offset) as u16;
            let line = state
                .lines
                .get(state.cursor_row)
                .map(String::as_str)
                .unwrap_or("");
            let cursor_x = cursor_screen_x(line, state.cursor_col, state.col_offset, max_cols);
            stdout.queue(MoveTo(cursor_x.min(cols.saturating_sub(1)), screen_row))?;
        }
    }

    stdout.flush()
}

fn visible_line(line: &str, col_offset: usize, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }

    let total_width = line
        .graphemes(true)
        .map(UnicodeWidthStr::width)
        .sum::<usize>();
    let left_marker = col_offset > 0;
    let mut text_width = max_cols.saturating_sub(usize::from(left_marker));
    let right_marker = total_width > col_offset + text_width;
    if right_marker {
        text_width = text_width.saturating_sub(1);
    }

    let mut rendered = String::new();
    if left_marker {
        rendered.push('<');
    }

    let mut consumed_width = 0;
    let mut visible_width = 0;
    for grapheme in line.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if consumed_width + width <= col_offset {
            consumed_width += width;
            continue;
        }
        if visible_width + width > text_width {
            break;
        }
        rendered.push_str(grapheme);
        visible_width += width;
    }

    if right_marker {
        rendered.push('>');
    }

    rendered
}

fn cursor_screen_x(line: &str, cursor_col: usize, col_offset: usize, max_cols: usize) -> u16 {
    let total_width = line
        .graphemes(true)
        .map(UnicodeWidthStr::width)
        .sum::<usize>();
    let left_marker = col_offset > 0;
    let mut text_width = max_cols.saturating_sub(usize::from(left_marker));
    let right_marker = total_width > col_offset + text_width;
    if right_marker {
        text_width = text_width.saturating_sub(1);
    }

    let cursor_width: usize = line
        .graphemes(true)
        .take(cursor_col)
        .map(UnicodeWidthStr::width)
        .sum();

    let relative_x = cursor_width.saturating_sub(col_offset);
    let left_padding = usize::from(left_marker);
    (left_padding + relative_x.min(text_width.saturating_sub(1))) as u16
}

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

fn prompt_text(prompt_mode: &PromptMode) -> Option<String> {
    match prompt_mode {
        PromptMode::SaveAs(input) => Some(format!("Save as: {}", input)),
        PromptMode::Find(input) => Some(format!("Find: {}", input)),
        PromptMode::ConfirmExit | PromptMode::None => None,
    }
}

fn status_text(prompt_mode: &PromptMode) -> Option<String> {
    match prompt_mode {
        PromptMode::SaveAs(input) => Some(format!("Save as: {}", input)),
        PromptMode::Find(input) => Some(format!("Find: {}", input)),
        PromptMode::ConfirmExit => Some("Save changes? (Y/n): ".to_string()),
        PromptMode::None => None,
    }
}
