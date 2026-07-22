use cairo::{Context, Format, ImageSurface};
use pango::{FontDescription, Weight};
use pangocairo::functions;

use crate::config::{Color, Config};
use crate::telemetry::{History, ModelInfo, Telemetry};

const PADDING: f64 = 16.0;
const TITLE_SIZE: i32 = 16;
const BODY_SIZE: i32 = 13;
const SECTION_SIZE: i32 = 11;
const GRAPH_HEIGHT: f64 = 20.0;
const GRAPH_GAP: f64 = 8.0;
const TEXT_GAP: f64 = 2.0;
const SECTION_GAP: f64 = 14.0;
const DOT_RADIUS: f64 = 4.0;
const PANGO_SCALE: f32 = 1024.0;
const MIN_SCROLL_THUMB_HEIGHT: f64 = 24.0;

pub struct RenderedFrame {
    pub data: Vec<u8>,
    pub content_height: i32,
}

/// Format large counts with k/M/B suffixes.
fn fmt_count(n: u64) -> String {
    if n >= 10_000_000_000 {
        format!("{:.0}B", n as f64 / 1_000_000_000.0)
    } else if n >= 10_000_000 {
        format!("{:.0}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.0}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub struct Renderer {
    width: i32,
    height: i32,
    font_family: String,
    font_scale: f32,
    config: Config,
}

impl Renderer {
    pub fn new(width: u16, height: u16, config: &Config) -> Self {
        Self {
            width: width.into(),
            height: height.into(),
            font_family: config.font_family().to_string(),
            font_scale: config.font_size() / BODY_SIZE as f32,
            config: config.clone(),
        }
    }

    pub fn render(
        &self,
        telemetry: &Telemetry,
        history: &History,
        scroll_offset: i32,
    ) -> RenderedFrame {
        let mut surface =
            ImageSurface::create(Format::ARgb32, self.width, self.height).expect("surface");
        let content_height;

        {
            let cr = Context::new(&surface).expect("context");
            let bg = self.config.bg_color();
            cr.set_operator(cairo::Operator::Source);
            cr.set_source_rgba(bg.r, bg.g, bg.b, bg.a);
            cr.paint().expect("paint");
            cr.set_operator(cairo::Operator::Over);

            let mut y = PADDING - f64::from(scroll_offset);

            let title = telemetry.system_name.as_deref().unwrap_or("LLM Edge Panel");
            y = self.draw_text(
                &cr,
                title,
                (PADDING, y),
                TITLE_SIZE,
                true,
                self.config.fg_color(),
            );
            y += 6.0;

            self.draw_separator(&cr, y);
            y += SECTION_GAP;

            if let Some(system) = &telemetry.system {
                y = self.draw_section(&cr, "SYSTEM", y);
                y = self.draw_graph(
                    &cr,
                    ("CPU", &format!("{:.0}%", system.cpu_percent)),
                    &history.cpu,
                    100.0,
                    y,
                    self.config.cpu_util_color(),
                );

                let temperature_color = if system.cpu_temp_c > self.config.cpu_temp_warn() {
                    self.config.warn_color()
                } else {
                    self.config.cpu_temp_color()
                };
                y = self.draw_graph(
                    &cr,
                    ("Temp", &format!("{:.0}°C", system.cpu_temp_c)),
                    &history.cpu_temp,
                    100.0,
                    y,
                    temperature_color,
                );

                let memory_percent = if system.memory_total_gb > 0.0 {
                    system.memory_used_gb / system.memory_total_gb * 100.0
                } else {
                    0.0
                };
                y = self.draw_graph(
                    &cr,
                    ("RAM", &format!("{memory_percent:.0}%")),
                    &history.ram,
                    100.0,
                    y,
                    self.config.ram_color(),
                );

                if system.disk_pct > 0.0 {
                    y = self.draw_text_line(&cr, "Disk", &format!("{:.0}%", system.disk_pct), y);
                }
                if system.swap_pct > 0.0 {
                    y = self.draw_text_line(&cr, "Swap", &format!("{:.0}%", system.swap_pct), y);
                }
                if system.load_avg > 0.0 {
                    y = self.draw_text_line(&cr, "Load", &format!("{:.2}", system.load_avg), y);
                }
                y += SECTION_GAP;
            }

            if let Some(system) = &telemetry.system {
                for (index, gpu) in system.gpus.iter().enumerate() {
                    y = self.draw_section(&cr, &format!("GPU {}", gpu.name), y);
                    if let Some(gpu_history) = history.gpus.get(index) {
                        y = self.draw_graph(
                            &cr,
                            ("Util", &format!("{:.0}%", gpu.percent)),
                            &gpu_history.util,
                            100.0,
                            y,
                            self.config.gpu_util_color(),
                        );
                        let vram_value =
                            format!("{:.1}/{:.0}G", gpu.memory_used_gb, gpu.memory_total_gb);
                        y = self.draw_graph(
                            &cr,
                            ("VRAM", &vram_value),
                            &gpu_history.vram,
                            100.0,
                            y,
                            self.config.gpu_vram_color(),
                        );
                        let temperature_color = if gpu.temp_c > self.config.gpu_temp_warn() {
                            self.config.warn_color()
                        } else {
                            self.config.gpu_temp_color()
                        };
                        y = self.draw_graph(
                            &cr,
                            ("Temp", &format!("{:.0}°C", gpu.temp_c)),
                            &gpu_history.temp,
                            100.0,
                            y,
                            temperature_color,
                        );
                    } else {
                        y = self.draw_text_line(&cr, "Util", &format!("{:.0}%", gpu.percent), y);
                        let vram_value =
                            format!("{:.1}/{:.0}G", gpu.memory_used_gb, gpu.memory_total_gb);
                        y = self.draw_text_line(&cr, "VRAM", &vram_value, y);
                        y = self.draw_text_line(&cr, "Temp", &format!("{:.0}°C", gpu.temp_c), y);
                    }
                    y += SECTION_GAP;
                }
            }

            if !telemetry.models.is_empty() {
                y = self.draw_section(&cr, "MODELS", y);
                for model in &telemetry.models {
                    y = self.draw_model(&cr, model, y);
                }
                y += SECTION_GAP;
            }

            if telemetry.models.is_empty() && telemetry.system.is_none() {
                y = self.draw_text(
                    &cr,
                    "Waiting for data...",
                    (PADDING, y),
                    BODY_SIZE,
                    false,
                    self.config.dim_color(),
                );
            }

            content_height = (y + f64::from(scroll_offset) + PADDING).ceil() as i32;
            self.draw_scrollbar(&cr, scroll_offset, content_height);
        }

        RenderedFrame {
            data: self.extract_data(&mut surface),
            content_height,
        }
    }

    fn make_font(&self, size: i32, bold: bool) -> FontDescription {
        let mut description = FontDescription::from_string(&self.font_family);
        description.set_size((size as f32 * self.font_scale * PANGO_SCALE).round() as i32);
        if bold {
            description.set_weight(Weight::Bold);
        }
        description
    }

    fn draw_text(
        &self,
        cr: &Context,
        text: &str,
        position: (f64, f64),
        font_size: i32,
        bold: bool,
        color: Color,
    ) -> f64 {
        let layout = functions::create_layout(cr);
        layout.set_font_description(Some(&self.make_font(font_size, bold)));
        layout.set_text(text);
        let (_, height) = layout.pixel_size();

        cr.set_source_rgba(color.r, color.g, color.b, color.a);
        cr.move_to(position.0, position.1);
        functions::show_layout(cr, &layout);
        position.1 + f64::from(height) + 4.0
    }

    fn draw_text_right(&self, cr: &Context, text: &str, y: f64, font_size: i32, color: Color) {
        let layout = functions::create_layout(cr);
        layout.set_font_description(Some(&self.make_font(font_size, false)));
        layout.set_text(text);
        let (width, _) = layout.pixel_size();

        cr.set_source_rgba(color.r, color.g, color.b, color.a);
        cr.move_to(f64::from(self.width) - PADDING - f64::from(width), y);
        functions::show_layout(cr, &layout);
    }

    fn draw_section(&self, cr: &Context, text: &str, y: f64) -> f64 {
        self.draw_text(
            cr,
            text,
            (PADDING, y),
            SECTION_SIZE,
            true,
            self.config.dim_color(),
        ) + 4.0
    }

    fn draw_separator(&self, cr: &Context, y: f64) {
        let color = self.config.dim_color();
        cr.set_source_rgba(color.r, color.g, color.b, 0.3);
        cr.set_line_width(1.0);
        cr.move_to(PADDING, y);
        cr.line_to(f64::from(self.width) - PADDING, y);
        cr.stroke().expect("stroke");
    }

    fn draw_model(&self, cr: &Context, model: &ModelInfo, y: f64) -> f64 {
        let dot_color = match model.state {
            crate::telemetry::ModelState::Ready if model.is_processing => self.config.warn_color(),
            crate::telemetry::ModelState::Ready => self.config.accent_color(),
            crate::telemetry::ModelState::Stopped => self.config.dim_color(),
        };
        cr.set_source_rgba(dot_color.r, dot_color.g, dot_color.b, dot_color.a);
        cr.arc(
            PADDING + DOT_RADIUS,
            y + 8.0 * f64::from(self.font_scale),
            DOT_RADIUS,
            0.0,
            std::f64::consts::TAU,
        );
        cr.fill().expect("fill");

        let text_x = PADDING + DOT_RADIUS * 2.0 + 8.0;
        let mut y = self.draw_text(
            cr,
            &model.name,
            (text_x, y),
            BODY_SIZE,
            false,
            self.config.fg_color(),
        );
        if model.state == crate::telemetry::ModelState::Ready {
            let dim = self.config.dim_color();
            y = self.draw_text(
                cr,
                &format!("Gen {:.1} tok/s", model.predicted_tokens_seconds),
                (text_x, y),
                BODY_SIZE,
                false,
                dim,
            );
            y = self.draw_text(
                cr,
                &format!("Prm {:.1} tok/s", model.prompt_tokens_seconds),
                (text_x, y),
                BODY_SIZE,
                false,
                dim,
            );
            y = self.draw_text(
                cr,
                &format!(
                    "In {}  Out {}",
                    fmt_count(model.prompt_tokens_total),
                    fmt_count(model.tokens_predicted_total)
                ),
                (text_x, y),
                BODY_SIZE,
                false,
                dim,
            );
        }
        y
    }

    fn draw_text_line(&self, cr: &Context, label: &str, value_text: &str, y: f64) -> f64 {
        let next_y = self.draw_text(
            cr,
            label,
            (PADDING, y),
            BODY_SIZE,
            false,
            self.config.fg_color(),
        );
        self.draw_text_right(cr, value_text, y, BODY_SIZE, self.config.fg_color());
        next_y + TEXT_GAP
    }

    fn draw_graph(
        &self,
        cr: &Context,
        text: (&str, &str),
        history: &[f64],
        max: f64,
        y: f64,
        color: Color,
    ) -> f64 {
        let graph_width = f64::from(self.width) - 2.0 * PADDING;
        let label_bottom = self.draw_text(
            cr,
            text.0,
            (PADDING, y),
            BODY_SIZE,
            false,
            self.config.fg_color(),
        );
        self.draw_text_right(cr, text.1, y, BODY_SIZE, color);
        let y = label_bottom + 2.0;

        let background = self.config.bar_bg_color();
        cr.set_source_rgba(background.r, background.g, background.b, background.a);
        cr.rectangle(PADDING, y, graph_width, GRAPH_HEIGHT);
        cr.fill().expect("fill");

        if history.len() >= 2 && max > 0.0 {
            let points: Vec<(f64, f64)> = history
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    let x = PADDING + (graph_width - history.len() as f64) + index as f64;
                    let normalized = (value / max).clamp(0.0, 1.0);
                    (x, y + GRAPH_HEIGHT * (1.0 - normalized))
                })
                .collect();

            cr.set_source_rgba(color.r, color.g, color.b, color.a * 0.25);
            cr.move_to(points[0].0, y + GRAPH_HEIGHT);
            for &(x, point_y) in &points {
                cr.line_to(x, point_y);
            }
            cr.line_to(points[points.len() - 1].0, y + GRAPH_HEIGHT);
            cr.close_path();
            cr.fill().expect("fill");

            cr.set_source_rgba(color.r, color.g, color.b, color.a);
            cr.set_line_width(1.0);
            for (index, &(x, point_y)) in points.iter().enumerate() {
                if index == 0 {
                    cr.move_to(x, point_y);
                } else {
                    cr.line_to(x, point_y);
                }
            }
            cr.stroke().expect("stroke");
        }

        y + GRAPH_HEIGHT + GRAPH_GAP
    }

    fn draw_scrollbar(&self, cr: &Context, scroll_offset: i32, content_height: i32) {
        if content_height <= self.height {
            return;
        }
        let track_height = f64::from(self.height) - 2.0 * PADDING;
        let thumb_height = (track_height * f64::from(self.height) / f64::from(content_height))
            .max(MIN_SCROLL_THUMB_HEIGHT)
            .min(track_height);
        let max_scroll = content_height - self.height;
        let travel = track_height - thumb_height;
        let thumb_y = PADDING + travel * f64::from(scroll_offset) / f64::from(max_scroll);
        let color = self.config.dim_color();
        cr.set_source_rgba(color.r, color.g, color.b, color.a * 0.65);
        cr.rectangle(f64::from(self.width) - 5.0, thumb_y, 3.0, thumb_height);
        cr.fill().expect("fill");
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
