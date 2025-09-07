use std::collections::VecDeque;
use crate::grid::Cell;

/// Efficient scrollback buffer with configurable history size
pub struct ScrollbackBuffer {
    /// Stored lines in the scrollback (older lines)
    lines: VecDeque<Vec<Cell>>,
    /// Maximum number of lines to store
    max_lines: usize,
    /// Current scroll offset (0 = viewing latest, >0 = scrolled up)
    pub scroll_offset: usize,
}

impl ScrollbackBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max_lines),
            max_lines,
            scroll_offset: 0,
        }
    }
    
    /// Push a line to the scrollback buffer
    pub fn push_line(&mut self, line: Vec<Cell>) {
        // If at capacity, remove oldest line
        if self.lines.len() >= self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
        
        // Auto-scroll to bottom when new content arrives (unless user is scrolling)
        if self.scroll_offset > 0 {
            self.scroll_offset += 1;
        }
    }
    
    /// Get lines for display (from scroll position)
    pub fn get_visible_lines(&self, viewport_height: usize) -> Vec<Vec<Cell>> {
        let total_lines = self.lines.len();
        
        if total_lines == 0 {
            return vec![];
        }
        
        // Calculate the starting line based on scroll offset
        let start = if self.scroll_offset >= total_lines {
            0
        } else {
            total_lines - self.scroll_offset - viewport_height.min(total_lines - self.scroll_offset)
        };
        
        let end = (start + viewport_height).min(total_lines);
        
        self.lines
            .range(start..end)
            .map(|line| line.clone())
            .collect()
    }
    
    /// Scroll up by n lines
    pub fn scroll_up(&mut self, n: usize) {
        let max_scroll = self.lines.len();
        self.scroll_offset = (self.scroll_offset + n).min(max_scroll);
    }
    
    /// Scroll down by n lines
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }
    
    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = self.lines.len();
    }
    
    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }
    
    /// Page up (scroll by viewport height)
    pub fn page_up(&mut self, viewport_height: usize) {
        self.scroll_up(viewport_height);
    }
    
    /// Page down (scroll by viewport height)
    pub fn page_down(&mut self, viewport_height: usize) {
        self.scroll_down(viewport_height);
    }
    
    /// Check if we're at the bottom
    pub fn is_at_bottom(&self) -> bool {
        self.scroll_offset == 0
    }
    
    /// Clear scrollback buffer
    pub fn clear(&mut self) {
        self.lines.clear();
        self.scroll_offset = 0;
    }
    
    /// Get total number of lines in scrollback
    pub fn len(&self) -> usize {
        self.lines.len()
    }
    
    /// Search for text in scrollback
    pub fn search(&self, query: &str, case_sensitive: bool) -> Vec<(usize, usize, usize)> {
        let mut matches = Vec::new();
        let query_lower = if !case_sensitive { 
            query.to_lowercase() 
        } else { 
            query.to_string() 
        };
        
        for (line_idx, line) in self.lines.iter().enumerate() {
            let line_text: String = line.iter()
                .map(|cell| if cell.ch == '\0' { ' ' } else { cell.ch })
                .collect();
            
            let search_text = if !case_sensitive {
                line_text.to_lowercase()
            } else {
                line_text.clone()
            };
            
            // Find all matches in this line
            let mut start = 0;
            while let Some(pos) = search_text[start..].find(&query_lower) {
                let match_start = start + pos;
                let match_end = match_start + query.len();
                matches.push((line_idx, match_start, match_end));
                start = match_start + 1;
            }
        }
        
        matches
    }
}