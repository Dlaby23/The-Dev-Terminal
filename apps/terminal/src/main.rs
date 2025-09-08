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
    event::{Event, WindowEvent, ElementState, KeyEvent, MouseButton, MouseScrollDelta},
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
    last_click_time: Option<std::time::Instant>,
    last_click_pos: Option<(usize, usize)>,
    click_count: usize,          // For double/triple click detection
}

struct ScrollState {
    top_abs: usize,              // Absolute top row position (single source of truth)
    subrow: f32,                 // Fractional row offset in rows (not pixels)
    vel_rows_per_s: f32,         // Current scroll velocity for inertia
    stick_to_bottom: bool,       // Auto-scroll when new content arrives
    last_t: Instant,             // For delta time calculation
}

#[derive(Default)]
struct SearchState {
    active: bool,                // Is search mode active
    query: String,               // Current search query
    matches: Vec<(usize, usize, usize, usize)>, // (start_col, start_row, end_col, end_row)
    current_match: Option<usize>, // Index of currently highlighted match
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

fn find_word_boundaries(grid: &Grid, col: usize, row: usize) -> (usize, usize) {
    // Find word boundaries at the given position
    let line_start = row * grid.cols;
    
    // Helper to check if a character is a word boundary
    let is_word_char = |ch: char| ch.is_alphanumeric() || ch == '_';
    
    let mut start = col;
    let mut end = col;
    
    // If we're not on a word character, return the single position
    let idx = line_start + col;
    if idx >= grid.cells.len() || !is_word_char(grid.cells[idx].ch) {
        return (col, col);
    }
    
    // Find start of word
    while start > 0 {
        let idx = line_start + start - 1;
        if idx >= grid.cells.len() || !is_word_char(grid.cells[idx].ch) {
            break;
        }
        start -= 1;
    }
    
    // Find end of word
    while end < grid.cols - 1 {
        let idx = line_start + end + 1;
        if idx >= grid.cells.len() || !is_word_char(grid.cells[idx].ch) {
            break;
        }
        end += 1;
    }
    
    (start, end)
}

fn find_line_boundaries(grid: &Grid, row: usize) -> (usize, usize) {
    // Find the actual content boundaries of a line (trimming trailing spaces)
    let line_start = row * grid.cols;
    let mut end_col = grid.cols - 1;
    
    // Find last non-space character
    while end_col > 0 {
        let idx = line_start + end_col;
        if idx < grid.cells.len() && grid.cells[idx].ch != ' ' && grid.cells[idx].ch != '\0' {
            break;
        }
        end_col -= 1;
    }
    
    (0, end_col)
}

fn detect_url_at_position(grid: &Grid, col: usize, row: usize) -> Option<String> {
    // Simple URL detection - look for http:// or https:// patterns
    let line_start = row * grid.cols;
    let mut text = String::new();
    
    // Collect the line text
    for c in 0..grid.cols {
        let idx = line_start + c;
        if idx < grid.cells.len() {
            let ch = grid.cells[idx].ch;
            if ch != '\0' {
                text.push(ch);
            }
        }
    }
    
    // Look for URLs in the text
    let url_prefixes = ["http://", "https://", "ftp://", "file://"];
    for prefix in &url_prefixes {
        if let Some(start_idx) = text.find(prefix) {
            if col >= start_idx && col < start_idx + text[start_idx..].len() {
                // Find the end of the URL
                let url_start = start_idx;
                let remaining = &text[start_idx..];
                let url_end = remaining.find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '>' || c == ')' || c == ']')
                    .unwrap_or(remaining.len());
                
                let url = &text[url_start..url_start + url_end];
                return Some(url.to_string());
            }
        }
    }
    
    None
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
    
    let renderer = Arc::new(Mutex::new(Renderer::new(window.clone()).await?));
    
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
    
    // Search state
    let mut search = SearchState::default();
    
    // Initialize scroll state - stick to bottom by default
    let scroll = Arc::new(Mutex::new(ScrollState {
        top_abs: 0,
        subrow: 0.0,
        vel_rows_per_s: 0.0,
        stick_to_bottom: true,
        last_t: Instant::now(),
    }));
    
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
                    
