use std::io::BufRead;
use std::sync::{Arc, Mutex};
use serde::Deserialize;
use std::time::{Duration, Instant};

#[derive(Default, Clone, Debug)]
pub struct Telemetry {
    pub models: Vec<ModelInfo>,
    pub system: Option<SystemMetrics>,
    pub system_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GpuHistory {
    pub util: Vec<f64>,
    pub vram: Vec<f64>,
    pub temp: Vec<f64>,
}

/// Time-series history for graphable metrics.
/// Capacity = graph width in pixels (1 sample = 1 pixel).
#[derive(Clone, Debug)]
pub struct History {
    pub cpu: Vec<f64>,
    pub cpu_temp: Vec<f64>,
    pub ram: Vec<f64>,
    pub gpus: Vec<GpuHistory>,
    capacity: usize,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            cpu: Vec::with_capacity(capacity),
            cpu_temp: Vec::with_capacity(capacity),
            ram: Vec::with_capacity(capacity),
            gpus: Vec::new(),
            capacity,
        }
    }

    pub fn push(&mut self, sys: &SystemMetrics) {
        let cap = self.capacity;
        let push = |v: &mut Vec<f64>, val: f64| {
            if v.len() >= cap {
                v.remove(0);
            }
            v.push(val);
        };

        push(&mut self.cpu, sys.cpu_percent as f64);
        push(&mut self.cpu_temp, sys.cpu_temp_c as f64);

        let ram_pct = if sys.memory_total_gb > 0.0 {
            sys.memory_used_gb / sys.memory_total_gb * 100.0
        } else {
            0.0
        };
        push(&mut self.ram, ram_pct);

        // Resize gpu history if GPU count changed
        if self.gpus.len() != sys.gpus.len() {
            self.gpus = (0..sys.gpus.len())
                .map(|_| GpuHistory {
                    util: Vec::with_capacity(cap),
                    vram: Vec::with_capacity(cap),
                    temp: Vec::with_capacity(cap),
                })
                .collect();
        }

        for (i, gpu) in sys.gpus.iter().enumerate() {
            push(&mut self.gpus[i].util, gpu.percent as f64);
            let vram_pct = if gpu.memory_total_gb > 0.0 {
                gpu.memory_used_gb / gpu.memory_total_gb * 100.0
            } else {
                0.0
            };
            push(&mut self.gpus[i].vram, vram_pct);
            push(&mut self.gpus[i].temp, gpu.temp_c as f64);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModelState {
    #[allow(dead_code)]
    Stopped,
    Ready,
}

#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub name: String,
    pub state: ModelState,
    pub is_processing: bool,
    pub prompt_tokens_total: u64,
    pub tokens_predicted_total: u64,
    pub prompt_tokens_seconds: f64,
    pub predicted_tokens_seconds: f64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GpuInfo {
    pub name: String,
    pub percent: f32,
    pub memory_used_gb: f64,
    pub memory_total_gb: f64,
    pub temp_c: f32,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct SystemMetrics {
    #[serde(default)]
    pub cpu_percent: f32,
    #[serde(default)]
    pub cpu_temp_c: f32,
    #[serde(default)]
    pub memory_used_gb: f64,
    #[serde(default)]
    pub memory_total_gb: f64,
    #[serde(default)]
    pub gpus: Vec<GpuInfo>,
    #[serde(default)]
    pub disk_pct: f64,
    #[serde(default)]
    pub swap_pct: f64,
    #[serde(default)]
    pub load_avg: f64,
    #[serde(default)]
    #[allow(dead_code)]
    pub uptime_secs: u64,
}

// --- llama-swap /v1/models (OpenAI-compatible) ---

#[derive(Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelData>,
}

#[derive(Deserialize)]
struct ModelData {
    id: String,
}

#[derive(Deserialize)]
struct SystemRecord {
    name: String,
}

// --- Beszel hub types (PocketBase REST API) ---

#[derive(Deserialize)]
struct PbAuthResponse {
    token: String,
}

#[derive(Deserialize)]
struct PbRecordsResponse<T> {
    items: Vec<T>,
}

#[derive(Deserialize)]
struct SystemStatsRecord {
    stats: BeszelStats,
}

#[derive(Deserialize, Default)]
#[allow(dead_code)]
struct BeszelStats {
    #[serde(default)]
    cpu: f64,
    #[serde(default)]
    m: f64,  // max memory (bytes)
    #[serde(default)]
    mu: f64, // memory used (bytes)
    #[serde(default)]
    mp: f64, // memory percentage
    #[serde(default)]
    s: f64,  // swap total
    #[serde(default)]
    su: f64, // swap used
    #[serde(default)]
    d: f64,  // disk total
    #[serde(default)]
    du: f64, // disk used
    #[serde(default)]
    dp: f64, // disk percentage
    #[serde(default)]
    t: Option<serde_json::Value>,               // temperatures
    #[serde(default)]
    g: Option<serde_json::Value>,               // GPU data
    #[serde(default)]
    la: Option<Vec<f64>>,                       // load avg [1, 5, 15]
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct BeszelGpuData {
    #[serde(default)]
    n: String,  // name
    #[serde(default)]
    mu: f64,    // memory used
    #[serde(default)]
    mt: f64,    // memory total
    #[serde(default)]
    u: f64,     // usage %
    #[serde(default)]
    p: f64,     // power
}

pub struct TelemetryFetcher {
    config: crate::config::Config,
    token: Option<String>,
    token_time: Option<Instant>,
    system_name: Option<String>,
    system_name_time: Option<Instant>,
    model_statuses: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

impl TelemetryFetcher {
    pub fn new(config: crate::config::Config) -> Self {
        let model_statuses = Arc::new(Mutex::new(std::collections::HashMap::new()));

        // Spawn SSE thread for model statuses
        let statuses_clone = model_statuses.clone();
        let url = config.llama_swap_url().to_string();
        let key = config.llama_swap_api_key().map(|k| k.to_string());
        std::thread::spawn(move || {
            sse_loop(&url, key.as_deref(), &statuses_clone);
        });

        Self {
            config,
            token: None,
            token_time: None,
            system_name: None,
            system_name_time: None,
            model_statuses,
        }
    }

    pub fn fetch(&mut self) -> Telemetry {
        let statuses = self.model_statuses.lock().unwrap().clone();
        let models = fetch_models(
            self.config.llama_swap_url(),
            self.config.llama_swap_api_key(),
            &statuses,
        ).unwrap_or_default();

        let (system, system_name) = if let Some(beszel) = self.config.beszel.clone() {
            let sys = self.fetch_beszel(&beszel);
            let name = self.fetch_system_name(&beszel);
            (sys, name)
        } else {
            (
                self.config
                    .telemetry_url()
                    .as_ref()
                    .and_then(|url| fetch_system_metrics(url).ok()),
                None,
            )
        };

        Telemetry {
            models,
            system,
            system_name,
        }
    }

    fn get_token(&mut self, beszel: &crate::config::BeszelConfig) -> Option<String> {
        // Re-auth if no token or token is older than 30 minutes
        let need_auth = self
            .token
            .as_ref()
            .zip(self.token_time)
            .map(|(_, t)| t.elapsed() > Duration::from_secs(1800))
            .unwrap_or(true);

        if need_auth {
            let url = format!(
                "{}/api/collections/users/auth-with-password",
                beszel.hub_url.trim_end_matches('/')
            );
            let body = serde_json::json!({
                "identity": beszel.email,
                "password": beszel.password,
            });
            match ureq::post(&url)
                .timeout(Duration::from_secs(3))
                .send_json(body)
            {
                Ok(resp) => {
                    if let Ok(auth) = resp.into_json::<PbAuthResponse>() {
                        self.token = Some(auth.token);
                        self.token_time = Some(Instant::now());
                    }
                }
                Err(e) => {
                    eprintln!("[beszel] auth failed: {e}");
                }
            }
        }
        self.token.clone()
    }

    fn fetch_system_name(&mut self, beszel: &crate::config::BeszelConfig) -> Option<String> {
        // Cache system name for 5 minutes — it rarely changes
        let need_refresh = self
            .system_name
            .as_ref()
            .zip(self.system_name_time)
            .map(|(_, t)| t.elapsed() > Duration::from_secs(300))
            .unwrap_or(true);

        if need_refresh {
            if let Some(token) = self.get_token(beszel) {
                let url = format!(
                    "{}/api/collections/systems/records/{}",
                    beszel.hub_url.trim_end_matches('/'),
                    beszel.system_id
                );
                match ureq::get(&url)
                    .timeout(Duration::from_secs(2))
                    .set("Authorization", &format!("Bearer {token}"))
                    .call()
                {
                    Ok(resp) => {
                        if let Ok(rec) = resp.into_json::<SystemRecord>() {
                            self.system_name = Some(rec.name);
                            self.system_name_time = Some(Instant::now());
                        }
                    }
                    Err(e) => eprintln!("[beszel] fetch system name failed: {e}"),
                }
            }
        }
        self.system_name.clone()
    }

    fn fetch_beszel(
        &mut self,
        beszel: &crate::config::BeszelConfig,
    ) -> Option<SystemMetrics> {
        let token = self.get_token(beszel)?;

        let url = format!(
            "{}/api/collections/system_stats/records?filter=system='{}'%26%26type='1m'&sort=-created&perPage=1",
            beszel.hub_url.trim_end_matches('/'),
            beszel.system_id
        );

        let resp = ureq::get(&url)
            .timeout(Duration::from_secs(2))
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(|e| {
                eprintln!("[beszel] fetch stats failed: {e}");
            })
            .ok()?;

        let records: PbRecordsResponse<SystemStatsRecord> = resp.into_json().ok()?;
        let record = records.items.first()?;
        let stats = &record.stats;

        // Parse temperature map
        let temp_map: std::collections::HashMap<String, f64> = if let Some(t) = &stats.t {
            serde_json::from_value(t.clone()).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

        // CPU temperature: prefer coretemp_core_0, fall back to first coretemp
        let cpu_temp = temp_map
            .get("coretemp_core_0")
            .copied()
            .or_else(|| temp_map.get("coretemp_package_id_0").copied())
            .or_else(|| {
                temp_map
                    .iter()
                    .filter(|(k, _)| k.starts_with("coretemp_core"))
                    .map(|(_, v)| *v)
                    .next()
            })
            .unwrap_or(0.0) as f32;

        // Parse GPU data — keep per-GPU, sort by map key for stable order
        let mut gpus = Vec::new();
        if let Some(g) = &stats.g {
            if let Ok(map) = serde_json::from_value::<std::collections::HashMap<String, BeszelGpuData>>(g.clone()) {
                let mut entries: Vec<(_, _)> = map.into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                for (_, gpu) in entries {
                    let temp_c = temp_map.get(&gpu.n).copied().unwrap_or(0.0) as f32;
                    gpus.push(GpuInfo {
                        name: gpu.n,
                        percent: gpu.u as f32,
                        memory_used_gb: gpu.mu / 1024.0,  // MB → GB
                        memory_total_gb: gpu.mt / 1024.0,
                        temp_c,
                    });
                }
            }
        }

        let load_avg = stats
            .la
            .as_ref()
            .and_then(|la| la.first().copied())
            .unwrap_or(0.0);

        Some(SystemMetrics {
            cpu_percent: stats.cpu as f32,
            cpu_temp_c: cpu_temp,
            // m / mu are in GB already
            memory_used_gb: stats.mu,
            memory_total_gb: stats.m,
            gpus,
            disk_pct: stats.dp,
            // s / su are in GB
            swap_pct: if stats.s > 0.0 { stats.su / stats.s * 100.0 } else { 0.0 },
            load_avg,
            uptime_secs: 0,
        })
    }
}

#[derive(Deserialize)]
struct SseEvent {
    #[serde(rename = "type")]
    event_type: String,
    data: String,
}

#[derive(Deserialize)]
struct ModelStatus {
    id: String,
    state: String,
}

/// Fetch model list. Uses cached statuses from SSE thread.
/// Only ready models are included; metrics fetched from /upstream/<id>/metrics.
fn fetch_models(
    base_url: &str,
    api_key: Option<&str>,
    statuses: &std::collections::HashMap<String, String>,
) -> Result<Vec<ModelInfo>, ureq::Error> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let mut req = ureq::get(&url).timeout(Duration::from_secs(2));
    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    let resp: ModelsResponse = req.call()?.into_json()?;

    let models: Vec<ModelInfo> = resp.data
        .into_iter()
        .filter_map(|m| {
            let state = statuses
                .get(&m.id)
                .map(|s| s.as_str())
                .unwrap_or("stopped");

            if state == "ready" {
                let metrics = fetch_model_metrics(base_url, &m.id, api_key);
                Some(ModelInfo {
                    name: m.id,
                    state: ModelState::Ready,
                    is_processing: metrics.4,
                    prompt_tokens_total: metrics.0,
                    tokens_predicted_total: metrics.1,
                    prompt_tokens_seconds: metrics.2,
                    predicted_tokens_seconds: metrics.3,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(models)
}

/// SSE thread loop — maintains persistent connection to /api/events.
fn sse_loop(
    base_url: &str,
    api_key: Option<&str>,
    statuses: &Arc<Mutex<std::collections::HashMap<String, String>>>,
) {
    let url = format!("{}/api/events", base_url.trim_end_matches('/'));
    loop {
        if sse_connect(&url, api_key, statuses).is_err() {
            eprintln!("[sse] disconnected, reconnecting in 2s...");
            std::thread::sleep(Duration::from_secs(2));
        }
    }
}

/// Connect to SSE, read events and update status cache until disconnected.
fn sse_connect(
    url: &str,
    api_key: Option<&str>,
    statuses: &Arc<Mutex<std::collections::HashMap<String, String>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut req = ureq::get(url);
    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    let resp = req.call()?;
    let reader = std::io::BufReader::new(resp.into_reader());

    for line in reader.lines() {
        let line = line?;
        if !line.starts_with("data:") {
            continue;
        }
        let json_str = &line[5..];
        if let Ok(evt) = serde_json::from_str::<SseEvent>(json_str) {
            if evt.event_type == "modelStatus" {
                if let Ok(model_statuses) = serde_json::from_str::<Vec<ModelStatus>>(&evt.data) {
                    let mut cache = statuses.lock().unwrap();
                    cache.clear();
                    for s in model_statuses {
                        cache.insert(s.id, s.state);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Fetch llama.cpp Prometheus metrics from /upstream/<model_id>/metrics.
/// Returns (prompt_tokens_total, tokens_predicted_total, prompt_tok/s, predicted_tok/s, is_processing).
fn fetch_model_metrics(
    base_url: &str,
    model_id: &str,
    api_key: Option<&str>,
) -> (u64, u64, f64, f64, bool) {
    let url = format!(
        "{}/upstream/{}/metrics",
        base_url.trim_end_matches('/'),
        model_id
    );
    let mut req = ureq::get(&url).timeout(Duration::from_secs(2));
    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }

    let body = match req.call() {
        Ok(r) => r.into_string().unwrap_or_default(),
        Err(_) => return (0, 0, 0.0, 0.0, false),
    };

    let mut prompt_total = 0u64;
    let mut predicted_total = 0u64;
    let mut prompt_tok_s = 0.0f64;
    let mut predicted_tok_s = 0.0f64;
    let mut requests_processing = 0i64;

    for line in body.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let name = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");
        match name {
            "llamacpp:prompt_tokens_total" => prompt_total = value.parse().unwrap_or(0),
            "llamacpp:tokens_predicted_total" => predicted_total = value.parse().unwrap_or(0),
            "llamacpp:prompt_tokens_seconds" => prompt_tok_s = value.parse().unwrap_or(0.0),
            "llamacpp:predicted_tokens_seconds" => predicted_tok_s = value.parse().unwrap_or(0.0),
            "llamacpp:requests_processing" => requests_processing = value.parse().unwrap_or(0),
            _ => {}
        }
    }

    (prompt_total, predicted_total, prompt_tok_s, predicted_tok_s, requests_processing > 0)
}

fn fetch_system_metrics(url: &str) -> Result<SystemMetrics, Box<dyn std::error::Error>> {
    let endpoint = format!("{}/metrics", url.trim_end_matches('/'));
    Ok(ureq::get(&endpoint)
        .timeout(Duration::from_secs(1))
        .call()?
        .into_json::<SystemMetrics>()?)
}