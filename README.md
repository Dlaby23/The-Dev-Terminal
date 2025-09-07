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

### Currently Implemented (Week 1)
- **Native Metal Rendering** - Direct GPU acceleration via wgpu
- **Instant Input Response** - Every keystroke rendered in under 1 frame
- **Smart Zoom Controls** - ⌘+/⌘-/⌘0 with dynamic grid recalculation
- **Proper VT Parsing** - Full ANSI/VT escape sequence support
- **Unicode Support** - Handles wide characters correctly
- **Real PTY Integration** - Proper shell interaction, not a simulation

### Coming Soon
- **AI-First Features** - Optimized for Claude Code workflows
- **Predictive Rendering** - Start rendering before input completes
- **Smart Caching** - Intelligent frame caching for instant scrollback
- **Ligature Support** - Beautiful code typography without performance cost
- **Theme Hot-Reload** - Change themes without missing a beat
- **Multi-Tab Performance** - 100 tabs? No problem.

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

| Shortcut | Action |
|----------|--------|
| ⌘+ | Zoom in |
| ⌘- | Zoom out |
| ⌘0 | Reset zoom |

## 🗺️ Roadmap

### Week 1 ✅
- Basic PTY integration
- VT parser implementation
- GPU text rendering
- Keyboard input handling
- Zoom functionality

### Week 2 (In Progress)
- Performance optimizations
- Smooth scrolling
- Font selection
- Basic theming

### Week 3
- Tabs and splits
- Search functionality
- Copy/paste with system clipboard
- URL detection and clicking

### Week 4
- Configuration system
- Theme customization
- Plugin architecture
- Performance profiling tools

### Future
- AI integration features
- Collaborative sessions
- Cloud sync
- Advanced rendering effects

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