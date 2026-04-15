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

    // Clear screen
    stdout.queue(crossterm::terminal::Clear(ClearType::All))?;

    // Render visible lines
    for (i, line_idx) in (state.row_offset..state.lines.len()).enumerate() {
        if i >= visible_rows {
            break;
        }
        stdout.queue(MoveTo(0, i as u16))?;
        let line = &state.lines[line_idx];
        let truncated = truncate_to_width(line, max_cols);
        stdout.queue(Print(truncated))?;
    }

    // Status bar / prompt
    stdout.queue(MoveTo(0, status_row))?;
    if let Some(prompt) = status_text(&state.prompt_mode) {
        stdout.queue(Print(truncate_to_width(&prompt, max_cols)))?;
    } else if let Some(ref msg) = state.message {
        stdout.queue(Print(truncate_to_width(msg, max_cols)))?;
    }

    // Cursor position
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
            let display_col: usize = line
                .graphemes(true)
                .take(state.cursor_col)
                .map(UnicodeWidthStr::width)
                .sum();
            stdout.queue(MoveTo(display_col as u16, screen_row))?;
        }
    }

    stdout.flush()
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
