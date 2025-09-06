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
    keyboard::{Key, NamedKey},
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
    RequestRedraw,
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
                UserEvent::RequestRedraw => {
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
                    
                    // Estimate cell size (roughly)
                    let cols = (physical_size.width / 10).max(20) as u16;
                    let rows = (physical_size.height / 20).max(10) as u16;
                    
                    // Update grid
                    {
                        let mut g = grid.lock().unwrap();
                        g.resize(cols as usize, rows as usize);
                    }
                    
                    // Update PTY
                    let _ = pty.resize(rows, cols);
                }
                
                WindowEvent::KeyboardInput {
                    event: KeyEvent {
                        state: ElementState::Pressed,
                        logical_key,
                        ..
                    },
                    ..
                } => {
                    handle_keyboard_input(&pty, logical_key);
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

fn handle_keyboard_input(pty: &PtyHandle, key: Key) {
    let bytes = match key {
        Key::Named(NamedKey::Enter) => vec![b'\r'],
        Key::Named(NamedKey::Backspace) => vec![0x7f],
        Key::Named(NamedKey::Tab) => vec![b'\t'],
        Key::Named(NamedKey::Escape) => vec![0x1b],
        Key::Named(NamedKey::ArrowUp) => vec![0x1b, b'[', b'A'],
        Key::Named(NamedKey::ArrowDown) => vec![0x1b, b'[', b'B'],
        Key::Named(NamedKey::ArrowRight) => vec![0x1b, b'[', b'C'],
        Key::Named(NamedKey::ArrowLeft) => vec![0x1b, b'[', b'D'],
        Key::Character(ref s) => s.as_bytes().to_vec(),
        _ => return,
    };
    
    if let Err(e) = pty.write(&bytes) {
        error!("Failed to write to PTY: {}", e);
    }
}