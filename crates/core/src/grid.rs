use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CellAttributes {
    pub fg_color: Option<[u8; 3]>,
    pub bg_color: Option<[u8; 3]>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Cell {
    pub c: char,
    pub attrs: CellAttributes,
}

pub struct Grid {
    pub cells: Vec<Vec<Cell>>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub rows: usize,
    pub cols: usize,
    scrollback: Vec<Vec<Cell>>,
    max_scrollback_lines: usize,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        Self {
            cells,
            cursor_row: 0,
            cursor_col: 0,
            rows,
            cols,
            scrollback: Vec::new(),
            max_scrollback_lines: 10000,
        }
    }

    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        let mut new_cells = vec![vec![Cell::default(); new_cols]; new_rows];
        
        for (row_idx, row) in self.cells.iter().take(new_rows.min(self.rows)).enumerate() {
            for (col_idx, cell) in row.iter().take(new_cols.min(self.cols)).enumerate() {
                new_cells[row_idx][col_idx] = cell.clone();
            }
        }
        
        self.cells = new_cells;
        self.rows = new_rows;
        self.cols = new_cols;
        
        if self.cursor_row >= new_rows {
            self.cursor_row = new_rows.saturating_sub(1);
        }
        if self.cursor_col >= new_cols {
            self.cursor_col = new_cols.saturating_sub(1);
        }
    }

    pub fn print_char(&mut self, c: char) {
        if self.cursor_col < self.cols && self.cursor_row < self.rows {
            self.cells[self.cursor_row][self.cursor_col].c = c;
            self.cursor_col += 1;
            
            if self.cursor_col >= self.cols {
                self.cursor_col = 0;
                self.line_feed();
            }
        }
    }

    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    pub fn line_feed(&mut self) {
        self.cursor_row += 1;
        if self.cursor_row >= self.rows {
            self.scroll_up();
            self.cursor_row = self.rows - 1;
        }
    }

    pub fn erase_display(&mut self, mode: u16) {
        match mode {
            0 => {
                for col in self.cursor_col..self.cols {
                    self.cells[self.cursor_row][col] = Cell::default();
                }
                for row in (self.cursor_row + 1)..self.rows {
                    for col in 0..self.cols {
                        self.cells[row][col] = Cell::default();
                    }
                }
            }
            1 => {
                for col in 0..=self.cursor_col {
                    self.cells[self.cursor_row][col] = Cell::default();
                }
                for row in 0..self.cursor_row {
                    for col in 0..self.cols {
                        self.cells[row][col] = Cell::default();
                    }
                }
            }
            2 => {
                for row in 0..self.rows {
                    for col in 0..self.cols {
                        self.cells[row][col] = Cell::default();
                    }
                }
            }
            _ => {}
        }
    }

    pub fn cursor_position(&mut self, row: usize, col: usize) {
        self.cursor_row = row.saturating_sub(1).min(self.rows - 1);
        self.cursor_col = col.saturating_sub(1).min(self.cols - 1);
    }

    fn scroll_up(&mut self) {
        if !self.cells.is_empty() {
            let line = self.cells.remove(0);
            if self.scrollback.len() >= self.max_scrollback_lines {
                self.scrollback.remove(0);
            }
            self.scrollback.push(line);
            self.cells.push(vec![Cell::default(); self.cols]);
        }
    }

    pub fn get_visible_text(&self) -> String {
        let mut result = String::new();
        for row in &self.cells {
            for cell in row {
                if cell.c == '\0' || cell.c == ' ' {
                    result.push(' ');
                } else {
                    result.push(cell.c);
                }
            }
            result.push('\n');
        }
        result
    }
}