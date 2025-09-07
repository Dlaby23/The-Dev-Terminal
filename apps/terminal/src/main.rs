use anyhow::Result;
use clap::Parser;
use copypasta::{ClipboardContext, ClipboardProvider};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use the_dev_terminal_core::{grid::Grid, pty::PtyHandle, vt::advance_bytes_with_bracketed};
use the_dev_terminal_ui_wgpu::Renderer;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber;
use winit::{
    event::{Event, WindowEvent, ElementState, KeyEvent, MouseButton},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    keyboard::{Key, KeyCode, PhysicalKey, ModifiersState},
    window::WindowBuilder,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    smoketest: bool,
}

#[derive(Debug, Clone)]
enum UserEvent {
    PtyData(Vec<u8>),
}

#[derive(Default, Clone, Copy)]
struct Region { 
    start: (usize, usize), 
    end: (usize, usize) 
}

#[derive(Default)]
struct SelectionState {
    dragging: bool,              // true only while mouse is down
    region: Option<Region>,      // current selection to render/copy
}

fn pixels_to_cell(x: f32, y: f32, cw: f32, ch: f32) -> (usize, usize) {
    let col = (x / cw).floor().max(0.0) as usize;
    let row = (y / ch).floor().max(0.0) as usize;
    (col, row)
}

fn copy_to_clipboard(s: &str) {
    if let Ok(mut cb) = ClipboardContext::new() {
        let _ = cb.set_contents(s.to_string());
    }
}

fn paste_from_clipboard() -> Option<String> {
    ClipboardContext::new().ok()?.get_contents().ok()
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    
    let args = Args::parse();
    
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run(args))
}

