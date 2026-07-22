mod animation;
mod config;
mod render;
mod telemetry;
mod window;

use std::sync::mpsc;
use std::time::Duration;

use x11rb::protocol::xproto::{ButtonPressEvent, EnterNotifyEvent, LeaveNotifyEvent};
use x11rb::protocol::Event;

use animation::SlideAnimation;
use telemetry::{History, Telemetry};

const SCROLL_STEP: i32 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelState {
    Hidden,
    SlidingIn,
    Visible,
    SlidingOut,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    let start_visible = args.iter().any(|arg| arg == "--visible");
    let config_path = parse_config_arg(&args).unwrap_or_else(config::default_config_path);
    let config = config::load_config(&config_path);
    config.validate().map_err(|message| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid configuration {}: {message}", config_path.display()),
        )
    })?;

    let panel = window::PanelWindow::create(&config)?;
    let renderer = render::Renderer::new(config.panel_width(), panel.screen_height, &config);

    // History has one sample per pixel of graph width.
    let graph_capacity = (config.panel_width() as usize).saturating_sub(32).max(1);
    let mut history = History::new(graph_capacity);

    let (tx, rx) = mpsc::channel::<Telemetry>();
    let telemetry_config = config.clone();
    let refresh_ms = telemetry_config.refresh_interval_ms();
    std::thread::spawn(move || {
        let mut fetcher = telemetry::TelemetryFetcher::new(telemetry_config);
        loop {
            if tx.send(fetcher.fetch()).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(refresh_ms));
        }
    });

    let mut state = PanelState::Hidden;
    let mut animation: Option<SlideAnimation> = None;
    let mut panel_x = panel.hidden_x;
    let mut telemetry = Telemetry::default();
    let mut needs_redraw = true;
    let mut trigger_hovered = false;
    let mut scroll_offset = 0;
    let mut max_scroll = 0;

    if start_visible {
        state = PanelState::Visible;
        panel_x = panel.visible_x;
        panel.set_x(panel_x)?;
        panel.raise_panel()?;
    } else {
        panel.set_x(panel_x)?;
    }

    loop {
        while let Some(event) = panel.poll_event()? {
            match event {
                Event::EnterNotify(EnterNotifyEvent { event: win, .. })
                    if win == panel.trigger_win =>
                {
                    trigger_hovered = true;
                    if matches!(state, PanelState::Hidden | PanelState::SlidingOut) {
                        state = PanelState::SlidingIn;
                        animation = Some(SlideAnimation::new(
                            panel_x as f64,
                            panel.visible_x as f64,
                            config.animation_duration_ms(),
                        ));
                        panel.raise_panel()?;
                        needs_redraw = true;
                    }
                }
                Event::LeaveNotify(LeaveNotifyEvent { event: win, .. })
                    if win == panel.trigger_win =>
                {
                    trigger_hovered = false;
                }
                Event::LeaveNotify(LeaveNotifyEvent { event: win, .. })
                    if win == panel.panel_win
                        && matches!(state, PanelState::Visible | PanelState::SlidingIn) =>
                {
                    state = PanelState::SlidingOut;
                    animation = Some(SlideAnimation::new(
                        panel_x as f64,
                        panel.hidden_x as f64,
                        config.animation_duration_ms(),
                    ));
                }
                Event::ButtonPress(ButtonPressEvent {
                    event: win, detail, ..
                }) if win == panel.panel_win && state != PanelState::Hidden => {
                    let previous_offset = scroll_offset;
                    match detail {
                        4 => scroll_offset = (scroll_offset - SCROLL_STEP).max(0),
                        5 => scroll_offset = (scroll_offset + SCROLL_STEP).min(max_scroll),
                        _ => {}
                    }
                    needs_redraw |= scroll_offset != previous_offset;
                }
                Event::Expose(_)
                    if matches!(state, PanelState::Visible | PanelState::SlidingIn) =>
                {
                    needs_redraw = true;
                }
                _ => {}
            }
        }

        if let Some(anim) = &animation {
            panel_x = anim.current_value().round() as i32;
            panel.set_x(panel_x)?;
            if anim.is_finished() {
                animation = None;
                match state {
                    PanelState::SlidingIn => {
                        state = PanelState::Visible;
                        panel_x = panel.visible_x;
                        panel.set_x(panel_x)?;
                    }
                    PanelState::SlidingOut if trigger_hovered => {
                        state = PanelState::SlidingIn;
                        animation = Some(SlideAnimation::new(
                            panel_x as f64,
                            panel.visible_x as f64,
                            config.animation_duration_ms(),
                        ));
                    }
                    PanelState::SlidingOut => {
                        state = PanelState::Hidden;
                        panel_x = panel.hidden_x;
                        panel.set_x(panel_x)?;
                    }
                    _ => {}
                }
            }
        }

        if let Ok(new_telemetry) = rx.try_recv() {
            if let Some(system) = &new_telemetry.system {
                history.push(system);
            }
            telemetry = new_telemetry;
            if state != PanelState::Hidden {
                needs_redraw = true;
            }
        }

        if needs_redraw && state != PanelState::Hidden {
            let mut frame = renderer.render(&telemetry, &history, scroll_offset);
            max_scroll = (frame.content_height - i32::from(panel.screen_height)).max(0);
            let clamped_offset = scroll_offset.min(max_scroll);
            if clamped_offset != scroll_offset {
                scroll_offset = clamped_offset;
                frame = renderer.render(&telemetry, &history, scroll_offset);
            }
            panel.put_image(&frame.data)?;
            needs_redraw = false;
        }

        let sleep_ms = if animation.is_some() { 8 } else { 50 };
        std::thread::sleep(Duration::from_millis(sleep_ms));
    }
}

fn print_help() {
    println!("mon-panel — telemetry edge panel for LLM servers\n");
    println!("Usage: mon-panel [OPTIONS]\n");
    println!("Options:");
    println!("  --config <PATH>   Path to config file (default: ~/.config/mon-panel/config.toml)");
    println!("  --visible         Start with panel visible (for debugging)");
    println!("  --help            Show this help and exit\n");
    println!("Controls:");
    println!("  Mouse wheel       Scroll overflowing panel content\n");
    println!("Config:");
    println!("  See config.example.toml for all options with descriptions.");
}

fn parse_config_arg(args: &[String]) -> Option<std::path::PathBuf> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--config" {
            return iter.next().map(std::path::PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix("--config=") {
            return Some(std::path::PathBuf::from(path));
        }
    }
    None
}
