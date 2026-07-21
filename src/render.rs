use cairo::{Context, Format, ImageSurface};
use pango::{FontDescription, Weight};
use pangocairo::functions;

use crate::config::{Color, Config};
use crate::telemetry::{ModelInfo, Telemetry};

const PADDING: f64 = 16.0;
const TITLE_SIZE: i32 = 16;
const BODY_SIZE: i32 = 13;
const SECTION_SIZE: i32 = 11;
const BAR_HEIGHT: f64 = 6.0;
const BAR_GAP: f64 = 8.0;
const SECTION_GAP: f64 = 14.0;
const DOT_RADIUS: f64 = 4.0;
const PANGO_SCALE: i32 = 1024;

pub struct Renderer {
    width: i32,
    height: i32,
    font_family: String,
    config: Config,
}

impl Renderer {
    pub fn new(width: u16, height: u16, config: &Config) -> Self {
        Self {
            width: width as i32,
            height: height as i32,
            font_family: config.font_family.clone(),
            config: config.clone(),
        }
    }

    pub fn render(&self, telemetry: &Telemetry) -> Vec<u8> {
        let mut surface =
            ImageSurface::create(Format::ARgb32, self.width, self.height).expect("surface");

        {
            let cr = Context::new(&surface).expect("context");

            // Background
            let bg = &self.config.bg_color;
            cr.set_operator(cairo::Operator::Source);
            cr.set_source_rgba(bg.r, bg.g, bg.b, bg.a);
            cr.paint().expect("paint");
            cr.set_operator(cairo::Operator::Over);

            let mut y = PADDING;

            // Title
            let title = telemetry.system_name.as_deref().unwrap_or("LLM Edge Panel");
            y = self.draw_text(&cr, title, PADDING, y, TITLE_SIZE, true, &self.config.fg_color);
            y += 6.0;

            // Separator
            self.draw_separator(&cr, y);
            y += SECTION_GAP;

            // Models
            if !telemetry.models.is_empty() {
                y = self.draw_section(&cr, "MODELS", y);
                for model in &telemetry.models {
                    y = self.draw_model(&cr, model, y);
                }
                y += SECTION_GAP;
            }

            // System
            if let Some(sys) = &telemetry.system {
                y = self.draw_section(&cr, "SYSTEM", y);
                y = self.draw_bar(&cr, "CPU", sys.cpu_percent as f64, 100.0, "%", y, &self.config.accent_color);
                let mem_pct = if sys.memory_total_gb > 0.0 {
                    sys.memory_used_gb / sys.memory_total_gb * 100.0
                } else {
                    0.0
                };
                y = self.draw_bar(&cr, "RAM", mem_pct, 100.0, "%", y, &self.config.accent_color);
                if sys.disk_pct > 0.0 {
                    y = self.draw_bar(&cr, "Disk", sys.disk_pct, 100.0, "%", y, &self.config.accent_color);
                }
                if sys.swap_pct > 0.0 {
                    y = self.draw_bar(&cr, "Swap", sys.swap_pct, 100.0, "%", y, &self.config.accent_color);
                }
                if sys.load_avg > 0.0 {
                    y = self.draw_text(&cr, &format!("Load  {:.2}", sys.load_avg), PADDING, y, BODY_SIZE, false, &self.config.fg_color);
                }
                y += SECTION_GAP;
            }

            // GPU
            if let Some(sys) = &telemetry.system {
                if sys.gpu_percent > 0.0 || sys.gpu_temp_c > 0.0 || sys.gpu_memory_total_gb > 0.0 {
                    y = self.draw_section(&cr, "GPU", y);
                    y = self.draw_bar(&cr, "Util", sys.gpu_percent as f64, 100.0, "%", y, &self.config.accent_color);

                    let vram_pct = if sys.gpu_memory_total_gb > 0.0 {
                        sys.gpu_memory_used_gb / sys.gpu_memory_total_gb * 100.0
                    } else {
                        0.0
                    };
                    let vram_val = format!("{:.1}/{:.0}G", sys.gpu_memory_used_gb, sys.gpu_memory_total_gb);
                    y = self.draw_bar(&cr, "VRAM", vram_pct, 100.0, &vram_val, y, &self.config.accent_color);

                    let temp_color = if sys.gpu_temp_c > 80.0 {
                        &self.config.warn_color
                    } else {
                        &self.config.fg_color
                    };
                    y = self.draw_text(&cr, &format!("Temp  {:.0}°C", sys.gpu_temp_c), PADDING, y, BODY_SIZE, false, temp_color);
                }
            }

            // No data
            if telemetry.models.is_empty() && telemetry.system.is_none() {
                self.draw_text(&cr, "Waiting for data...", PADDING, y, BODY_SIZE, false, &self.config.dim_color);
            }
        }

        self.extract_data(&mut surface)
    }