async fn run(args: Args) -> Result<()> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build()?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("The Dev Terminal")
            .with_inner_size(winit::dpi::LogicalSize::new(800, 600))
            .build(&event_loop)?
    );
    
    let mut renderer = Renderer::new(window.clone()).await?;
    
    let grid = Arc::new(Mutex::new(Grid::new(80, 25)));
    
    let (pty, pty_rx) = PtyHandle::spawn(25, 80)?;
    
    let proxy = event_loop.create_proxy();
    
    spawn_pty_reader(pty_rx, proxy.clone());
    
    let mut frame_count = 0;
    let start_time = Instant::now();
    let mut modifiers = ModifiersState::empty();
    
    // Selection state
    let mut selection = SelectionState::default();
    let mut selection_text: Option<String> = None;
    let mut cursor_position = (0.0, 0.0);
    
    // Bracketed paste state (updated by VT parser when it sees CSI ? 2004 h/l)
    let bracketed_paste_enabled = Arc::new(AtomicBool::new(false));
    
    event_loop.set_control_flow(ControlFlow::Wait);
    
    event_loop.run(move |event, elwt| {
        match event {
            Event::UserEvent(user_event) => match user_event {
                UserEvent::PtyData(data) => {
                    // Parse VT sequences and update grid
                    {
                        let mut g = grid.lock().unwrap();
                        advance_bytes_with_bracketed(&mut g, &data, Some(bracketed_paste_enabled.clone()));
                    }
                    // Get text snapshot from grid
                    let snapshot = {
                        let g = grid.lock().unwrap();
                        g.to_string_lines()
                    };
                    renderer.set_text(snapshot);
                    window.request_redraw();
                }
            },
            
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    info!("Close requested");
                    elwt.exit();
                }
                
                WindowEvent::ModifiersChanged(new_mods) => {
                    modifiers = new_mods.state();
                }
                
                WindowEvent::CursorMoved { position, .. } => {
                    cursor_position = (position.x as f32, position.y as f32);
                    // If dragging, update selection end
                    if selection.dragging {
                        if let Some(mut region) = selection.region {
                            let (col, row) = pixels_to_cell(
                                cursor_position.0,
                                cursor_position.1,
                                renderer.cell_width,
                                renderer.cell_height
                            );
                            region.end = (col, row);
                            selection.region = Some(region);
                            window.request_redraw();
                        }
                    }
                }
                
                WindowEvent::MouseInput { state, button, .. } => {
                    if button == MouseButton::Left {
                        if state == ElementState::Pressed {
                            // Begin selection
                            let (col, row) = pixels_to_cell(
                                cursor_position.0,
                                cursor_position.1,
                                renderer.cell_width,
                                renderer.cell_height
                            );
                            selection.dragging = true;
                            selection.region = Some(Region { 
                                start: (col, row), 
                                end: (col, row) 
                            });
                            selection_text = None; // Clear old selection text
                            window.request_redraw();
                        } else {
                            // Mouse released - finalize selection
                            selection.dragging = false;
                            if let Some(region) = selection.region {
                                let (x0, y0) = region.start;
                                let (x1, y1) = region.end;
                                let (minx, maxx) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
                                let (miny, maxy) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
                                let text = grid.lock().unwrap().get_text_in_region(minx, miny, maxx, maxy);
                                // Trim trailing whitespace from selection
                                let text = text.trim_end().to_string();
                                if !text.is_empty() {
                                    selection_text = Some(text.clone());
                                    info!("Selected text: {} chars", text.len());
                                } else {
                                    // Clear selection if no text selected
                                    selection.region = None;
                                    window.request_redraw();
                                }
                            }
                        }
                    }
                }
                
                WindowEvent::Resized(physical_size) => {
                    renderer.resize(physical_size);
                    
                    // Calculate cells based on actual font metrics
                    let cols = ((physical_size.width as f32) / renderer.cell_width).floor().max(1.0) as u16;
                    let rows = ((physical_size.height as f32) / renderer.cell_height).floor().max(1.0) as u16;
                    
                    // Update grid - preserve content
                    {
                        let mut g = grid.lock().unwrap();
                        g.resize_preserve(cols as usize, rows as usize);
                    }
                    
                    // Update PTY
                    let _ = pty.resize(rows, cols);
                    window.request_redraw();
                }
                
                WindowEvent::KeyboardInput {
                    event: KeyEvent {
                        state: ElementState::Pressed,
                        logical_key,
                        physical_key,
                        ..
                    },
                    ..
                } => {
                    // Handle Command-based shortcuts (macOS)
                    if modifiers.super_key() {
                        const STEP_PT: f32 = 1.0;
                        const DEFAULT_PT: f32 = 14.0;
                        
                        match physical_key {
                            // Clear screen + scrollback: ⌘K
                            PhysicalKey::Code(KeyCode::KeyK) => {
                                // Clear grid and scrollback
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.clear_all();
                                    g.x = 0;
                                    g.y = 0;
                                }
                                window.request_redraw();
                                // Ask shell to repaint prompt (Ctrl-L)
                                let _ = pty.write(b"\x0C");
                                info!("Clear screen and scrollback");
                            }
                            
                            // Copy: ⌘C (when Shift is also held) or when selection exists
                            PhysicalKey::Code(KeyCode::KeyC) => {
                                if modifiers.shift_key() || selection_text.is_some() {
                                    if let Some(text) = selection_text.as_ref() {
                                        copy_to_clipboard(text);
                                        info!("Copied to clipboard: {} chars", text.len());
                                    }
                                } else {
                                    // If no selection and no shift, let Ctrl-C through for SIGINT
                                    let _ = pty.write(b"\x03");
                                }
                            }
                            
                            // Paste: ⌘V
                            PhysicalKey::Code(KeyCode::KeyV) => {
                                if let Some(text) = paste_from_clipboard() {
                                    // Respect bracketed paste if enabled
                                    if bracketed_paste_enabled.load(Ordering::Relaxed) {
                                        let _ = pty.write(b"\x1b[200~");
                                        let _ = pty.write(text.as_bytes());
                                        let _ = pty.write(b"\x1b[201~");
                                    } else {
                                        let _ = pty.write(text.as_bytes());
                                    }
                                    info!("Pasted from clipboard: {} chars", text.len());
                                }
                            }
                            
                            // New window: ⌘N (placeholder)
                            PhysicalKey::Code(KeyCode::KeyN) => {
                                info!("TODO: New window");
                            }
                            
                            // New tab: ⌘T (placeholder)
                            PhysicalKey::Code(KeyCode::KeyT) => {
                                info!("TODO: New tab");
                            }
                            
                            // Close window: ⌘W
                            PhysicalKey::Code(KeyCode::KeyW) => {
                                info!("Close window requested");
                                elwt.exit();
                            }
                            
                            // Move to start/end of line: ⌘←/⌘→
                            PhysicalKey::Code(KeyCode::ArrowLeft) => {
                                let _ = pty.write(b"\x01"); // Ctrl-A (beginning of line)
                            }
                            PhysicalKey::Code(KeyCode::ArrowRight) => {
                                let _ = pty.write(b"\x05"); // Ctrl-E (end of line)
                            }
                            
                            // Delete to start of line: ⌘Backspace
                            PhysicalKey::Code(KeyCode::Backspace) => {
                                let _ = pty.write(b"\x15"); // Ctrl-U
                            }
                            
                            // Zoom controls
                            // Cmd + (Note: '+' is Shift + '=' so we watch Equal)
                            PhysicalKey::Code(KeyCode::Equal) => {
                                let new_size = renderer.font_size() + STEP_PT;
                                renderer.set_font_size(new_size);
                                
                                // Recalculate cols/rows with new font size
                                let size = window.inner_size();
                                let cols = ((size.width as f32) / renderer.cell_width).floor().max(1.0) as u16;
                                let rows = ((size.height as f32) / renderer.cell_height).floor().max(1.0) as u16;
                                
                                // Update grid - preserve content
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.resize_preserve(cols as usize, rows as usize);
                                }
                                
                                // Update PTY
                                let _ = pty.resize(rows, cols);
                                window.request_redraw();
                                info!("Zoom in: font size {}", renderer.font_size());
                            }
                            // Cmd -
                            PhysicalKey::Code(KeyCode::Minus) => {
                                let new_size = renderer.font_size() - STEP_PT;
                                renderer.set_font_size(new_size);
                                
                                // Recalculate cols/rows with new font size
                                let size = window.inner_size();
                                let cols = ((size.width as f32) / renderer.cell_width).floor().max(1.0) as u16;
                                let rows = ((size.height as f32) / renderer.cell_height).floor().max(1.0) as u16;
                                
                                // Update grid - preserve content
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.resize_preserve(cols as usize, rows as usize);
                                }
                                
                                // Update PTY
                                let _ = pty.resize(rows, cols);
                                window.request_redraw();
                                info!("Zoom out: font size {}", renderer.font_size());
                            }
                            // Cmd 0 (reset)
                            PhysicalKey::Code(KeyCode::Digit0) => {
                                renderer.set_font_size(DEFAULT_PT);
                                
                                // Recalculate cols/rows with new font size
                                let size = window.inner_size();
                                let cols = ((size.width as f32) / renderer.cell_width).floor().max(1.0) as u16;
                                let rows = ((size.height as f32) / renderer.cell_height).floor().max(1.0) as u16;
                                
                                // Update grid - preserve content
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.resize_preserve(cols as usize, rows as usize);
                                }
                                
                                // Update PTY
                                let _ = pty.resize(rows, cols);
                                window.request_redraw();
                                info!("Zoom reset: font size {}", DEFAULT_PT);
                            }
                            _ => {}
                        }
                        // Don't process normal input when Command is held
                        return;
                    }
                    
                    // Handle Option-based shortcuts (word navigation)
                    if modifiers.alt_key() {
                        match physical_key {
                            // Option+← / → : back/forward by word
                            PhysicalKey::Code(KeyCode::ArrowLeft) => {
                                let _ = pty.write(b"\x1bb"); // ESC b (backward word)
                            }
                            PhysicalKey::Code(KeyCode::ArrowRight) => {
                                let _ = pty.write(b"\x1bf"); // ESC f (forward word)
                            }
                            
                            // Option+Backspace: delete previous word
                            PhysicalKey::Code(KeyCode::Backspace) => {
                                let _ = pty.write(b"\x17"); // Ctrl-W
                            }
                            
                            // Option+D: delete next word
                            PhysicalKey::Code(KeyCode::KeyD) => {
                                let _ = pty.write(b"\x1bd"); // ESC d
                            }
                            
                            _ => {}
                        }
                        // Don't process normal input when Option is held
                        return;
                    }
                    
                    // Handle Control shortcuts
                    if modifiers.control_key() {
                        match physical_key {
                            PhysicalKey::Code(KeyCode::KeyC) => {
                                let _ = pty.write(b"\x03"); // Ctrl-C (SIGINT)
                                return;
                            }
                            PhysicalKey::Code(KeyCode::KeyD) => {
                                let _ = pty.write(b"\x04"); // Ctrl-D (EOF)
                                return;
                            }
                            PhysicalKey::Code(KeyCode::KeyZ) => {
                                let _ = pty.write(b"\x1A"); // Ctrl-Z (suspend)
                                return;
                            }
                            PhysicalKey::Code(KeyCode::KeyL) => {
                                let _ = pty.write(b"\x0C"); // Ctrl-L (clear)
                                return;
                            }
                            _ => {}
                        }
                    }
                    
                    // Handle special keys using physical key
                    let seq: Option<&[u8]> = match physical_key {
                        PhysicalKey::Code(KeyCode::Space) => Some(b" "),  // Ensure space is sent
                        PhysicalKey::Code(KeyCode::Enter) => Some(b"\r"),
                        PhysicalKey::Code(KeyCode::Backspace) => Some(b"\x7f"),
                        PhysicalKey::Code(KeyCode::Tab) => Some(b"\t"),
                        PhysicalKey::Code(KeyCode::Escape) => Some(b"\x1b"),
                        PhysicalKey::Code(KeyCode::ArrowUp) => Some(b"\x1b[A"),
                        PhysicalKey::Code(KeyCode::ArrowDown) => Some(b"\x1b[B"),
                        PhysicalKey::Code(KeyCode::ArrowRight) => Some(b"\x1b[C"),
                        PhysicalKey::Code(KeyCode::ArrowLeft) => Some(b"\x1b[D"),
                        _ => {
                            // Handle regular characters via logical key
                            if let Key::Character(s) = logical_key {
                                // Log what we're sending for debugging
                                if s == " " {
                                    info!("Sending space character to PTY");
                                }
                                if let Err(e) = pty.write(s.as_bytes()) {
                                    error!("Failed to write to PTY: {}", e);
                                }
                            }
                            None
                        }
                    };
                    
                    if let Some(s) = seq {
                        if let Err(e) = pty.write(s) {
                            error!("Failed to write to PTY: {}", e);
                        }
                    }
                }
                
                WindowEvent::RedrawRequested => {
                    // Update renderer with current selection for highlighting
                    if let Some(region) = selection.region {
                        renderer.selection = Some((region.start, region.end));
                    } else {
                        renderer.selection = None;
                    }
                    
                    if let Err(e) = renderer.render_frame() {
                        match e.downcast_ref::<wgpu::SurfaceError>() {
                            Some(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                let size = window.inner_size();
                                renderer.resize(size);
                            }
                            Some(wgpu::SurfaceError::OutOfMemory) => {
                                error!("Out of memory");
                                elwt.exit();
                            }
                            _ => error!("Render error: {:?}", e),
                        }
                    }
                    
                    frame_count += 1;
                    info!("Frame {} presented", frame_count);
                    
                    if args.smoketest {
                        if frame_count >= 3 {
                            info!("Smoketest passed: {} frames", frame_count);
                            std::process::exit(0);
                        } else {
                            window.request_redraw();
                        }
                    }
                }
                
                _ => {}
            },
            
            Event::AboutToWait => {
                if args.smoketest && start_time.elapsed() > Duration::from_secs(5) {
                    error!("Smoketest failed: timeout");
                    std::process::exit(1);
                }
            }
            
            _ => {}
        }
    })?;
    
    Ok(())
}

fn spawn_pty_reader(mut pty_rx: mpsc::UnboundedReceiver<Vec<u8>>, proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        while let Some(data) = pty_rx.blocking_recv() {
            let _ = proxy.send_event(UserEvent::PtyData(data));
        }
    });
}