use vte::{Params, Perform};
use crate::grid::Grid;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Performer<'a> { 
    pub g: &'a mut Grid,
    pub bracketed_paste: Option<Arc<AtomicBool>>,
}

impl<'a> Perform for Performer<'a> {
    // Printable glyphs
    fn print(&mut self, c: char) { 
        self.g.put(c); 
    }

    // C0 controls like \n \r \t \x08 (backspace)
    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.g.lf(),
            b'\r' => self.g.cr(),
            b'\t' => {
                // Tab: move to next tab stop (every 8 columns)
                let tab_stop = ((self.g.x / 8) + 1) * 8;
                let tab_stop = tab_stop.min(self.g.cols - 1);
                while self.g.x < tab_stop {
                    self.g.put(' ');
                }
            }
            0x08 => { 
                // Backspace
                if self.g.x > 0 { 
                    self.g.x -= 1; 
                } 
            }
            _ => {}
        }
    }

    // CSI sequences (ESC [ ... )
    fn csi_dispatch(&mut self, params: &Params, inter: &[u8], _ignore: bool, c: char) {
        // Handle DEC private mode set/reset (CSI ? ... h/l)
        if inter == b"?" {
            let is_set = c == 'h';
            for param in params.iter() {
                for n in param {
                    if *n == 2004 {
                        // Bracketed paste mode
                        if let Some(ref bp) = self.bracketed_paste {
                            bp.store(is_set, Ordering::Relaxed);
                        }
                    }
                }
            }
            return;
        }
        
        match c {
            // ED – erase in display (0 or 2)
            'J' => {
                let n = params.iter().next().and_then(|p| p.first()).copied().unwrap_or(0);
                if n == 2 { 
                    self.g.clear_all(); 
                }
                // (0 and 1 can be added later)
            }
            // EL – erase in line (0)
            'K' => { 
                self.g.clear_eol(); 
            }
            // CUP – cursor position: 1-based row;col
            'H' | 'f' => {
                let mut it = params.iter();
                let row = it.next().and_then(|p| p.first()).copied().unwrap_or(1) as usize;
                let col = it.next().and_then(|p| p.first()).copied().unwrap_or(1) as usize;
                self.g.y = row.saturating_sub(1).min(self.g.rows.saturating_sub(1));
                self.g.x = col.saturating_sub(1).min(self.g.cols.saturating_sub(1));
            }
            // Cursor movement
            'A' => {
                // Cursor up
                let n = params.iter().next().and_then(|p| p.first()).copied().unwrap_or(1) as usize;
                self.g.y = self.g.y.saturating_sub(n);
            }
            'B' => {
                // Cursor down
                let n = params.iter().next().and_then(|p| p.first()).copied().unwrap_or(1) as usize;
                self.g.y = (self.g.y + n).min(self.g.rows - 1);
            }
            'C' => {
                // Cursor forward
                let n = params.iter().next().and_then(|p| p.first()).copied().unwrap_or(1) as usize;
                self.g.x = (self.g.x + n).min(self.g.cols - 1);
            }
            'D' => {
                // Cursor backward
                let n = params.iter().next().and_then(|p| p.first()).copied().unwrap_or(1) as usize;
                self.g.x = self.g.x.saturating_sub(n);
            }
            // SGR – ignore colors/styles for now
            'm' => {}
            _ => {}
        }
    }

    // ESC single-char sequences; ignore for now
    fn esc_dispatch(&mut self, _inter: &[u8], _ignore: bool, _byte: u8) {}
    
    // OSC (ESC ] ... BEL) – vte will swallow; ignore payload
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    
    // Hooks for device control strings
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
}

pub fn advance_bytes(g: &mut Grid, bytes: &[u8]) {
    advance_bytes_with_bracketed(g, bytes, None);
}

pub fn advance_bytes_with_bracketed(g: &mut Grid, bytes: &[u8], bracketed_paste: Option<Arc<AtomicBool>>) {
    static PARSER: std::sync::OnceLock<std::sync::Mutex<vte::Parser>> = std::sync::OnceLock::new();
    let mut parser = PARSER.get_or_init(|| std::sync::Mutex::new(vte::Parser::new())).lock().unwrap();
    let mut p = Performer { g, bracketed_paste };
    for &b in bytes { 
        parser.advance(&mut p, b); 
    }
}