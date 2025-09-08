# The Dev Terminal 🚀

A blazingly fast, GPU-accelerated terminal emulator built for developers who demand zero-latency input and buttery-smooth rendering. Designed from the ground up for vibe coding sessions with Claude Code and modern development workflows.

## Why Another Terminal?

Because every millisecond matters when you're in the flow. The Dev Terminal is engineered for developers who:
- Can't stand input lag disrupting their thought process
- Want GPU-accelerated rendering for smooth scrolling at any speed
- Need a terminal that keeps up with AI-assisted coding workflows
- Appreciate native macOS performance with Metal acceleration
- Demand instant feedback for every keystroke

## 🎯 Core Philosophy

**Speed First, Everything Else Second**

This isn't just another pretty terminal. Every architectural decision prioritizes raw performance:
- Direct Metal/GPU rendering pipeline (via wgpu)
- Zero-copy data paths from PTY to screen
- Lock-free async architecture where possible
- Native performance, no Electron overhead
- Optimized for M-series Apple Silicon

## ⚡ Features

### Currently Implemented ✅
- **Native Metal Rendering** - Direct GPU acceleration via wgpu
- **Instant Input Response** - Every keystroke rendered in under 1 frame
- **Smooth Inertial Scrolling** - Buttery smooth trackpad scrolling with physics
- **Smart Selection** - Single/double/triple click for char/word/line selection
- **URL Detection** - Cmd+Click to open URLs in browser
- **Search Functionality** - ⌘F to search through terminal content
- **Smart Zoom Controls** - ⌘+/⌘-/⌘0 with dynamic grid recalculation
- **Proper VT Parsing** - Full ANSI/VT escape sequence support with CSI modes
- **Unicode Support** - Handles wide characters correctly
- **Real PTY Integration** - Proper shell interaction with bracketed paste
- **Configuration System** - TOML-based config with hot-reload support
- **Performance Monitoring** - Built-in FPS counter and latency tracking
- **Scrollback Buffer** - 10,000 lines of history with efficient memory usage
- **Copy/Paste** - Full system clipboard integration
- **256 Color Support** - Complete ANSI color palette

### Advanced Features
- **Edge-Clamped Scrolling** - No jitter at viewport boundaries
- **Sub-Row Rendering** - Pixel-perfect smooth scrolling
- **Resize Preservation** - Content stays stable during window resize
- **Stick-to-Bottom** - Auto-follow new content when at bottom
- **Memory Efficient** - < 50MB for typical sessions

## 🏗️ Architecture

Built with a modular Rust workspace:
```
crates/
├── core/          # VT parser, grid management, PTY handling
├── ui-wgpu/       # GPU renderer with Metal backend
└── apps/terminal/ # Window management and event loop
```

### Tech Stack
- **Language**: Rust (stable, 2024 edition) - For zero-cost abstractions
- **GPU**: wgpu with Metal backend - Native macOS performance
- **Text Rendering**: Glyphon + Cosmic Text - GPU-accelerated text
- **Window System**: Winit - Minimal overhead event handling
- **PTY**: portable-pty - Cross-platform terminal interface
- **Parser**: vte - Battle-tested VT sequence parsing

## 🚀 Performance Targets

- **Input Latency**: < 1ms from keystroke to PTY write
- **Render Latency**: < 8ms from PTY output to pixels (120fps capable)
- **Scroll Performance**: 60fps minimum at any speed
- **Memory Usage**: < 50MB for typical sessions
- **Startup Time**: < 100ms cold start

## 🛠️ Development

### Prerequisites
- Rust 1.75+ (stable)
- macOS 13+ (Metal support required)
- Xcode Command Line Tools

### Quick Start
```bash
# Clone the repository
git clone https://github.com/Dlaby23/The-Dev-Terminal.git
cd The-Dev-Terminal

# Build and run in release mode (recommended for performance)
cargo run --release

# Run with debug logging
RUST_LOG=info cargo run --release
```

### Development Commands
```bash
# Run tests
cargo test

# Check types
cargo check

# Format code
cargo fmt

# Lint
cargo clippy
```

## 🎮 Keyboard Shortcuts

### Essential Commands
| Shortcut | Action |
|----------|--------|
| ⌘C | Copy selection (or send SIGINT if no selection) |
| ⌘V | Paste from clipboard |
| ⌘K | Clear screen and scrollback |
| ⌘F | Toggle search mode |
| ⌘W | Close window |

### Zoom Controls
| Shortcut | Action |
|----------|--------|
| ⌘+ | Zoom in |
| ⌘- | Zoom out |
| ⌘0 | Reset zoom |

### Navigation
| Shortcut | Action |
|----------|--------|
| ⌘← | Jump to start of line |
| ⌘→ | Jump to end of line |
| ⌘Backspace | Delete to start of line |
| Option+← | Move back one word |
| Option+→ | Move forward one word |
| Option+Backspace | Delete previous word |
| PageUp | Scroll up one page |
| PageDown | Scroll down one page |
| Shift+Home | Scroll to top |
| Shift+End | Scroll to bottom |

### Mouse Actions
| Action | Result |
|--------|--------|
| Click | Position cursor |
| Drag | Select text |
| Double-click | Select word |
| Triple-click | Select line |
| ⌘Click on URL | Open URL in browser |
| Scroll | Smooth inertial scrolling |

## 🗺️ Roadmap

### Week 1 ✅ Complete
- Basic PTY integration
- VT parser implementation
- GPU text rendering
- Keyboard input handling
- Zoom functionality

### Week 2 ✅ Complete
- Performance optimizations
- Smooth inertial scrolling with physics
- Smart viewport management
- Edge-clamped scrolling
- Resize preservation

### Week 3 ✅ Complete
- Enhanced copy/paste with smart selection
- Search functionality (⌘F)
- URL detection and clicking
- Double/triple click selection

### Week 4 ✅ Complete
- Configuration system (TOML-based)
- Performance profiling tools
- Memory usage tracking
- FPS and latency monitoring

### Future Enhancements
- Tabs and splits
- Theme hot-reload
- Plugin architecture
- AI integration features
- Collaborative sessions
- Cloud sync
- Advanced rendering effects
- Ligature support

## 🤝 Contributing

This project is optimized for development with Claude Code. Contributions that improve performance, reduce latency, or enhance the developer experience are especially welcome.

## 📊 Benchmarks

*Coming soon - We'll be publishing comprehensive latency measurements and comparisons*

## 🎯 Design Goals

1. **Imperceptible Latency** - If you can feel it, it's too slow
2. **Predictable Performance** - No random stutters or slowdowns
3. **Developer First** - Built for people who live in the terminal
4. **AI-Native** - Optimized for AI-assisted development workflows
5. **macOS Native** - Take full advantage of Apple Silicon

## 📜 License

MIT - Because fast terminals should be free.

---

*Built for developers who type faster than they think, and think faster than their current terminal can render.*

**Status**: 🚧 Active Development - Breaking changes expected until v1.0

## 📢 Public Development Notice

This project is currently in **public development**. As I'm working on multiple projects simultaneously, there is **no set release date** for The Dev Terminal. Development happens when time permits, and features are added incrementally.

Feel free to:
- Watch the repository for updates
- Try out the current builds
- Report issues or suggest features
- Contribute if you're interested

The terminal is functional for daily use, but expect ongoing changes and improvements.