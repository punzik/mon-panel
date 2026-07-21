# mon-panel

Выезжающая вертикальная панель для отображения телеметрии сервера с инференсом LLM.
Позиционируется у правого края экрана, скрыта в обычном состоянии, выезжает при
наведении мыши на край. Ориентирована на работу в оконном менеджере i3.

## Конфигурация

Конфиг в TOML. По умолчанию ищется в `~/.config/mon-panel/config.toml`,
можно указать через `--config path`:

```toml
# Все поля опциональны — отсутствующие используют значения по умолчанию

panel_width = 260
side = "Right"                # "Left" | "Right"
animation_duration_ms = 200
trigger_width = 3

refresh_interval_ms = 2000
llama_swap_url = "http://localhost:8080"

[beszel]
hub_url = "https://mon.embddr.xyz"
email = "user@example.com"
password = "secret"
system_id = "abc123def456"

font_family = "Sans"
font_size = 13.0

[bg_color]
r = 0.08
g = 0.08
b = 0.10
a = 0.95

[fg_color]
r = 0.9
g = 0.9
b = 0.92
a = 1.0

[accent_color]
r = 0.3
g = 0.7
b = 1.0
a = 1.0
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
| **llama-swap** | Список загруженных моделей | OpenAI-совместимый `/v1/models` |

Beszel Hub хранит метрики, собранные агентом с сервера. Панель аутентифицируется
в Hub (JWT, кэш 30 мин) и опрашивает последние метрики каждые `refresh_interval_ms`.

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
  telemetry.rs   TelemetryFetcher: Beszel Hub + llama-swap
  animation.rs   Ease-in-out анимация
```