                    // Update scroll position if stick-to-bottom is enabled
                    {
                        let g = grid.lock().unwrap();
                        let total = g.scrollback.len() + g.rows;
                        let vis = g.rows;
                        let max_top = total.saturating_sub(vis);
                        
                        let mut s = scroll.lock().unwrap();
                        if s.stick_to_bottom {
                            s.top_abs = max_top;
                            s.subrow = 0.0;
                        } else {
                            // Keep viewport valid if content grew
                            s.top_abs = s.top_abs.min(max_top);
                        }
                    }
                    
                    // Get text snapshot from grid and update cursor
                    {
                        let g = grid.lock().unwrap();
                        let cells = g.get_cells_for_display();
                        let snapshot = g.get_display_content();
                        let mut r = renderer.lock().unwrap();
                        r.set_cells(cells, g.cols, g.rows);
                        r.set_text(snapshot);
                        r.set_cursor(g.x, g.y, true);
                    }
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
                            let (cw, ch) = {
                                let r = renderer.lock().unwrap();
                                (r.cell_width, r.cell_height)
                            };
                            let (col, row) = pixels_to_cell(
                                cursor_position.0,
                                cursor_position.1,
                                cw,
                                ch
                            );
                            region.end = (col, row);
                            selection.region = Some(region);
                            window.request_redraw();
                        }
                    }
                }
                
                WindowEvent::MouseWheel { delta, .. } => {
                    // Smooth wheel/trackpad scrolling
                    let cell_h = renderer.lock().unwrap().cell_height.max(1.0);
                    let rows_delta: f32 = match delta {
                        MouseScrollDelta::LineDelta(_x, y) => -y * 3.0, // tune: 2.5..4.0
                        MouseScrollDelta::PixelDelta(p) => {
                            (-(p.y as f32) / cell_h).clamp(-60.0, 60.0)
                        }
                    };
                    
                    {
                        let mut s = scroll.lock().unwrap();
                        // Immediate response + inertia kick
                        s.subrow += rows_delta;
                        s.vel_rows_per_s += rows_delta * 12.0; // inertia gain
                        
                        // User actively scrolled → unstick from bottom
                        s.stick_to_bottom = false;
                    }
                    
                    window.request_redraw();
                }
                
                WindowEvent::MouseInput { state, button, .. } => {
                    if button == MouseButton::Left {
                        if state == ElementState::Pressed {
                            // Calculate cell position
                            let (cw, ch) = {
                                let r = renderer.lock().unwrap();
                                (r.cell_width, r.cell_height)
                            };
                            let (col, row) = pixels_to_cell(
                                cursor_position.0,
                                cursor_position.1,
                                cw,
                                ch
                            );
                            
                            // Check for Cmd+Click on URL
                            if modifiers.super_key() {
                                let g = grid.lock().unwrap();
                                if let Some(url) = detect_url_at_position(&g, col, row) {
                                    info!("Opening URL: {}", url);
                                    // Open URL in default browser
                                    #[cfg(target_os = "macos")]
                                    {
                                        let _ = std::process::Command::new("open")
                                            .arg(&url)
                                            .spawn();
                                    }
                                    return; // Don't process as normal click
                                }
                            }
                            
                            // Handle multi-click selection
                            let now = Instant::now();
                            const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(500);
                            
                            // Check if this is a double or triple click
                            if let Some(last_time) = selection.last_click_time {
                                if let Some((last_col, last_row)) = selection.last_click_pos {
                                    if now.duration_since(last_time) < DOUBLE_CLICK_TIME 
                                       && last_col == col && last_row == row {
                                        selection.click_count += 1;
                                    } else {
                                        selection.click_count = 1;
                                    }
                                } else {
                                    selection.click_count = 1;
                                }
                            } else {
                                selection.click_count = 1;
                            }
                            
                            selection.last_click_time = Some(now);
                            selection.last_click_pos = Some((col, row));
                            
                            // Perform selection based on click count
                            match selection.click_count {
                                2 => {
                                    // Double-click: select word
                                    let g = grid.lock().unwrap();
                                    let (start_col, end_col) = find_word_boundaries(&g, col, row);
                                    selection.region = Some(Region {
                                        start: (start_col, row),
                                        end: (end_col, row)
                                    });
                                    selection.dragging = false; // Don't drag on double-click
                                }
                                3 => {
                                    // Triple-click: select line
                                    let g = grid.lock().unwrap();
                                    let (start_col, end_col) = find_line_boundaries(&g, row);
                                    selection.region = Some(Region {
                                        start: (start_col, row),
                                        end: (end_col, row)
                                    });
                                    selection.dragging = false; // Don't drag on triple-click
                                    selection.click_count = 0; // Reset for next click
                                }
                                _ => {
                                    // Single click: start normal selection
                                    selection.dragging = true;
                                    selection.region = Some(Region { 
                                        start: (col, row), 
                                        end: (col, row) 
                                    });
                                }
                            }
                            
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
                    let (cols, rows) = {
                        let mut r = renderer.lock().unwrap();
                        r.resize(physical_size);
                        
                        // Calculate cells based on actual font metrics
                        let cols = ((physical_size.width as f32) / r.cell_width).floor().max(1.0) as u16;
                        let rows = ((physical_size.height as f32) / r.cell_height).floor().max(1.0) as u16;
                        (cols, rows)
                    };
                    
                    // Update grid - preserve content
                    {
                        let mut g = grid.lock().unwrap();
                        g.resize_preserve(cols as usize, rows as usize);
                    }
                    
                    // Update PTY
                    let _ = pty.resize(rows, cols);
                    
                    // Reset fractional scroll to avoid stale offsets after metrics change
                    {
                        let g = grid.lock().unwrap();
                        let total = g.scrollback.len() + g.rows;
                        let vis = g.rows;
                        let max_top = total.saturating_sub(vis);
                        
                        let mut s = scroll.lock().unwrap();
                        if s.stick_to_bottom {
                            s.top_abs = max_top;
                        } else {
                            s.top_abs = s.top_abs.min(max_top);
                        }
                        s.subrow = 0.0;
                        s.vel_rows_per_s = 0.0;
                    }
                    
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
                        const DEFAULT_PT: f32 = 18.0;
                        
                        match physical_key {
                            // Clear screen + scrollback: ⌘K
                            PhysicalKey::Code(KeyCode::KeyK) => {
                                // Clear grid and scrollback
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.clear_all();
                                    g.scrollback.clear();
                                    g.x = 0;
                                    g.y = 0;
                                }
                                {
                                    let g = grid.lock().unwrap();
                                    let cells = g.get_cells_for_display();
                                    let content = g.get_display_content();
                                    let mut r = renderer.lock().unwrap();
                                    r.set_cells(cells, g.cols, g.rows);
                                    r.set_text(content);
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
                            
                            // Find: ⌘F
                            PhysicalKey::Code(KeyCode::KeyF) => {
                                search.active = !search.active;
                                if search.active {
                                    info!("Search mode activated");
                                    // TODO: Show search UI overlay
                                } else {
                                    info!("Search mode deactivated");
                                    search.query.clear();
                                    search.matches.clear();
                                    search.current_match = None;
                                }
                                window.request_redraw();
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
                                let (cols, rows) = {
                                    let mut r = renderer.lock().unwrap();
                                    let new_size = r.font_size() + STEP_PT;
                                    r.set_font_size(new_size);
                                    
                                    // Recalculate cols/rows with new font size
                                    let size = window.inner_size();
                                    let cols = ((size.width as f32) / r.cell_width).floor().max(1.0) as u16;
                                    let rows = ((size.height as f32) / r.cell_height).floor().max(1.0) as u16;
                                    info!("Zoom in: font size {}", r.font_size());
                                    (cols, rows)
                                };
                                
                                // Update grid - preserve content
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.resize_preserve(cols as usize, rows as usize);
                                }
                                
                                // Update PTY
                                let _ = pty.resize(rows, cols);
                                
                                // Reset fractional scroll to avoid stale offsets after zoom
                                {
                                    let g = grid.lock().unwrap();
                                    let total = g.scrollback.len() + g.rows;
                                    let vis = g.rows;
                                    let max_top = total.saturating_sub(vis);
                                    
                                    let mut s = scroll.lock().unwrap();
                                    if s.stick_to_bottom {
                                        s.top_abs = max_top;
                                    } else {
                                        s.top_abs = s.top_abs.min(max_top);
                                    }
                                    s.subrow = 0.0;
                                    s.vel_rows_per_s = 0.0;
                                }
                                
                                window.request_redraw();
                            }
                            // Cmd -
                            PhysicalKey::Code(KeyCode::Minus) => {
                                let (cols, rows) = {
                                    let mut r = renderer.lock().unwrap();
                                    let new_size = r.font_size() - STEP_PT;
                                    r.set_font_size(new_size);
                                    
                                    // Recalculate cols/rows with new font size
                                    let size = window.inner_size();
                                    let cols = ((size.width as f32) / r.cell_width).floor().max(1.0) as u16;
                                    let rows = ((size.height as f32) / r.cell_height).floor().max(1.0) as u16;
                                    info!("Zoom out: font size {}", r.font_size());
                                    (cols, rows)
                                };
                                
                                // Update grid - preserve content
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.resize_preserve(cols as usize, rows as usize);
                                }
                                
                                // Update PTY
                                let _ = pty.resize(rows, cols);
                                
                                // Reset fractional scroll to avoid stale offsets after zoom
                                {
                                    let g = grid.lock().unwrap();
                                    let total = g.scrollback.len() + g.rows;
                                    let vis = g.rows;
                                    let max_top = total.saturating_sub(vis);
                                    
                                    let mut s = scroll.lock().unwrap();
                                    if s.stick_to_bottom {
                                        s.top_abs = max_top;
                                    } else {
                                        s.top_abs = s.top_abs.min(max_top);
                                    }
                                    s.subrow = 0.0;
                                    s.vel_rows_per_s = 0.0;
                                }
                                
                                window.request_redraw();
                            }
                            // Cmd 0 (reset)
                            PhysicalKey::Code(KeyCode::Digit0) => {
                                let (cols, rows) = {
                                    let mut r = renderer.lock().unwrap();
                                    r.set_font_size(DEFAULT_PT);
                                    
                                    // Recalculate cols/rows with new font size
                                    let size = window.inner_size();
                                    let cols = ((size.width as f32) / r.cell_width).floor().max(1.0) as u16;
                                    let rows = ((size.height as f32) / r.cell_height).floor().max(1.0) as u16;
                                    info!("Zoom reset: font size {}", DEFAULT_PT);
                                    (cols, rows)
                                };
                                
                                // Update grid - preserve content
                                {
                                    let mut g = grid.lock().unwrap();
                                    g.resize_preserve(cols as usize, rows as usize);
                                }
                                
                                // Update PTY
                                let _ = pty.resize(rows, cols);
                                
                                // Reset fractional scroll to avoid stale offsets after zoom reset
                                {
                                    let g = grid.lock().unwrap();
                                    let total = g.scrollback.len() + g.rows;
                                    let vis = g.rows;
                                    let max_top = total.saturating_sub(vis);
                                    
                                    let mut s = scroll.lock().unwrap();
                                    if s.stick_to_bottom {
                                        s.top_abs = max_top;
                                    } else {
                                        s.top_abs = s.top_abs.min(max_top);
                                    }
                                    s.subrow = 0.0;
                                    s.vel_rows_per_s = 0.0;
                                }
                                
                                window.request_redraw();
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
                        
                        // Scrollback controls
                        PhysicalKey::Code(KeyCode::PageUp) => {
                            {
                                let mut s = scroll.lock().unwrap();
                                let g = grid.lock().unwrap();
                                let page_size = g.rows;
                                s.top_abs = s.top_abs.saturating_sub(page_size);
                                s.subrow = 0.0;
                                s.stick_to_bottom = false;
                            }
                            window.request_redraw();
                            None
                        }
                        PhysicalKey::Code(KeyCode::PageDown) => {
                            {
                                let mut s = scroll.lock().unwrap();
                                let g = grid.lock().unwrap();
                                let page_size = g.rows;
                                let total_lines = g.scrollback.len() + g.rows;
                                let max_top = total_lines.saturating_sub(g.rows);
                                s.top_abs = (s.top_abs + page_size).min(max_top);
                                s.subrow = 0.0;
                                if s.top_abs == max_top {
                                    s.stick_to_bottom = true;
                                }
                            }
                            window.request_redraw();
                            None
                        }
                        PhysicalKey::Code(KeyCode::Home) if modifiers.shift_key() => {
                            // Shift+Home: scroll to top
                            {
                                let mut s = scroll.lock().unwrap();
                                s.top_abs = 0;
                                s.subrow = 0.0;
                                s.stick_to_bottom = false;
                            }
                            window.request_redraw();
                            None
                        }
                        PhysicalKey::Code(KeyCode::End) if modifiers.shift_key() => {
                            // Shift+End: scroll to bottom
                            {
                                let mut s = scroll.lock().unwrap();
                                let g = grid.lock().unwrap();
                                let total_lines = g.scrollback.len() + g.rows;
                                let max_top = total_lines.saturating_sub(g.rows);
                                s.top_abs = max_top;
                                s.subrow = 0.0;
                                s.stick_to_bottom = true;
                            }
                            window.request_redraw();
                            None
                        }
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
                    // Smooth scrolling animation with proper edge clamping
                    let now = Instant::now();
                    let (should_animate, top_abs, y_offset_px) = {
                        let mut s = scroll.lock().unwrap();
                        let dt = (now - s.last_t).as_secs_f32().min(0.05);
                        s.last_t = now;
                        
                        // Integrate inertia
                        s.subrow += s.vel_rows_per_s * dt;
                        // Friction (exponential-ish)
                        let friction = 8.0_f32; // higher → stops quicker
                        s.vel_rows_per_s *= (1.0 - friction * dt).clamp(0.0, 1.0);
                        
                        // Convert whole rows from subrow safely with bounds-aware loops
                        let (total, vis) = {
                            let g = grid.lock().unwrap();
                            (g.scrollback.len() + g.rows, g.rows)
                        };
                        let max_top = total.saturating_sub(vis);
                        
                        // Move up (positive subrow) while allowed
                        while s.subrow >= 1.0 && s.top_abs < max_top {
                            s.subrow -= 1.0;
                            s.top_abs += 1;
                        }
                        // Move down (negative subrow) while allowed
                        while s.subrow <= -1.0 && s.top_abs > 0 {
                            s.subrow += 1.0;
                            s.top_abs -= 1;
                        }
                        
                        // Clamp remaining fractional subrow so it never exceeds available range at edges
                        let up_room = (max_top - s.top_abs) as f32;   // how many rows we can still go up
                        let down_room = s.top_abs as f32;              // how many rows we can go down
                        
                        // Clamp carefully to avoid min > max panic
                        if up_room > 0.0 && down_room > 0.0 {
                            s.subrow = s.subrow.clamp(-(down_room.min(1.0)), up_room.min(1.0));
                        } else if up_room > 0.0 {
                            s.subrow = s.subrow.clamp(0.0, up_room.min(1.0));
                        } else if down_room > 0.0 {
                            s.subrow = s.subrow.clamp(-(down_room.min(1.0)), 0.0);
                        } else {
                            s.subrow = 0.0;
                        }
                        
                        // Auto-stick when user hasn't scrolled up and inertia is tiny
                        if (s.top_abs == max_top) && s.vel_rows_per_s.abs() < 0.02 && s.subrow.abs() < 0.02 {
                            s.stick_to_bottom = true;
                        }
                        if s.stick_to_bottom {
                            s.top_abs = max_top;
                            s.subrow = 0.0;
                            s.vel_rows_per_s = 0.0;
                        }
                        
                        let cell_h = renderer.lock().unwrap().cell_height;
                        let y_offset_px = -s.subrow * cell_h; // ONE transform for all draws
                        
                        // Keep animating while there is motion
                        let should_animate = s.vel_rows_per_s.abs() > 0.02 || s.subrow.abs() > 0.02;
                        
                        (should_animate, s.top_abs, y_offset_px)
                    };
                    
                    // Set viewport for renderer
                    {
                        let mut r = renderer.lock().unwrap();
                        r.set_viewport(top_abs, y_offset_px);
                        
                        // Update text content based on viewport
                        let (cells, content, cursor_x, cursor_y, cols, rows) = {
                            let g = grid.lock().unwrap();
                            (g.get_cells_for_display(), g.get_display_content(), g.x, g.y, g.cols, g.rows)
                        };
                        r.set_cells(cells, cols, rows);
                        r.set_text(content);
                        r.set_cursor(cursor_x, cursor_y, true);
                        
                        // Update renderer with current selection for highlighting
                        if let Some(region) = selection.region {
                            r.selection = Some((region.start, region.end));
                        } else {
                            r.selection = None;
                        }
                    }
                    
                    // Keep animating if we have velocity
                    if should_animate {
                        window.request_redraw();
                    }
                    
                    if let Err(e) = renderer.lock().unwrap().render_frame() {
                        match e.downcast_ref::<wgpu::SurfaceError>() {
                            Some(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                let size = window.inner_size();
                                renderer.lock().unwrap().resize(size);
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