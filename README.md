# mon-panel

A slide-out vertical panel displaying LLM server telemetry. Docked to the
right edge of the screen, hidden by default, slides out on mouse hover.
Designed for the i3 window manager.

## Configuration

TOML config file. By default searched in `~/.config/mon-panel/config.toml`,
can be overridden with `--config path`:

```toml
# All fields are optional — missing values use defaults
# Sections can appear in any order

[panel]
width = 260
side = "Right"                # "Left" | "Right"
animation_duration_ms = 200
trigger_width = 3

[telemetry]
refresh_interval_ms = 10000
# Graph update interval: number of telemetry refreshes per graph data point
# (integer >= 1). The graph shows the max value of each metric over that
# many refreshes. Default: 1.
graph_update_interval = 1
llama_swap_url = "http://localhost:8080"
# llama_swap_api_key = "sk-..."
# Alternative to Beszel: sysmetrics (https://github.com/punzik/sysmetrics)
# If [beszel] is present, it takes priority over telemetry_url
# telemetry_url = "http://172.16.3.66:9100"

# Beszel Hub — omit this section entirely if using sysmetrics
[beszel]
hub_url = "https://mon.embddr.xyz"
email = "user@example.com"
password = "secret"
system_id = "abc123def456"

[display]
font_family = "Sans"
font_size = 13.0

[colors]
bg = { r = 0.08, g = 0.08, b = 0.10, a = 0.95 }
fg = { r = 0.9, g = 0.9, b = 0.92, a = 1.0 }
accent = { r = 0.3, g = 0.7, b = 1.0, a = 1.0 }
warn = { r = 1.0, g = 0.6, b = 0.3, a = 1.0 }
dim = { r = 0.5, g = 0.5, b = 0.55, a = 1.0 }
bar_bg = { r = 0.2, g = 0.2, b = 0.25, a = 1.0 }

[thresholds]
cpu_temp_warn = 80
gpu_temp_warn = 80
```

## Running

```sh
nix develop
cargo run --release

# Specify config explicitly:
cargo run --release -- --config /path/to/config.toml

# Debug (panel starts visible):
cargo run --release -- --visible
```

Use the mouse wheel while the pointer is over the panel to scroll when its
content exceeds the screen height. A scrollbar appears at the right edge.

## Data Sources

| Source | Data | How |
|---|---|---|
| **Beszel Hub** | CPU, RAM, Disk, Swap, GPU, temperatures | PocketBase REST API (`/api/collections/system_stats/records`) |
| **[sysmetrics](https://github.com/punzik/sysmetrics)** | CPU, RAM, Disk, Swap, GPU, temperatures | REST API (`/api/metrics`) |
| **llama-swap** | Loaded models, inference metrics | OpenAI-compatible `/v1/models`, SSE `/api/events`, Prometheus `/upstream/<id>/metrics` |

System metrics (CPU, RAM, GPU, temperatures) can be obtained from one of two
sources:

### Beszel Hub

Configured via the `[beszel]` section. The panel authenticates with Hub
(JWT, cached 30 min) and polls metrics every `refresh_interval_ms`.
Data updates every 60 seconds (Beszel Hub REST API limitation).

### sysmetrics

Alternative to Beszel — [sysmetrics](https://github.com/punzik/sysmetrics),
a lightweight Rust service that reads system metrics directly (`/sys/class/hwmon`,
`nvidia-smi`, `sysinfo`). Set `telemetry_url` in the `[telemetry]` section;
the `[beszel]` section is not needed:

```toml
[telemetry]
telemetry_url = "http://172.16.3.66:9100"
```

If `[beszel]` is present, it takes priority over `telemetry_url`.
Data from sysmetrics is available without delay (real-time request).

## Tech Stack

| Layer | Choice |
|---|---|
| Language | Rust |
| X11 | x11rb (pure Rust) |
| Graphics | cairo-rs |
| Text | pango + pangocairo |
| HTTP | ureq |
| JSON | serde + serde_json |
| Config | toml + serde |

## Project Structure

```
src/
  main.rs        Main loop, state machine, --config parsing
  config.rs      TOML config, defaults, loading
  window.rs      X11: 32-bit ARGB window (override_redirect), trigger
  render.rs      Cairo/Pango: background, text, sparkline graphs
  telemetry.rs   TelemetryFetcher: Beszel Hub / sysmetrics + llama-swap
  animation.rs   Ease-in-out animation
```