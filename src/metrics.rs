use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub struct MetricsTracker {
    active_listeners: AtomicUsize,
    streamed_bytes_total: AtomicU64,
    buffer_underruns_total: AtomicU64,
    user_agents: RwLock<HashMap<String, u64>>,
}

impl Default for MetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsTracker {
    pub fn new() -> Self {
        Self {
            active_listeners: AtomicUsize::new(0),
            streamed_bytes_total: AtomicU64::new(0),
            buffer_underruns_total: AtomicU64::new(0),
            user_agents: RwLock::new(HashMap::new()),
        }
    }

    pub fn inc_listener(&self, user_agent_str: Option<&str>) {
        self.active_listeners.fetch_add(1, Ordering::Relaxed);
        let client_category = classify_user_agent(user_agent_str);
        if let Ok(mut map) = self.user_agents.write() {
            *map.entry(client_category).or_insert(0) += 1;
        }
    }

    pub fn dec_listener(&self) {
        // Saturating subtraction to prevent underflow
        let _ = self
            .active_listeners
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |val| {
                Some(val.saturating_sub(1))
            });
    }

    pub fn add_streamed_bytes(&self, bytes: u64) {
        self.streamed_bytes_total
            .fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn inc_buffer_underrun(&self) {
        self.buffer_underruns_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn active_listeners(&self) -> usize {
        self.active_listeners.load(Ordering::Relaxed)
    }

    pub fn streamed_bytes_total(&self) -> u64 {
        self.streamed_bytes_total.load(Ordering::Relaxed)
    }

    pub fn buffer_underruns_total(&self) -> u64 {
        self.buffer_underruns_total.load(Ordering::Relaxed)
    }

    pub fn render_prometheus(&self) -> String {
        let listeners = self.active_listeners();
        let bytes = self.streamed_bytes_total();
        let bytes_gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let underruns = self.buffer_underruns_total();

        let mut out = String::with_capacity(1024);

        out.push_str("# HELP radio_listeners_active Current active listeners count.\n");
        out.push_str("# TYPE radio_listeners_active gauge\n");
        out.push_str(&format!("radio_listeners_active {}\n\n", listeners));

        out.push_str(
            "# HELP radio_streamed_bytes_total Total audio bytes transferred to listeners.\n",
        );
        out.push_str("# TYPE radio_streamed_bytes_total counter\n");
        out.push_str(&format!("radio_streamed_bytes_total {}\n\n", bytes));

        out.push_str("# HELP radio_streamed_gigabytes_total Total audio gigabytes transferred to listeners.\n");
        out.push_str("# TYPE radio_streamed_gigabytes_total counter\n");
        out.push_str(&format!(
            "radio_streamed_gigabytes_total {:.6}\n\n",
            bytes_gb
        ));

        out.push_str(
            "# HELP radio_buffer_underruns_total Broadcast buffer lagged/underrun events count.\n",
        );
        out.push_str("# TYPE radio_buffer_underruns_total counter\n");
        out.push_str(&format!("radio_buffer_underruns_total {}\n\n", underruns));

        out.push_str("# HELP radio_client_user_agent_total Total listener connections categorized by client.\n");
        out.push_str("# TYPE radio_client_user_agent_total counter\n");

        if let Ok(ua_map) = self.user_agents.read() {
            let mut sorted: Vec<_> = ua_map.iter().collect();
            sorted.sort_by_key(|(k, _)| (*k).clone());

            for (client, count) in sorted {
                out.push_str(&format!(
                    "radio_client_user_agent_total{{client=\"{}\"}} {}\n",
                    escape_prometheus_label(client),
                    count
                ));
            }
        }

        out
    }
}

pub fn classify_user_agent(ua_opt: Option<&str>) -> String {
    let Some(ua) = ua_opt else {
        return "Unknown".to_string();
    };

    let lower = ua.to_lowercase();
    if lower.contains("vlc") {
        "VLC".to_string()
    } else if lower.contains("mpv") {
        "mpv".to_string()
    } else if lower.contains("foobar") {
        "Foobar2000".to_string()
    } else if lower.contains("winamp") {
        "Winamp".to_string()
    } else if lower.contains("curl") || lower.contains("wget") {
        "CLI".to_string()
    } else if lower.contains("chrome") && !lower.contains("edg") {
        "Chrome".to_string()
    } else if lower.contains("firefox") {
        "Firefox".to_string()
    } else if lower.contains("safari") && !lower.contains("chrome") {
        "Safari".to_string()
    } else if lower.contains("edg") {
        "Edge".to_string()
    } else {
        "Other".to_string()
    }
}

fn escape_prometheus_label(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_agent_classification() {
        assert_eq!(classify_user_agent(Some("VLC/3.0.18 LibVLC/3.0.18")), "VLC");
        assert_eq!(classify_user_agent(Some("mpv 0.35.0")), "mpv");
        assert_eq!(
            classify_user_agent(Some("Mozilla/5.0 Chrome/120.0.0.0")),
            "Chrome"
        );
        assert_eq!(
            classify_user_agent(Some("Mozilla/5.0 Firefox/122.0")),
            "Firefox"
        );
        assert_eq!(classify_user_agent(Some("curl/7.88.1")), "CLI");
        assert_eq!(classify_user_agent(None), "Unknown");
    }

    #[test]
    fn test_metrics_tracking_and_render() {
        let tracker = MetricsTracker::new();
        assert_eq!(tracker.active_listeners(), 0);

        tracker.inc_listener(Some("VLC/3.0.18"));
        tracker.inc_listener(Some("Mozilla/5.0 Firefox/120.0"));
        assert_eq!(tracker.active_listeners(), 2);

        tracker.add_streamed_bytes(1024 * 1024 * 10);
        assert_eq!(tracker.streamed_bytes_total(), 10 * 1024 * 1024);

        tracker.inc_buffer_underrun();
        assert_eq!(tracker.buffer_underruns_total(), 1);

        tracker.dec_listener();
        assert_eq!(tracker.active_listeners(), 1);

        let output = tracker.render_prometheus();
        assert!(output.contains("radio_listeners_active 1"));
        assert!(output.contains("radio_streamed_bytes_total 10485760"));
        assert!(output.contains("radio_buffer_underruns_total 1"));
        assert!(output.contains("radio_client_user_agent_total{client=\"VLC\"} 1"));
        assert!(output.contains("radio_client_user_agent_total{client=\"Firefox\"} 1"));
    }
}
