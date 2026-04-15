use crossterm::event::{Event, KeyEvent, KeyModifiers};

use crate::editor::PromptMode;

pub enum Action {
    None,
    Insert(char),
    Delete,
    Backspace,
    Newline,
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    PageUp,
    PageDown,
    Home,
    End,
    SaveAs,
    Find,
    Cut,
    Paste,
    Exit,
    // Prompt mode actions
    PromptInsert(char),
    PromptBackspace,
    PromptSubmit,
    PromptCancel,
    // Confirm-exit actions
    ExitConfirmed,
    SaveAndExit,
}

pub fn handle_input(event: Event, prompt_mode: &PromptMode) -> Action {
    if let Event::Key(key) = event {
        match prompt_mode {
            PromptMode::None => handle_key_normal(key),
            PromptMode::ConfirmExit => handle_key_confirm_exit(key),
            _ => handle_key_prompt(key),
        }
    } else {
        Action::None
    }
}

fn handle_key_normal(key: KeyEvent) -> Action {
    use crossterm::event::KeyCode::*;

    // Ctrl key combinations (nano-style)
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            Char('x') | Char('X') => Action::Exit,
            Char('s') | Char('S') => Action::SaveAs,
            Char('w') | Char('W') => Action::Find,
            Char('k') | Char('K') => Action::Cut,
            Char('u') | Char('U') => Action::Paste,
            _ => Action::None,
        };
    }

    // Regular keys
    match key.code {
        Char(c) => Action::Insert(c),
        Enter => Action::Newline,
        Backspace => Action::Backspace,
        Delete => Action::Delete,
        Up => Action::CursorUp,
        Down => Action::CursorDown,
        Left => Action::CursorLeft,
        Right => Action::CursorRight,
        PageUp => Action::PageUp,
        PageDown => Action::PageDown,
        Home => Action::Home,
        End => Action::End,
        Esc => Action::None,  // Could be used for something
        Tab => Action::Insert('\t'),
        _ => Action::None,
    }
}

fn handle_key_prompt(key: KeyEvent) -> Action {
    use crossterm::event::KeyCode::*;

    match key.code {
        Char(c) => Action::PromptInsert(c),
        Enter => Action::PromptSubmit,
        Backspace => Action::PromptBackspace,
        Esc => Action::PromptCancel,
        _ => Action::None,
    }
}

fn handle_key_confirm_exit(key: KeyEvent) -> Action {
    use crossterm::event::KeyCode::*;

    match key.code {
        Char('y') | Char('Y') | Enter => Action::SaveAndExit,
        Char('n') | Char('N') => Action::ExitConfirmed,
        Esc => Action::PromptCancel,
        _ => Action::None,
    }
}