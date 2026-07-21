use serde::Deserialize;
use std::time::{Duration, Instant};

#[derive(Default, Clone, Debug)]
pub struct Telemetry {
    pub models: Vec<ModelInfo>,
    pub system: Option<SystemMetrics>,
    pub system_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub name: String,
    pub loaded: bool,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct SystemMetrics {
    #[serde(default)]
    pub cpu_percent: f32,
    #[serde(default)]
    pub memory_used_gb: f64,
    #[serde(default)]
    pub memory_total_gb: f64,
    #[serde(default)]
    pub gpu_percent: f32,
    #[serde(default)]
    pub gpu_memory_used_gb: f64,
    #[serde(default)]
    pub gpu_memory_total_gb: f64,
    #[serde(default)]
    pub gpu_temp_c: f32,
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
}

impl TelemetryFetcher {
    pub fn new(config: crate::config::Config) -> Self {
        Self {
            config,
            token: None,
            token_time: None,
            system_name: None,
            system_name_time: None,
        }
    }

    pub fn fetch(&mut self) -> Telemetry {
        let models = fetch_models(&self.config.llama_swap_url).unwrap_or_default();

        let (system, system_name) = if let Some(beszel) = self.config.beszel.clone() {
            let sys = self.fetch_beszel(&beszel);
            let name = self.fetch_system_name(&beszel);
            (sys, name)
        } else {
            (
                self.config
                    .telemetry_url
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
            "{}/api/collections/system_stats/records?filter=system='{}'&sort=-created&perPage=1",
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

        // Parse GPU data (map[string]GPUData) — memory in MB, usage in %
        let mut gpu_percent = 0.0f32;
        let mut gpu_mem_used = 0.0f64;
        let mut gpu_mem_total = 0.0f64;
        let mut gpu_count = 0u32;
        if let Some(g) = &stats.g {
            if let Ok(map) = serde_json::from_value::<std::collections::HashMap<String, BeszelGpuData>>(g.clone()) {
                for gpu in map.values() {
                    gpu_percent += gpu.u as f32;
                    gpu_mem_used += gpu.mu / 1024.0; // MB → GB
                    gpu_mem_total += gpu.mt / 1024.0;
                    gpu_count += 1;
                }
                if gpu_count > 0 {
                    gpu_percent /= gpu_count as f32;
                }
            }
        }

        // Parse temperature (map[string]float64, take first or average)
        let gpu_temp = if let Some(t) = &stats.t {
            if let Ok(map) = serde_json::from_value::<std::collections::HashMap<String, f64>>(t.clone()) {
                // Try to find a GPU temp, otherwise take the first
                map.iter()
                    .find(|(k, _)| k.to_lowercase().contains("gpu") || k.to_lowercase().contains("nvidia"))
                    .or_else(|| map.iter().next())
                    .map(|(_, v)| *v as f32)
                    .unwrap_or(0.0)
            } else {
                0.0
            }
        } else {
            0.0
        };

        let load_avg = stats
            .la
            .as_ref()
            .and_then(|la| la.first().copied())
            .unwrap_or(0.0);

        Some(SystemMetrics {
            cpu_percent: stats.cpu as f32,
            // m / mu are in GB already
            memory_used_gb: stats.mu,
            memory_total_gb: stats.m,
            gpu_percent,
            gpu_memory_used_gb: gpu_mem_used,
            gpu_memory_total_gb: gpu_mem_total,
            gpu_temp_c: gpu_temp,
            disk_pct: stats.dp,
            // s / su are in GB
            swap_pct: if stats.s > 0.0 { stats.su / stats.s * 100.0 } else { 0.0 },
            load_avg,
            uptime_secs: 0,
        })
    }
}

fn fetch_models(base_url: &str) -> Result<Vec<ModelInfo>, ureq::Error> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let resp: ModelsResponse = ureq::get(&url)
        .timeout(Duration::from_secs(1))
        .call()?
        .into_json()?;

    Ok(resp
        .data
        .into_iter()
        .map(|m| ModelInfo {
            name: m.id,
            loaded: true,
        })
        .collect())
}

fn fetch_system_metrics(url: &str) -> Result<SystemMetrics, Box<dyn std::error::Error>> {
    let endpoint = format!("{}/metrics", url.trim_end_matches('/'));
    Ok(ureq::get(&endpoint)
        .timeout(Duration::from_secs(1))
        .call()?
        .into_json::<SystemMetrics>()?)
}