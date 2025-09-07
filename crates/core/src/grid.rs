use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Default)]
pub struct Cell { 
    pub ch: char 
}

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Cell>,
    pub x: usize,
    pub y: usize,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self { 
            cols, 
            rows, 
            cells: vec![Cell::default(); cols * rows], 
            x: 0, 
            y: 0 
        }
    }
    
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols; 
        self.rows = rows;
        self.cells.resize(cols * rows, Cell::default());
        self.clear_all();
        self.x = 0; 
        self.y = 0;
    }
    
    pub fn resize_preserve(&mut self, new_cols: usize, new_rows: usize) {
        if new_cols == self.cols && new_rows == self.rows { 
            return; 
        }

        let old_cols = self.cols;
        let old_rows = self.rows;
        let old_cells = std::mem::take(&mut self.cells);

        self.cols = new_cols;
        self.rows = new_rows;
        self.cells = vec![Cell::default(); new_cols * new_rows];

        let keep_rows = old_rows.min(new_rows);
        let keep_cols = old_cols.min(new_cols);

        // Copy overlapping area, bottom-aligned like real terminals
        for r in 0..keep_rows {
            let src_r = old_rows - keep_rows + r;
            let dst_r = new_rows - keep_rows + r;

            // Copy only the overlapping width (left aligned)
            for c in 0..keep_cols {
                let src_idx = src_r * old_cols + c;
                let dst_idx = dst_r * new_cols + c;
                self.cells[dst_idx] = old_cells[src_idx];
            }
            // Remaining columns (if any) are already spaces
        }

        // Clamp cursor into bounds, don't reset it
        if self.y >= self.rows { 
            self.y = self.rows.saturating_sub(1); 
        }
        if self.x >= self.cols { 
            self.x = self.cols.saturating_sub(1); 
        }
    }
    
    fn idx(&self, x: usize, y: usize) -> usize { 
        y * self.cols + x 
    }
    
    pub fn clear_all(&mut self) { 
        for c in &mut self.cells { 
            *c = Cell::default(); 
        } 
    }
    
    pub fn clear_eol(&mut self) {
        let start = self.idx(self.x, self.y);
        let end = self.idx(self.cols - 1, self.y) + 1;
        for i in start..end { 
            self.cells[i] = Cell::default(); 
        }
    }
    
    pub fn put(&mut self, ch: char) {
        let w = UnicodeWidthChar::width(ch).unwrap_or(1).max(1).min(2);
        if self.x >= self.cols { 
            self.wrap(); 
        }
        let idx = self.y * self.cols + self.x;
        self.cells[idx].ch = ch;
        self.x = (self.x + w).min(self.cols.saturating_sub(1));
    }
    
    pub fn wrap(&mut self) { 
        self.cr(); 
        self.lf(); 
    }
    
    pub fn cr(&mut self) { 
        self.x = 0; 
    }
    
    pub fn lf(&mut self) {
        if self.y + 1 < self.rows { 
            self.y += 1; 
        } else {
            // scroll up by 1
            let cols = self.cols;
            self.cells.rotate_left(cols);
            let start = (self.rows - 1) * self.cols;
            for i in start..self.cells.len() { 
                self.cells[i] = Cell::default(); 
            }
        }
    }
    
    pub fn to_string_lines(&self) -> String {
        let mut s = String::with_capacity(self.rows * (self.cols + 1));
        for r in 0..self.rows {
            for c in 0..self.cols { 
                let ch = self.cells[self.idx(c, r)].ch;
                s.push(if ch == '\0' { ' ' } else { ch });
            }
            s.push('\n');
        }
        s
    }
    
    pub fn get_text_in_region(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> String {
        let mut s = String::new();
        for row in y0..=y1 {
            for col in x0..=x1 {
                let idx = self.idx(col.min(self.cols-1), row.min(self.rows-1));
                let ch = self.cells[idx].ch;
                s.push(if ch == '\0' { ' ' } else { ch });
            }
            if row < y1 {
                s.push('\n');
            }
        }
        s
    }
    
    pub fn selection_bounds(&self, start: (usize, usize), end: (usize, usize)) -> (usize, usize, usize, usize) {
        let (x0, y0) = start;
        let (x1, y1) = end;
        let minx = x0.min(x1);
        let maxx = x0.max(x1);
        let miny = y0.min(y1);
        let maxy = y0.max(y1);
        (minx, miny, maxx, maxy)
    }
}