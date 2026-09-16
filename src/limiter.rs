use crate::config::RateLimitConfig;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum RateLimitError {
    TooManyRequests,
    TooManyConcurrentConnections,
}

struct IpEntry {
    recent_requests: Vec<Instant>,
    active_connections: usize,
}

impl IpEntry {
    fn new() -> Self {
        Self {
            recent_requests: Vec::new(),
            active_connections: 0,
        }
    }
}

pub struct StreamRateLimiter {
    config: RateLimitConfig,
    state: RwLock<HashMap<String, IpEntry>>,
}

impl StreamRateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            state: RwLock::new(HashMap::new()),
        }
    }

    pub fn acquire(self: &Arc<Self>, ip: String) -> Result<ConnectionGuard, RateLimitError> {
        if !self.config.enabled {
            return Ok(ConnectionGuard {
                limiter: Arc::clone(self),
                ip,
                active: false,
            });
        }

        let now = Instant::now();
        let window = Duration::from_secs(60);

        let mut lock = self.state.write().unwrap_or_else(|e| e.into_inner());
        let entry = lock.entry(ip.clone()).or_insert_with(IpEntry::new);

        // Retain only requests within the 60s window
        entry
            .recent_requests
            .retain(|&t| now.duration_since(t) < window);

        if entry.active_connections >= self.config.max_connections_per_ip {
            return Err(RateLimitError::TooManyConcurrentConnections);
        }

        if entry.recent_requests.len() >= self.config.requests_per_minute as usize {
            return Err(RateLimitError::TooManyRequests);
        }

        entry.recent_requests.push(now);
        entry.active_connections += 1;

        Ok(ConnectionGuard {
            limiter: Arc::clone(self),
            ip,
            active: true,
        })
    }

    fn release(&self, ip: &str) {
        if let Ok(mut lock) = self.state.write()
            && let Some(entry) = lock.get_mut(ip)
        {
            entry.active_connections = entry.active_connections.saturating_sub(1);
        }
    }
}

pub struct ConnectionGuard {
    limiter: Arc<StreamRateLimiter>,
    ip: String,
    active: bool,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if self.active {
            self.limiter.release(&self.ip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_under_limit() {
        let cfg = RateLimitConfig {
            enabled: true,
            requests_per_minute: 5,
            burst_size: 5,
            max_connections_per_ip: 5,
        };
        let limiter = Arc::new(StreamRateLimiter::new(cfg));
        let ip = "1.2.3.4".to_string();

        let guard1 = limiter.acquire(ip.clone());
        assert!(guard1.is_ok());
        let guard2 = limiter.acquire(ip);
        assert!(guard2.is_ok());
    }

    #[test]
    fn test_concurrent_connections_limit() {
        let cfg = RateLimitConfig {
            enabled: true,
            requests_per_minute: 10,
            burst_size: 10,
            max_connections_per_ip: 2,
        };
        let limiter = Arc::new(StreamRateLimiter::new(cfg));
        let ip = "192.168.1.50".to_string();

        let g1 = limiter.acquire(ip.clone()).expect("1st conn ok");
        let g2 = limiter.acquire(ip.clone()).expect("2nd conn ok");
        let g3 = limiter.acquire(ip.clone());
        assert_eq!(g3.err(), Some(RateLimitError::TooManyConcurrentConnections));

        drop(g1);
        let g4 = limiter.acquire(ip);
        assert!(g4.is_ok(), "Should acquire after previous dropped");
        drop(g2);
        drop(g4);
    }

    #[test]
    fn test_requests_per_minute_limit() {
        let cfg = RateLimitConfig {
            enabled: true,
            requests_per_minute: 2,
            burst_size: 2,
            max_connections_per_ip: 10,
        };
        let limiter = Arc::new(StreamRateLimiter::new(cfg));
        let ip = "10.0.0.1".to_string();

        let g1 = limiter.acquire(ip.clone()).expect("1st ok");
        drop(g1);
        let g2 = limiter.acquire(ip.clone()).expect("2nd ok");
        drop(g2);

        let g3 = limiter.acquire(ip);
        assert_eq!(g3.err(), Some(RateLimitError::TooManyRequests));
    }
}
