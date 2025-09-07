use unicode_width::UnicodeWidthChar;
use crate::scrollback::ScrollbackBuffer;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
    pub const RED: Color = Color { r: 205, g: 49, b: 49 };
    pub const GREEN: Color = Color { r: 13, g: 188, b: 121 };
    pub const YELLOW: Color = Color { r: 229, g: 229, b: 16 };
    pub const BLUE: Color = Color { r: 36, g: 114, b: 200 };
    pub const MAGENTA: Color = Color { r: 188, g: 63, b: 188 };
    pub const CYAN: Color = Color { r: 17, g: 168, b: 205 };
    pub const WHITE: Color = Color { r: 229, g: 229, b: 229 };
    
    // Bright colors
    pub const BRIGHT_BLACK: Color = Color { r: 102, g: 102, b: 102 };
    pub const BRIGHT_RED: Color = Color { r: 241, g: 76, b: 76 };
    pub const BRIGHT_GREEN: Color = Color { r: 35, g: 209, b: 139 };
    pub const BRIGHT_YELLOW: Color = Color { r: 245, g: 245, b: 67 };
    pub const BRIGHT_BLUE: Color = Color { r: 59, g: 142, b: 234 };
    pub const BRIGHT_MAGENTA: Color = Color { r: 214, g: 112, b: 214 };
    pub const BRIGHT_CYAN: Color = Color { r: 41, g: 184, b: 219 };
    pub const BRIGHT_WHITE: Color = Color { r: 255, g: 255, b: 255 };
    
    pub fn from_ansi(n: u8) -> Color {
        match n {
            0 => Color::BLACK,
            1 => Color::RED,
            2 => Color::GREEN,
            3 => Color::YELLOW,
            4 => Color::BLUE,
            5 => Color::MAGENTA,
            6 => Color::CYAN,
            7 => Color::WHITE,
            8 => Color::BRIGHT_BLACK,
            9 => Color::BRIGHT_RED,
            10 => Color::BRIGHT_GREEN,
            11 => Color::BRIGHT_YELLOW,
            12 => Color::BRIGHT_BLUE,
            13 => Color::BRIGHT_MAGENTA,
            14 => Color::BRIGHT_CYAN,
            15 => Color::BRIGHT_WHITE,
            // 256 color palette
            16..=231 => {
                // 6x6x6 color cube
                let idx = n - 16;
                let r = (idx / 36) * 51;
                let g = ((idx / 6) % 6) * 51;
                let b = (idx % 6) * 51;
                Color { r, g, b }
            }
            // Grayscale (232..=255 covers all remaining values)
            _ => {
                // Grayscale
                let gray = 8 + (n - 232) * 10;
                Color { r: gray, g: gray, b: gray }
            }
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::WHITE
    }
}

#[derive(Clone, Copy, Default)]
pub struct Cell { 
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Cell>,
    pub x: usize,
    pub y: usize,
    pub scrollback: ScrollbackBuffer,
    // Current text attributes
    pub current_fg: Color,
    pub current_bg: Color,
    pub current_bold: bool,
    pub current_italic: bool,
    pub current_underline: bool,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self { 
            cols, 
            rows, 
            cells: vec![Cell::default(); cols * rows], 
            x: 0, 
            y: 0,
            scrollback: ScrollbackBuffer::new(10000), // 10k lines of scrollback
            current_fg: Color::default(),
            current_bg: Color::BLACK,
            current_bold: false,
            current_italic: false,
            current_underline: false,
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
    
    pub fn clear_line(&mut self, row: usize) {
        let row = row.min(self.rows.saturating_sub(1));
        let start = row * self.cols;
        let end = start + self.cols;
        for c in &mut self.cells[start..end] { 
            *c = Cell::default(); 
        }
    }
    
    pub fn clear_eol_from_cursor(&mut self) {
        let row = self.y.min(self.rows.saturating_sub(1));
        let start = row * self.cols + self.x.min(self.cols.saturating_sub(1));
        let end = row * self.cols + self.cols;
        for c in &mut self.cells[start..end] { 
            *c = Cell::default(); 
        }
    }
    
    pub fn clear_bol_to_cursor(&mut self) {
        let row = self.y.min(self.rows.saturating_sub(1));
        let start = row * self.cols;
        let end = row * self.cols + self.x.min(self.cols.saturating_sub(1)) + 1;
        for c in &mut self.cells[start..end] { 
            *c = Cell::default(); 
        }
    }
    
    pub fn put(&mut self, ch: char) {
        let w = UnicodeWidthChar::width(ch).unwrap_or(1).max(1).min(2);
        if self.x >= self.cols { 
            self.wrap(); 
        }
        let idx = self.y * self.cols + self.x;
        self.cells[idx].ch = ch;
        self.cells[idx].fg = self.current_fg;
        self.cells[idx].bg = self.current_bg;
        self.cells[idx].bold = self.current_bold;
        self.cells[idx].italic = self.current_italic;
        self.cells[idx].underline = self.current_underline;
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
            // Save the top line to scrollback before scrolling
            let mut line = Vec::with_capacity(self.cols);
            for c in 0..self.cols {
                line.push(self.cells[c]);
            }
            self.scrollback.push_line(line);
            
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
    
    /// Get display content including scrollback if scrolled
    pub fn get_display_content(&self) -> String {
        if self.scrollback.scroll_offset > 0 {
            // We're scrolled - show scrollback content
            let scrollback_lines = self.scrollback.get_visible_lines(self.rows);
            let mut s = String::new();
            
            for line in scrollback_lines {
                for cell in line {
                    s.push(if cell.ch == '\0' { ' ' } else { cell.ch });
                }
                s.push('\n');
            }
            
            // If we have fewer scrollback lines than viewport, show current grid too
            let remaining_rows = self.rows.saturating_sub(self.scrollback.len());
            if remaining_rows > 0 && self.scrollback.scroll_offset < self.scrollback.len() {
                for r in 0..remaining_rows.min(self.rows) {
                    for c in 0..self.cols {
                        let ch = self.cells[self.idx(c, r)].ch;
                        s.push(if ch == '\0' { ' ' } else { ch });
                    }
                    s.push('\n');
                }
            }
            
            s
        } else {
            // Normal view - show current grid
            self.to_string_lines()
        }
    }
    
    /// Scroll up in the scrollback
    pub fn scroll_up(&mut self, lines: usize) {
        self.scrollback.scroll_up(lines);
    }
    
    /// Scroll down in the scrollback
    pub fn scroll_down(&mut self, lines: usize) {
        self.scrollback.scroll_down(lines);
    }
    
    /// Page up
    pub fn page_up(&mut self) {
        self.scrollback.page_up(self.rows);
    }
    
    /// Page down
    pub fn page_down(&mut self) {
        self.scrollback.page_down(self.rows);
    }
    
    /// Check if we're viewing scrollback
    pub fn is_scrolled(&self) -> bool {
        self.scrollback.scroll_offset > 0
    }
    
    /// Jump to bottom (exit scrollback view)
    pub fn scroll_to_bottom(&mut self) {
        self.scrollback.scroll_to_bottom();
    }
}