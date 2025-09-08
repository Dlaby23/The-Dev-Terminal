use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub appearance: AppearanceConfig,
    pub theme: ThemeConfig,
    pub keybindings: KeybindingsConfig,
    pub performance: PerformanceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub shell: String,
    pub shell_args: Vec<String>,
    pub scrollback_lines: usize,
    pub mouse_reports: bool,
    pub clipboard_access: bool,
    pub bracketed_paste: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub window_padding: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CursorStyle {
    Block,
    Underline,
    Beam,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    pub copy: String,
    pub paste: String,
    pub search: String,
    pub new_tab: String,
    pub close_tab: String,
    pub next_tab: String,
    pub prev_tab: String,
    pub zoom_in: String,
    pub zoom_out: String,
    pub zoom_reset: String,
    pub clear_scrollback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceConfig {
    pub gpu_acceleration: bool,
    pub max_fps: u32,
    pub idle_fps: u32,
    pub cache_glyphs: bool,
    pub batch_rendering: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            appearance: AppearanceConfig::default(),
            theme: ThemeConfig::default(),
            keybindings: KeybindingsConfig::default(),
            performance: PerformanceConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string()),
            shell_args: vec![],
            scrollback_lines: 10000,
            mouse_reports: true,
            clipboard_access: true,
            bracketed_paste: true,
        }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            font_family: "SF Mono".to_string(),
            font_size: 18.0,
            line_height: 1.25,
            cursor_style: CursorStyle::Block,
            cursor_blink: false,
            window_padding: 12.0,
        }
    }
}

impl Default for CursorStyle {
    fn default() -> Self {
        CursorStyle::Block
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        // Default dark theme
        Self {
            name: "Default Dark".to_string(),
            background: "#0f0f10".to_string(),
            foreground: "#e5e5e5".to_string(),
            cursor: "#e5e5e5".to_string(),
            selection: "#3366cc44".to_string(),
            black: "#000000".to_string(),
            red: "#cd3131".to_string(),
            green: "#0dbc79".to_string(),
            yellow: "#e5e510".to_string(),
            blue: "#2472c8".to_string(),
            magenta: "#bc3fbc".to_string(),
            cyan: "#11a8cd".to_string(),
            white: "#e5e5e5".to_string(),
            bright_black: "#666666".to_string(),
            bright_red: "#f14c4c".to_string(),
            bright_green: "#23d18b".to_string(),
            bright_yellow: "#f5f543".to_string(),
            bright_blue: "#3b8eea".to_string(),
            bright_magenta: "#d670d6".to_string(),
            bright_cyan: "#29b8db".to_string(),
            bright_white: "#ffffff".to_string(),
        }
    }
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            copy: "cmd+c".to_string(),
            paste: "cmd+v".to_string(),
            search: "cmd+f".to_string(),
            new_tab: "cmd+t".to_string(),
            close_tab: "cmd+w".to_string(),
            next_tab: "cmd+shift+]".to_string(),
            prev_tab: "cmd+shift+[".to_string(),
            zoom_in: "cmd+=".to_string(),
            zoom_out: "cmd+-".to_string(),
            zoom_reset: "cmd+0".to_string(),
            clear_scrollback: "cmd+k".to_string(),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            gpu_acceleration: true,
            max_fps: 120,
            idle_fps: 30,
            cache_glyphs: true,
            batch_rendering: true,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;
        
        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&contents)?;
            Ok(config)
        } else {
            // Create default config if it doesn't exist
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }
    
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;
        
        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, contents)?;
        
        Ok(())
    }
    
    fn config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home = std::env::var("HOME")?;
        Ok(PathBuf::from(home).join(".config").join("the-dev-terminal").join("config.toml"))
    }
}