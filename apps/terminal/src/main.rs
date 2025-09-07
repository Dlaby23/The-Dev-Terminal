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
    keyboard::{Key, KeyCode, PhysicalKey},
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
    let pty = Arc::new(pty);
    
    let proxy = event_loop.create_proxy();
    
    spawn_pty_reader(pty_rx, proxy.clone());
    
    // Test command injection to verify typing works
    if !args.smoketest {
        let pty_test = pty.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let _ = pty_test.write(b"echo 'Terminal OK' && printf \"\\n\"\r");
        });
    }
    
    let mut frame_count = 0;
    let start_time = Instant::now();
    
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
                
                WindowEvent::Resized(physical_size) => {
                    renderer.resize(physical_size);
                    
                    // Calculate cells based on font metrics (estimated)
                    const CELL_WIDTH: f32 = 9.0;
                    const CELL_HEIGHT: f32 = 18.0;
                    let cols = ((physical_size.width as f32) / CELL_WIDTH).floor().max(1.0) as u16;
                    let rows = ((physical_size.height as f32) / CELL_HEIGHT).floor().max(1.0) as u16;
                    
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
                    // Handle special keys using physical key
                    let seq: Option<&[u8]> = match physical_key {
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