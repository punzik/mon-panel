use serde::Deserialize;
use std::io::BufRead;
use std::sync::{Arc, Mutex};
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
        let capacity = capacity.max(1);
        Self {
            cpu: Vec::with_capacity(capacity),
            cpu_temp: Vec::with_capacity(capacity),
            ram: Vec::with_capacity(capacity),
            gpus: Vec::new(),
            capacity,
        }
    }

    /// Push the **maximum** value of each metric across `samples` as a single
    /// data point.  Used when the graph update interval > 1: we collect several
    /// telemetry samples and only store the peak, so the sparkline shows the
    /// worst-case value in each window.
    pub fn push_max(&mut self, samples: &[SystemMetrics]) {
        if samples.is_empty() {
            return;
        }
        let cap = self.capacity;
        let push = |v: &mut Vec<f64>, val: f64| {
            if v.len() == cap {
                v.remove(0);
            }
            v.push(val);
        };

        let cpu_max = samples
            .iter()
            .map(|s| s.cpu_percent as f64)
            .fold(0.0_f64, f64::max);
        push(&mut self.cpu, cpu_max);

        let cpu_temp_max = samples
            .iter()
            .map(|s| s.cpu_temp_c as f64)
            .fold(0.0_f64, f64::max);
        push(&mut self.cpu_temp, cpu_temp_max);

        let ram_max = samples
            .iter()
            .map(|s| {
                if s.memory_total_gb > 0.0 {
                    s.memory_used_gb / s.memory_total_gb * 100.0
                } else {
                    0.0
                }
            })
            .fold(0.0_f64, f64::max);
        push(&mut self.ram, ram_max);

        // Resize gpu history if GPU count changed
        let max_gpus = samples.iter().map(|s| s.gpus.len()).max().unwrap_or(0);
        if max_gpus == 0 {
            return;
        }
        if self.gpus.len() != max_gpus {
            self.gpus = (0..max_gpus)
                .map(|_| GpuHistory {
                    util: Vec::with_capacity(cap),
                    vram: Vec::with_capacity(cap),
                    temp: Vec::with_capacity(cap),
                })
                .collect();
        }

        for i in 0..max_gpus {
            let util_max = samples
                .iter()
                .filter_map(|s| s.gpus.get(i))
                .map(|g| g.percent as f64)
                .fold(0.0_f64, f64::max);
            push(&mut self.gpus[i].util, util_max);

            let vram_max = samples
                .iter()
                .filter_map(|s| s.gpus.get(i))
                .map(|g| {
                    if g.memory_total_gb > 0.0 {
                        g.memory_used_gb / g.memory_total_gb * 100.0
                    } else {
                        0.0
                    }
                })
                .fold(0.0_f64, f64::max);
            push(&mut self.gpus[i].vram, vram_max);

            let temp_max = samples
                .iter()
                .filter_map(|s| s.gpus.get(i))
                .map(|g| g.temp_c as f64)
                .fold(0.0_f64, f64::max);
            push(&mut self.gpus[i].temp, temp_max);
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
    m: f64, // max memory (bytes)
    #[serde(default)]
    mu: f64, // memory used (bytes)
    #[serde(default)]
    mp: f64, // memory percentage
    #[serde(default)]
    s: f64, // swap total
    #[serde(default)]
    su: f64, // swap used
    #[serde(default)]
    d: f64, // disk total
    #[serde(default)]
    du: f64, // disk used
    #[serde(default)]
    dp: f64, // disk percentage
    #[serde(default)]
    t: Option<serde_json::Value>, // temperatures
    #[serde(default)]
    g: Option<serde_json::Value>, // GPU data
    #[serde(default)]
    la: Option<Vec<f64>>, // load avg [1, 5, 15]
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct BeszelGpuData {
    #[serde(default)]
    n: String, // name
    #[serde(default)]
    mu: f64, // memory used
    #[serde(default)]
    mt: f64, // memory total
    #[serde(default)]
    u: f64, // usage %
    #[serde(default)]
    p: f64, // power
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
        let statuses = match self.model_statuses.lock() {
            Ok(statuses) => statuses.clone(),
            Err(poisoned) => {
                eprintln!("[sse] status cache lock was poisoned; recovering");
                poisoned.into_inner().clone()
            }
        };
        let models = fetch_models(
            self.config.llama_swap_url(),
            self.config.llama_swap_api_key(),
            &statuses,
        )
        .unwrap_or_default();

        let (system, system_name) = if let Some(beszel) = self.config.beszel.clone() {
            let sys = self.fetch_beszel(&beszel);
            let name = self.fetch_system_name(&beszel);
            (sys, name)
        } else {
            self.config
                .telemetry_url()
                .as_ref()
                .and_then(|url| fetch_system_metrics(url).ok())
                .map(|(m, n)| (Some(m), Some(n)))
                .unwrap_or((None, None))
        };

        Telemetry {
            models,
            system,
            system_name,
        }
    }

    fn clear_token(&mut self) {
        self.token = None;
        self.token_time = None;
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

    fn fetch_beszel(&mut self, beszel: &crate::config::BeszelConfig) -> Option<SystemMetrics> {
        let url = format!(
            "{}/api/collections/system_stats/records?filter=system='{}'%26%26type='1m'&sort=-created&perPage=1",
            beszel.hub_url.trim_end_matches('/'),
            beszel.system_id
        );
        let request_stats = |token: &str| {
            ureq::get(&url)
                .timeout(Duration::from_secs(2))
                .set("Authorization", &format!("Bearer {token}"))
                .call()
                .map_err(Box::new)
        };

        let token = self.get_token(beszel)?;
        let resp = match request_stats(&token) {
            Ok(response) => response,
            Err(error) if matches!(error.as_ref(), ureq::Error::Status(401 | 403, _)) => {
                eprintln!("[beszel] token rejected; re-authenticating");
                self.clear_token();
                let refreshed_token = self.get_token(beszel)?;
                match request_stats(&refreshed_token) {
                    Ok(response) => response,
                    Err(error) => {
                        eprintln!("[beszel] fetch stats failed after re-authentication: {error}");
                        return None;
                    }
                }
            }
            Err(error) => {
                eprintln!("[beszel] fetch stats failed: {error}");
                return None;
            }
        };

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
            if let Ok(map) = serde_json::from_value::<
                std::collections::HashMap<String, BeszelGpuData>,
            >(g.clone())
            {
                let mut entries: Vec<(_, _)> = map.into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                for (_, gpu) in entries {
                    let temp_c = temp_map.get(&gpu.n).copied().unwrap_or(0.0) as f32;
                    gpus.push(GpuInfo {
                        name: gpu.n,
                        percent: gpu.u as f32,
                        memory_used_gb: gpu.mu / 1024.0, // MB → GB
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
            swap_pct: if stats.s > 0.0 {
                stats.su / stats.s * 100.0
            } else {
                0.0
            },
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
) -> Result<Vec<ModelInfo>, Box<dyn std::error::Error>> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let mut req = ureq::get(&url).timeout(Duration::from_secs(2));
    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    let resp: ModelsResponse = req.call()?.into_json()?;
    let ready_ids: Vec<String> = resp
        .data
        .into_iter()
        .filter(|model| {
            statuses
                .get(&model.id)
                .is_some_and(|state| state == "ready")
        })
        .map(|model| model.id)
        .collect();

    let models = std::thread::scope(|scope| {
        ready_ids
            .into_iter()
            .map(|model_id| {
                scope.spawn(move || {
                    let metrics = fetch_model_metrics(base_url, &model_id, api_key);
                    ModelInfo {
                        name: model_id,
                        state: ModelState::Ready,
                        is_processing: metrics.4,
                        prompt_tokens_total: metrics.0,
                        tokens_predicted_total: metrics.1,
                        prompt_tokens_seconds: metrics.2,
                        predicted_tokens_seconds: metrics.3,
                    }
                })
            })
            .map(|handle| handle.join().expect("model metrics worker panicked"))
            .collect()
    });

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
        match sse_connect(&url, api_key, statuses) {
            Ok(()) => eprintln!("[sse] stream ended, reconnecting in 2s..."),
            Err(error) => eprintln!("[sse] disconnected ({error}), reconnecting in 2s..."),
        }
        std::thread::sleep(Duration::from_secs(2));
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
                    let mut cache = match statuses.lock() {
                        Ok(cache) => cache,
                        Err(poisoned) => {
                            eprintln!("[sse] status cache lock was poisoned; recovering");
                            poisoned.into_inner()
                        }
                    };
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
        encode_path_segment(model_id)
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
        let mut parts = line.split_whitespace();
        let metric = parts.next().unwrap_or("");
        let name = metric.split_once('{').map_or(metric, |(name, _)| name);
        let value = parts.next().unwrap_or("");
        match name {
            "llamacpp:prompt_tokens_total" => prompt_total += value.parse::<u64>().unwrap_or(0),
            "llamacpp:tokens_predicted_total" => {
                predicted_total += value.parse::<u64>().unwrap_or(0)
            }
            "llamacpp:prompt_tokens_seconds" => prompt_tok_s += value.parse::<f64>().unwrap_or(0.0),
            "llamacpp:predicted_tokens_seconds" => {
                predicted_tok_s += value.parse::<f64>().unwrap_or(0.0)
            }
            "llamacpp:requests_processing" => {
                requests_processing += value.parse::<i64>().unwrap_or(0)
            }
            _ => {}
        }
    }

    (
        prompt_total,
        predicted_total,
        prompt_tok_s,
        predicted_tok_s,
        requests_processing > 0,
    )
}

fn encode_path_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

// --- sysmetrics /api/metrics response types ---

#[derive(Deserialize)]
struct SysinfoResponse {
    system: SysinfoSystem,
    cpu: SysinfoCpu,
    memory: SysinfoMemory,
    #[serde(default)]
    disks: Vec<SysinfoDisk>,
    #[serde(default)]
    temperatures: Vec<SysinfoTemp>,
    #[serde(default)]
    gpus: Vec<SysinfoGpu>,
}

#[derive(Deserialize)]
struct SysinfoSystem {
    hostname: String,
    #[serde(default)]
    uptime_seconds: u64,
}

#[derive(Deserialize)]
struct SysinfoCpu {
    overall_usage: f32,
    #[serde(default)]
    load_avg: Option<SysinfoLoadAvg>,
}

#[derive(Deserialize)]
struct SysinfoLoadAvg {
    one: f64,
}

#[derive(Deserialize)]
struct SysinfoMemory {
    ram: SysinfoMem,
    swap: SysinfoMem,
}

#[derive(Deserialize)]
struct SysinfoMem {
    used_bytes: u64,
    total_bytes: u64,
    used_percent: f64,
}

#[derive(Deserialize)]
struct SysinfoDisk {
    mountpoint: String,
    used_percent: f64,
}

#[derive(Deserialize)]
struct SysinfoTemp {
    chip: String,
    label: String,
    temp_c: f64,
}

#[derive(Deserialize)]
struct SysinfoGpu {
    #[allow(dead_code)]
    index: u32,
    name: String,
    utilization_gpu: f64,
    memory_used_mib: f64,
    memory_total_mib: f64,
    temperature_gpu: f64,
}

/// Fetch system metrics from sysinfo-crawler `/api/metrics`.
/// Returns (SystemMetrics, hostname).
fn fetch_system_metrics(url: &str) -> Result<(SystemMetrics, String), Box<dyn std::error::Error>> {
    let endpoint = format!("{}/api/metrics", url.trim_end_matches('/'));
    let resp: SysinfoResponse = ureq::get(&endpoint)
        .timeout(Duration::from_secs(2))
        .call()?
        .into_json()?;

    // CPU temp: prefer coretemp "Core 0", then "Package id 0", then any coretemp Core
    let cpu_temp = resp
        .temperatures
        .iter()
        .find(|t| t.chip == "coretemp" && t.label == "Core 0")
        .or_else(|| {
            resp.temperatures
                .iter()
                .find(|t| t.chip == "coretemp" && t.label == "Package id 0")
        })
        .or_else(|| {
            resp.temperatures
                .iter()
                .find(|t| t.chip == "coretemp" && t.label.starts_with("Core"))
        })
        .map(|t| t.temp_c as f32)
        .unwrap_or(0.0);

    // Disk: prefer root mountpoint, fall back to first disk
    let disk_pct = resp
        .disks
        .iter()
        .find(|d| d.mountpoint == "/")
        .or_else(|| resp.disks.first())
        .map(|d| d.used_percent)
        .unwrap_or(0.0);

    let gpus = resp
        .gpus
        .iter()
        .map(|g| GpuInfo {
            name: g.name.clone(),
            percent: g.utilization_gpu as f32,
            memory_used_gb: g.memory_used_mib / 1024.0,
            memory_total_gb: g.memory_total_mib / 1024.0,
            temp_c: g.temperature_gpu as f32,
        })
        .collect();

    let metrics = SystemMetrics {
        cpu_percent: resp.cpu.overall_usage,
        cpu_temp_c: cpu_temp,
        memory_used_gb: resp.memory.ram.used_bytes as f64 / 1e9,
        memory_total_gb: resp.memory.ram.total_bytes as f64 / 1e9,
        gpus,
        disk_pct,
        swap_pct: resp.memory.swap.used_percent,
        load_avg: resp.cpu.load_avg.as_ref().map(|la| la.one).unwrap_or(0.0),
        uptime_secs: resp.system.uptime_seconds,
    };

    Ok((metrics, resp.system.hostname))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_model_id_as_one_url_path_segment() {
        assert_eq!(encode_path_segment("model/a b"), "model%2Fa%20b");
    }

    #[test]
    fn push_max_stores_peak_of_each_metric() {
        let mut h = History::new(10);

        let s1 = SystemMetrics {
            cpu_percent: 30.0,
            cpu_temp_c: 50.0,
            memory_used_gb: 4.0,
            memory_total_gb: 16.0,
            gpus: vec![GpuInfo {
                name: "GPU0".into(),
                percent: 40.0,
                memory_used_gb: 2.0,
                memory_total_gb: 8.0,
                temp_c: 60.0,
            }],
            ..Default::default()
        };
        let s2 = SystemMetrics {
            cpu_percent: 80.0,
            cpu_temp_c: 75.0,
            memory_used_gb: 10.0,
            memory_total_gb: 16.0,
            gpus: vec![GpuInfo {
                name: "GPU0".into(),
                percent: 95.0,
                memory_used_gb: 6.0,
                memory_total_gb: 8.0,
                temp_c: 78.0,
            }],
            ..Default::default()
        };
        let s3 = SystemMetrics {
            cpu_percent: 50.0,
            cpu_temp_c: 65.0,
            memory_used_gb: 8.0,
            memory_total_gb: 16.0,
            gpus: vec![GpuInfo {
                name: "GPU0".into(),
                percent: 70.0,
                memory_used_gb: 5.0,
                memory_total_gb: 8.0,
                temp_c: 70.0,
            }],
            ..Default::default()
        };

        h.push_max(&[s1, s2, s3]);

        assert_eq!(h.cpu, vec![80.0]);       // max(30, 80, 50)
        assert_eq!(h.cpu_temp, vec![75.0]);    // max(50, 75, 65)
        assert_eq!(h.ram, vec![62.5]);       // max(25, 62.5, 50)%
        assert_eq!(h.gpus[0].util, vec![95.0]);  // max(40, 95, 70)
        assert_eq!(h.gpus[0].vram, vec![75.0]); // max(25, 75, 62.5)%
        assert_eq!(h.gpus[0].temp, vec![78.0]);  // max(60, 78, 70)
    }

    #[test]
    fn push_max_with_empty_samples_is_noop() {
        let mut h = History::new(10);
        h.push_max(&[]);
        assert!(h.cpu.is_empty());
    }

    #[test]
    fn push_max_respects_capacity() {
        let mut h = History::new(2);
        h.push_max(&[SystemMetrics { cpu_percent: 10.0, ..Default::default() }]);
        h.push_max(&[SystemMetrics { cpu_percent: 20.0, ..Default::default() }]);
        h.push_max(&[SystemMetrics { cpu_percent: 30.0, ..Default::default() }]);
        // capacity = 2, oldest point (10.0) should be evicted
        assert_eq!(h.cpu, vec![20.0, 30.0]);
    }
}
