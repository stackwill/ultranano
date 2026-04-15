mod editor;
mod input;
mod render;

use std::env;
use std::io;

use crossterm::{
    event::read,
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use scopeguard::defer;

use editor::Editor;
use input::{handle_input, Action};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
enum StartupMode {
    Open(Option<String>),
    Help,
    Version,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let startup = parse_args(env::args())?;
    match startup {
        StartupMode::Help => {
            print_help();
            return Ok(());
        }
        StartupMode::Version => {
            print_version();
            return Ok(());
        }
        StartupMode::Open(_) => {}
    }

    let filename = match &startup {
        StartupMode::Open(path) => path.as_deref(),
        StartupMode::Help | StartupMode::Version => None,
    };

    // Enter raw terminal mode
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    // Ensure terminal is restored on exit (even on panic)
    defer! {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }

    // Create editor and load file if provided
    let mut editor = Editor::new();
    if let Some(path) = filename {
        if let Err(e) = editor.load_file(path) {
            if e.kind() == std::io::ErrorKind::NotFound {
                // New file — pre-set the filename so Ctrl+S saves to the right place
                editor.set_filename(path);
            } else {
                return Err(format!("Failed to load '{}': {}", path, e).into());
            }
        }
    }

    // Event loop
    run_event_loop(&mut editor)
}

fn parse_args<I>(args: I) -> Result<StartupMode, String>
where
    I: IntoIterator<Item = String>,
{
    let mut iter = args.into_iter();
    let program = iter.next().unwrap_or_else(|| "ultranano".to_string());
    let mut positional: Vec<String> = Vec::new();
    let mut parsing_flags = true;

    for arg in iter {
        if parsing_flags {
            match arg.as_str() {
                "-h" | "--help" => return Ok(StartupMode::Help),
                "-V" | "--version" => return Ok(StartupMode::Version),
                "--" => {
                    parsing_flags = false;
                    continue;
                }
                _ if arg.starts_with('-') => {
                    return Err(format!("Unknown option: {arg}\nTry '{program} --help'"));
                }
                _ => {}
            }
        }

        positional.push(arg);
        if positional.len() > 1 {
            return Err(format!("Too many arguments\nUsage: {program} [FILE]"));
        }
    }

    Ok(StartupMode::Open(positional.into_iter().next()))
}

fn print_help() {
    println!("ultranano - A minimal terminal text editor\n");
    println!("USAGE:");
    println!("    ultranano [FILE]\n");
    println!("OPTIONS:");
    println!("    -h, --help    Show this help message");
    println!("    -V, --version Show version information\n");
    println!("KEYBINDINGS:");
    println!("  Ctrl+X         Exit editor");
    println!("  Ctrl+S         Save / Save as");
    println!("  Ctrl+W         Find text");
    println!("  Ctrl+H         Help");
    println!("  Ctrl+K         Cut current line");
    println!("  Ctrl+U         Paste cut line");
    println!("  Arrow keys     Move cursor");
    println!("  PageUp/Down    Scroll page");
    println!("  Home/End       Jump to start/end of line");
    println!("  Enter          Insert newline");
    println!("  Backspace      Delete character before cursor");
    println!("  Delete         Delete character at cursor");
    println!("  Tab            Insert tab character");
    println!("\n  In prompts:");
    println!("  Enter          Submit");
    println!("  Esc            Cancel");
}

fn print_version() {
    println!("ultranano {}", VERSION);
}

fn run_event_loop(editor: &mut Editor) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout();

    loop {
        // Get terminal size
        let (cols, rows) = terminal::size()?;

        // Render editor
        let visible_rows = rows.saturating_sub(1) as usize;
        let state = editor.render_state(visible_rows, cols);
        render::render_frame(&state, &mut stdout, cols, rows)?;

        // Read key event (blocking)
        let event = read()?;

        // Handle terminal resize immediately
        if let crossterm::event::Event::Resize(_, _) = event {
            continue;
        }

        // Handle input
        let action = handle_input(event, &editor.prompt_mode);

        // Execute action
        match action {
            Action::None => {}
            Action::Insert(c) => {
                editor.clear_message();
                editor.insert_char(c);
            }
            Action::Delete => {
                editor.clear_message();
                editor.delete_char();
            }
            Action::Backspace => {
                editor.clear_message();
                editor.backspace();
            }
            Action::Newline => {
                editor.clear_message();
                editor.insert_newline();
            }
            Action::CursorUp => editor.cursor_up(),
            Action::CursorDown => editor.cursor_down(),
            Action::CursorLeft => editor.cursor_left(),
            Action::CursorRight => editor.cursor_right(),
            Action::PageUp => editor.page_up(rows.saturating_sub(1) as usize),
            Action::PageDown => editor.page_down(rows.saturating_sub(1) as usize),
            Action::Home => editor.cursor_home(),
            Action::End => editor.cursor_end(),
            Action::SaveAs => {
                if editor.has_custom_filename() {
                    let _ = save_with_message(editor);
                } else {
                    editor.start_save_as_prompt();
                }
            }
            Action::Find => {
                editor.start_find_prompt();
            }
            Action::Help => editor.toggle_help(),
            Action::DismissHelp => editor.dismiss_help(),
            Action::Cut => editor.cut_line(),
            Action::Paste => editor.paste(),
            Action::Exit => {
                if editor.is_dirty() {
                    editor.start_confirm_exit_prompt();
                } else {
                    break;
                }
            }
            Action::ExitConfirmed => break,
            Action::SaveAndExit => {
                if editor.get_filename().is_some() {
                    if save_with_message(editor) {
                        break;
                    }
                } else {
                    // No filename yet — prompt for one, then exit
                    editor.start_save_as_and_exit_prompt();
                }
            }
            // Prompt mode actions
            Action::PromptInsert(c) => editor.prompt_insert_char(c),
            Action::PromptBackspace => editor.prompt_backspace(),
            Action::PromptSubmit => {
                editor.prompt_submit();
                if editor.pending_exit {
                    break;
                }
            }
            Action::PromptCancel => editor.prompt_cancel(),
        }
    }

    Ok(())
}