    fn make_font(&self, size: i32, bold: bool) -> FontDescription {
        let mut desc = FontDescription::from_string(&self.font_family);
        desc.set_size(size * PANGO_SCALE);
        if bold {
            desc.set_weight(Weight::Bold);
        }
        desc
    }

    fn draw_text(
        &self,
        cr: &Context,
        text: &str,
        x: f64,
        y: f64,
        font_size: i32,
        bold: bool,
        color: &Color,
    ) -> f64 {
        let layout = functions::create_layout(cr);
        layout.set_font_description(Some(&self.make_font(font_size, bold)));
        layout.set_text(text);

        let (_, h) = layout.pixel_size();

        cr.set_source_rgba(color.r, color.g, color.b, color.a);
        cr.move_to(x, y);
        functions::show_layout(cr, &layout);

        y + h as f64 + 4.0
    }

    fn draw_text_right(
        &self,
        cr: &Context,
        text: &str,
        y: f64,
        font_size: i32,
        color: &Color,
    ) {
        let layout = functions::create_layout(cr);
        layout.set_font_description(Some(&self.make_font(font_size, false)));
        layout.set_text(text);

        let (w, _) = layout.pixel_size();

        cr.set_source_rgba(color.r, color.g, color.b, color.a);
        cr.move_to(self.width as f64 - PADDING - w as f64, y);
        functions::show_layout(cr, &layout);
    }

    fn draw_section(&self, cr: &Context, text: &str, y: f64) -> f64 {
        self.draw_text(cr, text, PADDING, y, SECTION_SIZE, true, &self.config.dim_color) + 4.0
    }

    fn draw_separator(&self, cr: &Context, y: f64) {
        let c = &self.config.dim_color;
        cr.set_source_rgba(c.r, c.g, c.b, 0.3);
        cr.set_line_width(1.0);
        cr.move_to(PADDING, y);
        cr.line_to(self.width as f64 - PADDING, y);
        cr.stroke().expect("stroke");
    }

    fn draw_model(&self, cr: &Context, model: &ModelInfo, y: f64) -> f64 {
        let dot_color = if model.loaded {
            &self.config.accent_color
        } else {
            &self.config.dim_color
        };

        // Dot
        cr.set_source_rgba(dot_color.r, dot_color.g, dot_color.b, dot_color.a);
        cr.arc(PADDING + DOT_RADIUS, y + 8.0, DOT_RADIUS, 0.0, std::f64::consts::TAU);
        cr.fill().expect("fill");

        // Name
        self.draw_text(cr, &model.name, PADDING + DOT_RADIUS * 2.0 + 8.0, y, BODY_SIZE, false, &self.config.fg_color)
    }

    fn draw_bar(
        &self,
        cr: &Context,
        label: &str,
        value: f64,
        max: f64,
        value_text: &str,
        y: f64,
        color: &Color,
    ) -> f64 {
        let bar_width = self.width as f64 - 2.0 * PADDING;

        // Label (left) + value (right)
        let text_y = y;
        self.draw_text(cr, label, PADDING, text_y, BODY_SIZE, false, &self.config.fg_color);
        self.draw_text_right(cr, value_text, text_y, BODY_SIZE, color);

        let y = text_y + BODY_SIZE as f64 + 6.0;

        // Bar background
        let bg = &self.config.bar_bg_color;
        cr.set_source_rgba(bg.r, bg.g, bg.b, bg.a);
        cr.rectangle(PADDING, y, bar_width, BAR_HEIGHT);
        cr.fill().expect("fill");

        // Bar fill
        let pct = (value / max).clamp(0.0, 1.0);
        let fill_width = bar_width * pct;
        cr.set_source_rgba(color.r, color.g, color.b, color.a);
        cr.rectangle(PADDING, y, fill_width, BAR_HEIGHT);
        cr.fill().expect("fill");

        y + BAR_HEIGHT + BAR_GAP
    }

    fn extract_data(&self, surface: &mut ImageSurface) -> Vec<u8> {
        let stride = surface.stride() as usize;
        let row_size = self.width as usize * 4;
        let height = self.height as usize;

        let data = surface.data().expect("surface data");

        if stride == row_size {
            data.to_vec()
        } else {
            let mut packed = Vec::with_capacity(row_size * height);
            for row in 0..height {
                let start = row * stride;
                packed.extend_from_slice(&data[start..start + row_size]);
            }
            packed
        }
    }
}