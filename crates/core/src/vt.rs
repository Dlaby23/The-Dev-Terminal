use crate::grid::Grid;
use std::sync::{Arc, Mutex};
use vte::{Params, Parser, Perform};

pub struct VtParser {
    parser: Parser,
    grid: Arc<Mutex<Grid>>,
}

impl VtParser {
    pub fn new(grid: Arc<Mutex<Grid>>) -> Self {
        Self {
            parser: Parser::new(),
            grid,
        }
    }

    pub fn advance(&mut self, bytes: &[u8]) {
        let mut performer = VtPerformer {
            grid: self.grid.clone(),
        };
        
        for byte in bytes {
            self.parser.advance(&mut performer, *byte);
        }
    }
}

struct VtPerformer {
    grid: Arc<Mutex<Grid>>,
}

impl Perform for VtPerformer {
    fn print(&mut self, c: char) {
        let mut grid = self.grid.lock().unwrap();
        grid.print_char(c);
    }

    fn execute(&mut self, byte: u8) {
        let mut grid = self.grid.lock().unwrap();
        match byte {
            b'\r' => grid.carriage_return(),
            b'\n' => grid.line_feed(),
            b'\t' => {
                let tab_stop = ((grid.cursor_col / 8) + 1) * 8;
                let tab_stop = tab_stop.min(grid.cols - 1);
                for _ in grid.cursor_col..tab_stop {
                    grid.print_char(' ');
                }
            }
            b'\x08' => {
                if grid.cursor_col > 0 {
                    grid.cursor_col -= 1;
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: char) {
        let mut grid = self.grid.lock().unwrap();
        match action {
            'J' => {
                let mode = params.iter().next().map(|p| p[0]).unwrap_or(0);
                grid.erase_display(mode);
            }
            'H' | 'f' => {
                let mut params_iter = params.iter();
                let row = params_iter.next().and_then(|p| p.get(0)).map(|&v| v as usize).unwrap_or(1);
                let col = params_iter.next().and_then(|p| p.get(0)).map(|&v| v as usize).unwrap_or(1);
                grid.cursor_position(row, col);
            }
            'A' => {
                let n = params.iter().next().and_then(|p| p.get(0)).map(|&v| v as usize).unwrap_or(1);
                grid.cursor_row = grid.cursor_row.saturating_sub(n);
            }
            'B' => {
                let n = params.iter().next().and_then(|p| p.get(0)).map(|&v| v as usize).unwrap_or(1);
                grid.cursor_row = (grid.cursor_row + n).min(grid.rows - 1);
            }
            'C' => {
                let n = params.iter().next().and_then(|p| p.get(0)).map(|&v| v as usize).unwrap_or(1);
                grid.cursor_col = (grid.cursor_col + n).min(grid.cols - 1);
            }
            'D' => {
                let n = params.iter().next().and_then(|p| p.get(0)).map(|&v| v as usize).unwrap_or(1);
                grid.cursor_col = grid.cursor_col.saturating_sub(n);
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}