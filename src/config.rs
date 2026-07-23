use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[allow(dead_code)]
pub enum Side {
    Left,
    #[default]
    Right,
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
    /// How many telemetry refreshes elapse before a new data point is added to
    /// the graphs. 1 = update graphs on every telemetry refresh (default).
    #[serde(default = "default_graph_update_interval")]
    pub graph_update_interval: u32,
    #[serde(default = "default_llama_url")]
    pub llama_swap_url: String,
    #[serde(default)]
    pub llama_swap_api_key: Option<String>,
    #[serde(default)]
    pub telemetry_url: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct DisplaySection {
    #[serde(default = "default_font")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
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
    // Per-graph colors (fall back to accent / fg if not set)
    #[serde(default)]
    pub cpu_util: Option<Color>,
    #[serde(default)]
    pub cpu_temp: Option<Color>,
    #[serde(default)]
    pub ram: Option<Color>,
    #[serde(default)]
    pub gpu_util: Option<Color>,
    #[serde(default)]
    pub gpu_vram: Option<Color>,
    #[serde(default)]
    pub gpu_temp: Option<Color>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ThresholdsSection {
    #[serde(default = "default_temp_warn")]
    pub cpu_temp_warn: f32,
    #[serde(default = "default_temp_warn")]
    pub gpu_temp_warn: f32,
}

fn default_temp_warn() -> f32 {
    80.0
}

#[derive(Deserialize, Clone, Debug, Default)]
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
    #[serde(default)]
    pub thresholds: ThresholdsSection,
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
    pub fn graph_update_interval(&self) -> u32 {
        self.telemetry.graph_update_interval
    }
    pub fn llama_swap_url(&self) -> &str {
        &self.telemetry.llama_swap_url
    }
    pub fn llama_swap_api_key(&self) -> Option<&str> {
        self.telemetry.llama_swap_api_key.as_deref()
    }
    pub fn telemetry_url(&self) -> Option<&str> {
        self.telemetry.telemetry_url.as_deref()
    }
    pub fn font_family(&self) -> &str {
        &self.display.font_family
    }
    pub fn font_size(&self) -> f32 {
        self.display.font_size
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.panel.width < 33 {
            return Err("panel.width must be at least 33 pixels".to_string());
        }
        if self.panel.trigger_width == 0 {
            return Err("panel.trigger_width must be at least 1 pixel".to_string());
        }
        if self.telemetry.refresh_interval_ms == 0 {
            return Err("telemetry.refresh_interval_ms must be greater than zero".to_string());
        }
        if self.telemetry.graph_update_interval == 0 {
            return Err("telemetry.graph_update_interval must be at least 1".to_string());
        }
        if !self.display.font_size.is_finite() || self.display.font_size <= 0.0 {
            return Err("display.font_size must be a positive finite number".to_string());
        }
        for (name, color) in [
            ("colors.bg", self.colors.bg),
            ("colors.fg", self.colors.fg),
            ("colors.accent", self.colors.accent),
            ("colors.warn", self.colors.warn),
            ("colors.dim", self.colors.dim),
            ("colors.bar_bg", self.colors.bar_bg),
        ] {
            if ![color.r, color.g, color.b, color.a]
                .iter()
                .all(|component| component.is_finite() && (0.0..=1.0).contains(component))
            {
                return Err(format!(
                    "{name} components must be finite values from 0.0 to 1.0"
                ));
            }
        }
        for (name, color) in [
            ("colors.cpu_util", self.colors.cpu_util),
            ("colors.cpu_temp", self.colors.cpu_temp),
            ("colors.ram", self.colors.ram),
            ("colors.gpu_util", self.colors.gpu_util),
            ("colors.gpu_vram", self.colors.gpu_vram),
            ("colors.gpu_temp", self.colors.gpu_temp),
        ] {
            if let Some(color) = color {
                if ![color.r, color.g, color.b, color.a]
                    .iter()
                    .all(|component| component.is_finite() && (0.0..=1.0).contains(component))
                {
                    return Err(format!(
                        "{name} components must be finite values from 0.0 to 1.0"
                    ));
                }
            }
        }
        if !self.thresholds.cpu_temp_warn.is_finite() || self.thresholds.cpu_temp_warn < 0.0 {
            return Err(
                "thresholds.cpu_temp_warn must be a non-negative finite number".to_string(),
            );
        }
        if !self.thresholds.gpu_temp_warn.is_finite() || self.thresholds.gpu_temp_warn < 0.0 {
            return Err(
                "thresholds.gpu_temp_warn must be a non-negative finite number".to_string(),
            );
        }
        if let Some(beszel) = &self.beszel {
            for (name, value) in [
                ("beszel.hub_url", &beszel.hub_url),
                ("beszel.email", &beszel.email),
                ("beszel.password", &beszel.password),
                ("beszel.system_id", &beszel.system_id),
            ] {
                if value.trim().is_empty() {
                    return Err(format!(
                        "{name} must not be empty when [beszel] is configured"
                    ));
                }
            }
        }
        Ok(())
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

    // Per-graph colors — fall back to accent (for utilisation) or fg (for temp)
    pub fn cpu_util_color(&self) -> Color {
        self.colors.cpu_util.unwrap_or(self.colors.accent)
    }
    pub fn cpu_temp_color(&self) -> Color {
        self.colors.cpu_temp.unwrap_or(self.colors.fg)
    }
    pub fn ram_color(&self) -> Color {
        self.colors.ram.unwrap_or(self.colors.accent)
    }
    pub fn gpu_util_color(&self) -> Color {
        self.colors.gpu_util.unwrap_or(self.colors.accent)
    }
    pub fn gpu_vram_color(&self) -> Color {
        self.colors.gpu_vram.unwrap_or(self.colors.accent)
    }
    pub fn gpu_temp_color(&self) -> Color {
        self.colors.gpu_temp.unwrap_or(self.colors.fg)
    }
    pub fn cpu_temp_warn(&self) -> f32 {
        self.thresholds.cpu_temp_warn
    }
    pub fn gpu_temp_warn(&self) -> f32 {
        self.thresholds.gpu_temp_warn
    }
}

// --- Default functions ---

fn default_panel_width() -> u16 {
    260
}
fn default_anim_ms() -> u64 {
    200
}
fn default_trigger_width() -> u16 {
    3
}
fn default_llama_url() -> String {
    "http://localhost:8080".to_string()
}
fn default_refresh_ms() -> u64 {
    10000
}
fn default_graph_update_interval() -> u32 {
    1
}
fn default_font() -> String {
    "Sans".to_string()
}
fn default_font_size() -> f32 {
    13.0
}
fn default_bg_color() -> Color {
    Color::new(0.08, 0.08, 0.10, 0.95)
}
fn default_fg_color() -> Color {
    Color::new(0.9, 0.9, 0.92, 1.0)
}
fn default_accent_color() -> Color {
    Color::new(0.3, 0.7, 1.0, 1.0)
}
fn default_warn_color() -> Color {
    Color::new(1.0, 0.6, 0.3, 1.0)
}
fn default_dim_color() -> Color {
    Color::new(0.5, 0.5, 0.55, 1.0)
}
fn default_bar_bg_color() -> Color {
    Color::new(0.2, 0.2, 0.25, 1.0)
}

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
            graph_update_interval: default_graph_update_interval(),
            llama_swap_url: default_llama_url(),
            llama_swap_api_key: None,
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
            cpu_util: None,
            cpu_temp: None,
            ram: None,
            gpu_util: None,
            gpu_vram: None,
            gpu_temp: None,
        }
    }
}

impl Default for ThresholdsSection {
    fn default() -> Self {
        Self {
            cpu_temp_warn: default_temp_warn(),
            gpu_temp_warn: default_temp_warn(),
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
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("[config] {} not found, using defaults", path.display());
            Config::default()
        }
        Err(error) => {
            eprintln!(
                "[config] failed to read {}: {error}; using defaults",
                path.display()
            );
            Config::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn defaults_are_valid() {
        assert!(Config::default().validate().is_ok());
    }

    #[test]
    fn rejects_zero_graph_update_interval() {
        let mut config = Config::default();
        config.telemetry.graph_update_interval = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_a_panel_that_cannot_render_graphs() {
        let mut config = Config::default();
        config.panel.width = 32;
        assert!(config.validate().is_err());
    }
}
