use anyhow::Result;
use clap::Parser;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use the_dev_terminal_core::{grid::Grid, pty::PtyHandle, vt::advance_bytes};
use the_dev_terminal_ui_wgpu::Renderer;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber;
use winit::{
    event::{Event, WindowEvent, ElementState, KeyEvent},
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
    
    event_loop.set_control_flow(ControlFlow::Wait);
    
    event_loop.run(move |event, elwt| {
        match event {
            Event::UserEvent(user_event) => match user_event {
                UserEvent::PtyData(data) => {
                    // Parse VT sequences and update grid
                    {
                        let mut g = grid.lock().unwrap();
                        advance_bytes(&mut g, &data);
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
                
                WindowEvent::Resized(physical_size) => {
                    renderer.resize(physical_size);
                    
                    // Calculate cells based on actual font metrics
                    let cols = ((physical_size.width as f32) / renderer.cell_width).floor().max(1.0) as u16;
                    let rows = ((physical_size.height as f32) / renderer.cell_height).floor().max(1.0) as u16;
                    
                    // Update grid
                    {
                        let mut g = grid.lock().unwrap();
                        g.resize(cols as usize, rows as usize);
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
                    // Handle zoom shortcuts when Command is held (macOS)
                    if modifiers.super_key() {
                        const STEP_PT: f32 = 1.0;
                        const DEFAULT_PT: f32 = 14.0;
                        
                        match physical_key {
                            // Cmd + (Note: '+' is Shift + '=' so we watch Equal)
                            PhysicalKey::Code(KeyCode::Equal) => {
                                let new_size = renderer.font_size() + STEP_PT;
                                renderer.set_font_size(new_size);
                                
                                // Recalculate cols/rows with new font size
                                let size = window.inner_size();
                                let cols = ((size.width as f32) / renderer.cell_width).floor().max(1.0) as u16;
                                let rows = ((size.height as f32) / renderer.cell_height).floor().max(1.0) as u16;
                                
                                // Update grid
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.resize(cols as usize, rows as usize);
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
                                
                                // Update grid
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.resize(cols as usize, rows as usize);
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
                                
                                // Update grid
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.resize(cols as usize, rows as usize);
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