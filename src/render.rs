use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    style::Print,
    terminal::ClearType,
    QueueableCommand,
};
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

    // Clear screen
    stdout.queue(crossterm::terminal::Clear(ClearType::All))?;

    // Render visible lines
    for (i, line_idx) in (state.row_offset..state.lines.len()).enumerate() {
        if i >= visible_rows {
            break;
        }
        stdout.queue(MoveTo(0, i as u16))?;
        let line = &state.lines[line_idx];
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

    // Status bar / prompt
    stdout.queue(MoveTo(0, rows - 1))?;
    match &state.prompt_mode {
        PromptMode::SaveAs(input) => {
            let prompt = format!("Save as: {}", input);
            stdout.queue(Print(truncate_to_width(&prompt, cols as usize)))?;
        }
        PromptMode::Find(input) => {
            let prompt = format!("Find: {}", input);
            stdout.queue(Print(truncate_to_width(&prompt, cols as usize)))?;
        }
        PromptMode::ConfirmExit => {
            stdout.queue(Print("Save changes? (Y/n): "))?;
        }
        PromptMode::None => {
            if let Some(ref msg) = state.message {
                stdout.queue(Print(truncate_to_width(msg, cols as usize)))?;
            }
        }
    }

    // Cursor position
    match &state.prompt_mode {
        PromptMode::SaveAs(input) => {
            let prompt = format!("Save as: {}", input);
            let w = UnicodeWidthStr::width(prompt.as_str()) as u16;
            stdout.queue(MoveTo(w.min(cols.saturating_sub(1)), rows - 1))?;
        }
        PromptMode::Find(input) => {
            let prompt = format!("Find: {}", input);
            let w = UnicodeWidthStr::width(prompt.as_str()) as u16;
            stdout.queue(MoveTo(w.min(cols.saturating_sub(1)), rows - 1))?;
        }
        PromptMode::ConfirmExit => {
            stdout.queue(MoveTo(20, rows - 1))?;
        }
        PromptMode::None => {
            let screen_row = (state.cursor_row - state.row_offset) as u16;
            let line = state.lines.get(state.cursor_row).map(String::as_str).unwrap_or("");
            let display_col: usize = line.graphemes(true)
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
