# mon-panel

Выезжающая вертикальная панель для отображения телеметрии сервера с инференсом LLM.
Позиционируется у правого края экрана, скрыта в обычном состоянии, выезжает при
наведении мыши на край. Ориентирована на работу в оконном менеджере i3.

## Конфигурация

Конфиг в TOML. По умолчанию ищется в `~/.config/mon-panel/config.toml`,
можно указать через `--config path`:

```toml
# Все поля опциональны — отсутствующие используют значения по умолчанию
# Секции могут идти в любом порядке

[panel]
width = 260
side = "Right"                # "Left" | "Right"
animation_duration_ms = 200
trigger_width = 3

[telemetry]
refresh_interval_ms = 10000
llama_swap_url = "http://localhost:8080"
# llama_swap_api_key = "sk-..."
# Альтернатива Beszel — sysmetrics (https://github.com/punzik/sysmetrics)
# Если [beszel] задан, он имеет приоритет над telemetry_url
# telemetry_url = "http://172.16.3.66:9100"

# Beszel Hub — убрать секцию целиком, если используется sysmetrics
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

## Запуск

```sh
nix develop
cargo run --release

# Указать конфиг явно:
cargo run --release -- --config /path/to/config.toml

# Дебаг (панель стартует видимой):
cargo run --release -- --visible
```

## Источники данных

| Источник | Что получает | Как |
|---|---|---|
| **Beszel Hub** | CPU, RAM, Disk, Swap, GPU, температура | PocketBase REST API (`/api/collections/system_stats/records`) |
| **[sysmetrics](https://github.com/punzik/sysmetrics)** | CPU, RAM, Disk, Swap, GPU, температура | REST API (`/api/metrics`) |
| **llama-swap** | Список загруженных моделей, метрики инференса | OpenAI-совместимый `/v1/models`, SSE `/api/events`, Prometheus `/upstream/<id>/metrics` |

Системные метрики (CPU, RAM, GPU, температуры) можно получать из одного из
двух источников на выбор:

### Beszel Hub

Конфигурация через секцию `[beszel]`. Панель аутентифицируется в Hub
(JWT, кэш 30 мин) и опрашивает метрики каждые `refresh_interval_ms`.
Данные обновляются раз в 60 секунд (ограничение Beszel Hub REST API).

### sysmetrics

Альтернатива Beszel — [sysmetrics](https://github.com/punzik/sysmetrics),
лёгкий Rust-сервис, читающий системные метрики напрямую (`/sys/class/hwmon`,
`nvidia-smi`, `sysinfo`). В конфиге указывается `telemetry_url` в секции
`[telemetry]`, секция `[beszel]` при этом не нужна:

```toml
[telemetry]
telemetry_url = "http://172.16.3.66:9100"
```

Если `[beszel]` присутствует, она имеет приоритет над `telemetry_url`.
Данные от sysmetrics доступны без задержки (запрос в реальном времени).

## Стек

| Слой | Выбор |
|---|---|
| Язык | Rust |
| X11 | x11rb (pure Rust) |
| Графика | cairo-rs |
| Текст | pango + pangocairo |
| HTTP | ureq |
| JSON | serde + serde_json |
| Конфиг | toml + serde |

## Структура

```
src/
  main.rs        Главный цикл, конечный автомат, парсинг --config
  config.rs      TOML-конфиг, дефолты, загрузка
  window.rs      X11: 32-битное ARGB-окно (override_redirect), trigger
  render.rs      Cairo/Pango: фон, текст, прогресс-бары
  telemetry.rs   TelemetryFetcher: Beszel Hub / sysmetrics + llama-swap
  animation.rs   Ease-in-out анимация
```