fn save_with_message(editor: &mut Editor) -> bool {
    match editor.save() {
        Ok(()) => true,
        Err(e) => {
            editor.message = Some(format!("Error saving: {}", e));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_args, StartupMode};

    fn parse(args: &[&str]) -> Result<StartupMode, String> {
        parse_args(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn parses_help_flag() {
        assert!(matches!(parse(&["un", "--help"]), Ok(StartupMode::Help)));
        assert!(matches!(parse(&["un", "-h"]), Ok(StartupMode::Help)));
    }

    #[test]
    fn parses_version_flag() {
        assert!(matches!(parse(&["un", "--version"]), Ok(StartupMode::Version)));
        assert!(matches!(parse(&["un", "-V"]), Ok(StartupMode::Version)));
    }

    #[test]
    fn parses_optional_filename() {
        assert!(matches!(parse(&["un"]), Ok(StartupMode::Open(None))));
        assert!(matches!(
            parse(&["un", "notes.txt"]),
            Ok(StartupMode::Open(Some(path))) if path == "notes.txt"
        ));
    }

    #[test]
    fn supports_double_dash_before_filename() {
        assert!(matches!(
            parse(&["un", "--", "--notes.txt"]),
            Ok(StartupMode::Open(Some(path))) if path == "--notes.txt"
        ));
    }

    #[test]
    fn rejects_unknown_flags() {
        let error = parse(&["un", "--wat"]).unwrap_err();
        assert!(error.contains("Unknown option: --wat"));
    }

    #[test]
    fn rejects_extra_positional_args() {
        let error = parse(&["un", "a.txt", "b.txt"]).unwrap_err();
        assert!(error.contains("Too many arguments"));
    }
}
