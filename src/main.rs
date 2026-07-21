mod animation;
mod config;
mod render;
mod telemetry;
mod window;

use std::sync::mpsc;
use std::time::Duration;

use x11rb::protocol::Event;
use x11rb::protocol::xproto::{EnterNotifyEvent, LeaveNotifyEvent};

use animation::SlideAnimation;
use telemetry::Telemetry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelState {
    Hidden,
    SlidingIn,
    Visible,
    SlidingOut,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI args
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }
    let start_visible = args.iter().any(|a| a == "--visible");
    let config_path = parse_config_arg(&args).unwrap_or_else(config::default_config_path);

    let config = config::load_config(&config_path);

    let panel = window::PanelWindow::create(&config)?;
    let renderer = render::Renderer::new(config.panel_width(), panel.screen_height, &config);

    // Telemetry thread
    let (tx, rx) = mpsc::channel::<Telemetry>();
    let tel_config = config.clone();
    let refresh_ms = tel_config.refresh_interval_ms();
    std::thread::spawn(move || {
        let mut fetcher = telemetry::TelemetryFetcher::new(tel_config);
        loop {
            let t = fetcher.fetch();
            let _ = tx.send(t);
            std::thread::sleep(Duration::from_millis(refresh_ms));
        }
    });

    let mut state = PanelState::Hidden;
    let mut animation: Option<SlideAnimation> = None;
    let mut telemetry = Telemetry::default();
    let mut needs_redraw = true;

    if start_visible {
        state = PanelState::Visible;
        panel.set_x(panel.visible_x)?;
        panel.raise_panel()?;
    } else {
        panel.set_x(panel.hidden_x)?;
    }

    loop {
        while let Some(event) = panel.poll_event()? {
            match event {
                Event::EnterNotify(EnterNotifyEvent { event: win, .. }) => {
                    if win == panel.trigger_win && state == PanelState::Hidden {
                        state = PanelState::SlidingIn;
                        animation = Some(SlideAnimation::new(
                            panel.hidden_x as f64,
                            panel.visible_x as f64,
                            config.animation_duration_ms(),
                        ));
                        panel.raise_panel()?;
                        needs_redraw = true;
                    }
                }
                Event::LeaveNotify(LeaveNotifyEvent { event: win, .. }) => {
                    if win == panel.panel_win && state == PanelState::Visible {
                        state = PanelState::SlidingOut;
                        animation = Some(SlideAnimation::new(
                            panel.visible_x as f64,
                            panel.hidden_x as f64,
                            config.animation_duration_ms(),
                        ));
                    }
                }
                Event::Expose(_) => {
                    if state == PanelState::Visible || state == PanelState::SlidingIn {
                        needs_redraw = true;
                    }
                }
                _ => {}
            }
        }

        if let Some(anim) = &animation {
            let x = anim.current_value();
            panel.set_x(x as i32)?;
            if anim.is_finished() {
                animation = None;
                state = match state {
                    PanelState::SlidingIn => PanelState::Visible,
                    PanelState::SlidingOut => PanelState::Hidden,
                    other => other,
                };
            }
        }

        if let Ok(new_t) = rx.try_recv() {
            telemetry = new_t;
            if state != PanelState::Hidden {
                needs_redraw = true;
            }
        }

        if needs_redraw && state != PanelState::Hidden {
            let data = renderer.render(&telemetry);
            panel.put_image(&data)?;
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