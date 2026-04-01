# basilisk

A GPU-accelerated terminal emulator with built-in multiplexing. Because waiting for your terminal to render text is so 1995.

## Why?

Most terminal emulators still render text on the CPU. In 2026. With GPUs sitting idle. Basilisk uses wgpu for text rendering, achieving sub-millisecond frame times even with thousands of lines of colorful compiler errors.

Plus, it has tmux-like multiplexing built in. No more "I forgot to start tmux before SSHing into that server."

## Features

- GPU text rendering via wgpu
- Sub-millisecond render times
- Built-in session/window/pane multiplexing
- Full ANSI/VT100 escape sequence support
- 24-bit true color
- Configurable via TOML
- Scrollback with mouse wheel support
- Normal, block, and line text selection
- System clipboard integration

## Installation

```bash
cargo install basilisk
```

Or build from source:

```bash
git clone https://github.com/sudokatie/basilisk
cd basilisk
cargo build --release
./target/release/basilisk
```

## Quick Start

```bash
# Start with default shell
basilisk

# Start with specific shell
basilisk --shell /bin/zsh

# Use a config file
basilisk --config ~/.config/basilisk/config.toml
```

## Keyboard Shortcuts

### Session Management
| Shortcut | Action |
|----------|--------|
| Ctrl+Shift+N | New window |
| Ctrl+Shift+W | Close window |
| Ctrl+Shift+Tab | Next window |

### Pane Management
| Shortcut | Action |
|----------|--------|
| Ctrl+Shift+H | Split horizontal |
| Ctrl+Shift+V | Split vertical |
| Ctrl+Tab | Next pane |

### Scrollback
| Shortcut | Action |
|----------|--------|
| Shift+PageUp | Page up |
| Shift+PageDown | Page down |
| Shift+Home | Scroll to top |
| Shift+End | Scroll to bottom |

### Clipboard
| Shortcut | Action |
|----------|--------|
| Ctrl+Shift+C | Copy selection |
| Ctrl+Shift+V | Paste |

## Configuration

Basilisk looks for config at `~/.config/basilisk/config.toml`.

```toml
[font]
family = "JetBrains Mono"
size = 14.0

[colors]
foreground = "#c5c8c6"
background = "#1d1f21"
cursor = "#c5c8c6"

# ANSI colors
black = "#282a2e"
red = "#a54242"
green = "#8c9440"
yellow = "#de935f"
blue = "#5f819d"
magenta = "#85678f"
cyan = "#5e8d87"
white = "#707880"

[scrollback]
lines = 10000

[window]
width = 800
height = 600
opacity = 1.0
padding = 2

[terminal]
shell = "/bin/zsh"
cols = 80
rows = 24
```

## Architecture

Basilisk is built with:

- **wgpu** - Cross-platform GPU abstraction
- **winit** - Window management
- **fontdue** - Font rasterization
- **nix** - Unix PTY handling

The text rendering pipeline:
1. Terminal state (grid of cells with attributes)
2. Glyph rasterization to texture atlas
3. Quad generation for each cell
4. GPU render pass with alpha blending

This keeps the GPU doing what GPUs do best while the CPU handles escape sequences and PTY I/O.

## Performance

On a typical system:
- Frame render: <1ms
- Input latency: <5ms
- Glyph cache hit rate: >99%

The glyph atlas is sized at 1024x1024 by default, which handles most fonts comfortably. If you're using a font with 10,000+ glyphs (looking at you, Nerd Fonts), consider increasing atlas size in config.

## Supported Escape Sequences

- CSI sequences (cursor movement, erase, SGR)
- OSC sequences (window title, colors)
- DCS sequences (partial)
- C0 control codes

Not yet supported:
- Sixel graphics
- Synchronized updates (DECSET 2026)
- Some obscure DCS sequences

## License

MIT

---

*Built with Rust, caffeine, and mild frustration at existing terminal emulators.*
