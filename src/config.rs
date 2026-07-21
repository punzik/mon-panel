use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub enum Side {
    Left,
    Right,
}

impl Default for Side {
    fn default() -> Self {
        Side::Right
    }
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Color {
    #[serde(default)]
    pub r: f64,
    #[serde(default)]
    pub g: f64,
    #[serde(default)]
    pub b: f64,
    #[serde(default = "default_alpha")]
    pub a: f64,
}

fn default_alpha() -> f64 {
    1.0
}

impl Color {
    pub const fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct BeszelConfig {
    pub hub_url: String,
    pub email: String,
    pub password: String,
    pub system_id: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PanelSection {
    #[serde(default = "default_panel_width")]
    pub width: u16,
    #[serde(default)]
    pub side: Side,
    #[serde(default = "default_anim_ms")]
    pub animation_duration_ms: u64,
    #[serde(default = "default_trigger_width")]
    pub trigger_width: u16,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TelemetrySection {
    #[serde(default = "default_refresh_ms")]
    pub refresh_interval_ms: u64,
    #[serde(default = "default_llama_url")]
    pub llama_swap_url: String,
    #[serde(default)]
    pub telemetry_url: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct DisplaySection {
    #[serde(default = "default_font")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    #[allow(dead_code)]
    pub font_size: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ColorsSection {
    #[serde(default = "default_bg_color")]
    pub bg: Color,
    #[serde(default = "default_fg_color")]
    pub fg: Color,
    #[serde(default = "default_accent_color")]
    pub accent: Color,
    #[serde(default = "default_warn_color")]
    pub warn: Color,
    #[serde(default = "default_dim_color")]
    pub dim: Color,
    #[serde(default = "default_bar_bg_color")]
    pub bar_bg: Color,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default)]
    pub panel: PanelSection,
    #[serde(default)]
    pub telemetry: TelemetrySection,
    #[serde(default)]
    pub beszel: Option<BeszelConfig>,
    #[serde(default)]
    pub display: DisplaySection,
    #[serde(default)]
    pub colors: ColorsSection,
}

// --- Flattened view for internal use ---

impl Config {
    pub fn panel_width(&self) -> u16 {
        self.panel.width
    }
    pub fn side(&self) -> Side {
        self.panel.side
    }
    pub fn animation_duration_ms(&self) -> u64 {
        self.panel.animation_duration_ms
    }
    pub fn trigger_width(&self) -> u16 {
        self.panel.trigger_width
    }
    pub fn refresh_interval_ms(&self) -> u64 {
        self.telemetry.refresh_interval_ms
    }
    pub fn llama_swap_url(&self) -> &str {
        &self.telemetry.llama_swap_url
    }
    pub fn telemetry_url(&self) -> Option<&str> {
        self.telemetry.telemetry_url.as_deref()
    }
    pub fn font_family(&self) -> &str {
        &self.display.font_family
    }
    pub fn bg_color(&self) -> Color {
        self.colors.bg
    }
    pub fn fg_color(&self) -> Color {
        self.colors.fg
    }
    pub fn accent_color(&self) -> Color {
        self.colors.accent
    }
    pub fn warn_color(&self) -> Color {
        self.colors.warn
    }
    pub fn dim_color(&self) -> Color {
        self.colors.dim
    }
    pub fn bar_bg_color(&self) -> Color {
        self.colors.bar_bg
    }
}

// --- Default functions ---

fn default_panel_width() -> u16 { 260 }
fn default_anim_ms() -> u64 { 200 }
fn default_trigger_width() -> u16 { 3 }
fn default_llama_url() -> String { "http://localhost:8080".to_string() }
fn default_refresh_ms() -> u64 { 10000 }
fn default_font() -> String { "Sans".to_string() }
fn default_font_size() -> f32 { 13.0 }
fn default_bg_color() -> Color { Color::new(0.08, 0.08, 0.10, 0.95) }
fn default_fg_color() -> Color { Color::new(0.9, 0.9, 0.92, 1.0) }
fn default_accent_color() -> Color { Color::new(0.3, 0.7, 1.0, 1.0) }
fn default_warn_color() -> Color { Color::new(1.0, 0.6, 0.3, 1.0) }
fn default_dim_color() -> Color { Color::new(0.5, 0.5, 0.55, 1.0) }
fn default_bar_bg_color() -> Color { Color::new(0.2, 0.2, 0.25, 1.0) }

// --- Default impls for sections ---

impl Default for PanelSection {
    fn default() -> Self {
        Self {
            width: default_panel_width(),
            side: Side::default(),
            animation_duration_ms: default_anim_ms(),
            trigger_width: default_trigger_width(),
        }
    }
}

impl Default for TelemetrySection {
    fn default() -> Self {
        Self {
            refresh_interval_ms: default_refresh_ms(),
            llama_swap_url: default_llama_url(),
            telemetry_url: None,
        }
    }
}

impl Default for DisplaySection {
    fn default() -> Self {
        Self {
            font_family: default_font(),
            font_size: default_font_size(),
        }
    }
}

impl Default for ColorsSection {
    fn default() -> Self {
        Self {
            bg: default_bg_color(),
            fg: default_fg_color(),
            accent: default_accent_color(),
            warn: default_warn_color(),
            dim: default_dim_color(),
            bar_bg: default_bar_bg_color(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            panel: PanelSection::default(),
            telemetry: TelemetrySection::default(),
            beszel: None,
            display: DisplaySection::default(),
            colors: ColorsSection::default(),
        }
    }
}

// --- Config loading ---

pub fn default_config_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config/mon-panel/config.toml")
    } else {
        PathBuf::from("config.toml")
    }
}

pub fn load_config(path: &Path) -> Config {
    match std::fs::read_to_string(path) {
        Ok(content) => match toml::from_str::<Config>(&content) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("[config] parse error in {}: {e}", path.display());
                eprintln!("[config] using defaults");
                Config::default()
            }
        },
        Err(_) => {
            eprintln!("[config] {} not found, using defaults", path.display());
            Config::default()
        }
    }
}