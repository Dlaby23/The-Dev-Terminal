use vte::{Params, Perform};
use crate::grid::{Grid, Color};
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
                    // TODO: handle ?25h/?25l for cursor visible later
                }
            }
            return;
        }
        
        match c {
            // ED (Erase in Display) 0/1/2
            //   CSI 0 J  -> clear from cursor to end of screen
            //   CSI 1 J  -> clear from start of screen to cursor
            //   CSI 2 J  -> clear entire screen (and, by convention, home cursor)
            'J' => {
                let n = params.iter().next().and_then(|p| p.first()).copied().unwrap_or(0);
                match n {
                    0 => { // clear from cursor to end of screen
                        // clear current line from cursor to end
                        self.g.clear_eol_from_cursor();
                        // clear all lines below
                        for row in (self.g.y + 1)..self.g.rows {
                            self.g.clear_line(row);
                        }
                    }
                    1 => { // clear from start to cursor (inclusive)
                        // clear all lines above
                        for row in 0..self.g.y {
                            self.g.clear_line(row);
                        }
                        // clear beginning of current line up to cursor
                        self.g.clear_bol_to_cursor();
                    }
                    2 => { // clear entire screen and home cursor (typical terminal behavior)
                        self.g.clear_all();
                        self.g.x = 0;
                        self.g.y = 0;
                    }
                    _ => {}
                }
            }
            // EL (Erase in Line) 0/1/2
            //   CSI 0 K  -> clear from cursor to end of line
            //   CSI 1 K  -> clear from start of line to cursor
            //   CSI 2 K  -> clear entire line
            'K' => {
                let n = params.iter().next().and_then(|p| p.first()).copied().unwrap_or(0);
                match n {
                    0 => self.g.clear_eol_from_cursor(),
                    1 => self.g.clear_bol_to_cursor(),
                    2 => self.g.clear_line(self.g.y),
                    _ => {}
                }
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
            // SGR – Select Graphic Rendition (colors and text attributes)
            'm' => {
                let mut params_iter = params.iter();
                while let Some(param) = params_iter.next() {
                    for n in param {
                        match *n {
                            0 => {
                                // Reset all attributes
                                self.g.current_fg = Color::default();
                                self.g.current_bg = Color::BLACK;
                                self.g.current_bold = false;
                                self.g.current_italic = false;
                                self.g.current_underline = false;
                            }
                            1 => self.g.current_bold = true,
                            3 => self.g.current_italic = true,
                            4 => self.g.current_underline = true,
                            22 => self.g.current_bold = false,
                            23 => self.g.current_italic = false,
                            24 => self.g.current_underline = false,
                            
                            // Foreground colors
                            30..=37 => self.g.current_fg = Color::from_ansi((*n - 30) as u8),
                            38 => {
                                // Extended foreground color
                                if let Some(next_param) = params_iter.next() {
                                    if let Some(&2) = next_param.first() {
                                        // RGB color (38;2;r;g;b)
                                        let r = params_iter.next()
                                            .and_then(|p| p.first())
                                            .copied()
                                            .unwrap_or(0) as u8;
                                        let g = params_iter.next()
                                            .and_then(|p| p.first())
                                            .copied()
                                            .unwrap_or(0) as u8;
                                        let b = params_iter.next()
                                            .and_then(|p| p.first())
                                            .copied()
                                            .unwrap_or(0) as u8;
                                        self.g.current_fg = Color { r, g, b };
                                    } else if let Some(&5) = next_param.first() {
                                        // 256 color (38;5;n)
                                        if let Some(color_param) = params_iter.next() {
                                            if let Some(&color) = color_param.first() {
                                                self.g.current_fg = Color::from_ansi(color as u8);
                                            }
                                        }
                                    }
                                }
                            }
                            39 => self.g.current_fg = Color::default(), // Default foreground
                            
                            // Background colors
                            40..=47 => self.g.current_bg = Color::from_ansi((*n - 40) as u8),
                            48 => {
                                // Extended background color
                                if let Some(next_param) = params_iter.next() {
                                    if let Some(&2) = next_param.first() {
                                        // RGB color (48;2;r;g;b)
                                        let r = params_iter.next()
                                            .and_then(|p| p.first())
                                            .copied()
                                            .unwrap_or(0) as u8;
                                        let g = params_iter.next()
                                            .and_then(|p| p.first())
                                            .copied()
                                            .unwrap_or(0) as u8;
                                        let b = params_iter.next()
                                            .and_then(|p| p.first())
                                            .copied()
                                            .unwrap_or(0) as u8;
                                        self.g.current_bg = Color { r, g, b };
                                    } else if let Some(&5) = next_param.first() {
                                        // 256 color (48;5;n)
                                        if let Some(color_param) = params_iter.next() {
                                            if let Some(&color) = color_param.first() {
                                                self.g.current_bg = Color::from_ansi(color as u8);
                                            }
                                        }
                                    }
                                }
                            }
                            49 => self.g.current_bg = Color::BLACK, // Default background
                            
                            // Bright foreground colors
                            90..=97 => self.g.current_fg = Color::from_ansi(((*n - 90) + 8) as u8),
                            // Bright background colors
                            100..=107 => self.g.current_bg = Color::from_ansi(((*n - 100) + 8) as u8),
                            
                            _ => {} // Ignore other SGR codes for now
                        }
                    }
                }
            }
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