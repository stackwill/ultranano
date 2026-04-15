![header](https://capsule-render.vercel.app/api?type=waving&color=gradient&height=200&section=header&text=ultranano&fontSize=80&fontAlignY=35&desc=minimal%20terminal%20text%20editor&descAlignY=55)
---

**You have never tried a terminal text editor this light before.**

![Made with VHS](https://vhs.charm.sh/vhs-4GvO2F0mv1ifSWrBoOidRz.gif)

An incredibly minimal terminal text editor inspired by nano.

## Install

```bash
curl -sSf https://raw.githubusercontent.com/stackwill/ultranano/refs/heads/main/install.sh | bash
```

Downloads the latest pre-built binary and installs `un` to `~/.local/bin`. Linux only (x86_64 and aarch64). No dependencies required.

## Usage

```bash
un <filename>
```

Flags:

- `-h`, `--help` show help
- `-V`, `--version` show version
- `--` treat the next argument as a filename even if it starts with `-`

## Features

- Unicode-aware cursor movement and editing
- Horizontal scrolling with visible truncation markers
- Cut/paste line operations
- Search with wrap-around
- Nano-style keybindings

## Keybindings

| Key | Action |
|-----|--------|
| Ctrl+X | Exit editor |
| Ctrl+S | Save as (set filename) |
| Ctrl+W | Find text |
| Ctrl+H | Cycle inline help |
| Ctrl+K | Cut current line |
| Ctrl+U | Paste cut line |
| Arrow keys | Move cursor |
| PageUp/Down | Scroll page |
| Home/End | Jump to start/end of line |
| Enter | Insert newline |
| Backspace | Delete character before cursor |
| Delete | Delete character at cursor |
| Tab | Insert tab character |

In prompts:
| Key | Action |
|-----|--------|
| Enter | Submit |
| Esc | Cancel |

In help:
| Key | Action |
|-----|--------|
| Ctrl+H | Next help page |
| Esc | Close help |

## Uninstallation

```bash
curl -sSf https://raw.githubusercontent.com/stackwill/ultranano/refs/heads/main/uninstall.sh | bash
```

Or manually: `rm ~/.local/bin/un`
