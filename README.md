# The-Dev-Terminal

A fast, offline, macOS-first terminal emulator with a Metal (wgpu) renderer.

## Features (In Progress)

- Metal-accelerated rendering via wgpu
- PTY + VT parsing
- Editor-grade mouse UX
- Plugin-ready core

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run --release
```

## Smoketest

```bash
RUST_LOG=info,wgpu_core=warn WGPU_BACKEND=metal cargo run --release -- --smoketest
```

## Week 1 Goals

- [x] Create Cargo workspace structure
- [x] Implement EventLoop with winit and wgpu surface
- [x] Add smoketest flag for 3-frame test
- [x] Spawn zsh via portable-pty
- [x] Implement basic keyboard input handling
- [x] Integrate vte::Parser with basic VT commands
- [ ] Render grid with proper text rendering
- [ ] Handle window resize and PTY size updates

## License

MIT OR Apache-2.0