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
}