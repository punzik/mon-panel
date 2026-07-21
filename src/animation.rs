use std::time::{Duration, Instant};

pub struct SlideAnimation {
    start: Instant,
    duration: Duration,
    from: f64,
    to: f64,
}

impl SlideAnimation {
    pub fn new(from: f64, to: f64, duration_ms: u64) -> Self {
        Self {
            start: Instant::now(),
            duration: Duration::from_millis(duration_ms),
            from,
            to,
        }
    }

    pub fn current_value(&self) -> f64 {
        let elapsed = self.start.elapsed().as_secs_f64();
        let total = self.duration.as_secs_f64();
        let t = (elapsed / total).clamp(0.0, 1.0);
        let eased = ease_in_out(t);
        self.from + (self.to - self.from) * eased
    }

    pub fn is_finished(&self) -> bool {
        self.start.elapsed() >= self.duration
    }
}

fn ease_in_out(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}