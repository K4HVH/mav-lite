use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{info, warn};

/// Global metrics for the router
#[derive(Debug, Clone)]
pub struct Metrics {
    /// Total messages routed
    pub messages_routed: Arc<AtomicU64>,
    /// Total messages received
    pub messages_received: Arc<AtomicU64>,
    /// Total messages dropped (backpressure)
    pub messages_dropped: Arc<AtomicU64>,
    /// Total bytes routed
    pub bytes_routed: Arc<AtomicU64>,
    /// Start time for calculating uptime
    pub start_time: Instant,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            messages_routed: Arc::new(AtomicU64::new(0)),
            messages_received: Arc::new(AtomicU64::new(0)),
            messages_dropped: Arc::new(AtomicU64::new(0)),
            bytes_routed: Arc::new(AtomicU64::new(0)),
            start_time: Instant::now(),
        }
    }

    pub fn record_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_routed(&self, bytes: usize) {
        self.messages_routed.fetch_add(1, Ordering::Relaxed);
        self.bytes_routed.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub fn record_dropped(&self) {
        self.messages_dropped.fetch_add(1, Ordering::Relaxed);
        warn!("Message dropped due to backpressure!");
    }

    pub fn get_stats(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            messages_received: self.messages_received.load(Ordering::Relaxed),
            messages_routed: self.messages_routed.load(Ordering::Relaxed),
            messages_dropped: self.messages_dropped.load(Ordering::Relaxed),
            bytes_routed: self.bytes_routed.load(Ordering::Relaxed),
            uptime: self.start_time.elapsed(),
        }
    }

    /// Start a background task that logs stats periodically
    pub fn start_stats_logger(self, interval_secs: u64) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));
            let mut last_stats = self.get_stats();

            loop {
                interval.tick().await;
                let current_stats = self.get_stats();
                let delta = current_stats.delta(&last_stats, interval_secs);

                info!("=== Performance Stats ===");
                info!(
                    "  Uptime: {}h {}m {}s",
                    current_stats.uptime.as_secs() / 3600,
                    (current_stats.uptime.as_secs() % 3600) / 60,
                    current_stats.uptime.as_secs() % 60
                );
                info!(
                    "  Messages: {} received, {} routed, {} dropped",
                    current_stats.messages_received,
                    current_stats.messages_routed,
                    current_stats.messages_dropped
                );
                info!(
                    "  Throughput: {:.1} msg/s, {:.1} KB/s",
                    delta.messages_per_sec, delta.kbytes_per_sec
                );
                info!("  Total data: {:.2} MB", delta.total_mb);

                if current_stats.messages_dropped > last_stats.messages_dropped {
                    warn!(
                        "  ⚠ {} messages dropped in last {} seconds (BACKPRESSURE DETECTED)",
                        current_stats.messages_dropped - last_stats.messages_dropped,
                        interval_secs
                    );
                }

                last_stats = current_stats;
            }
        });
    }
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub messages_received: u64,
    pub messages_routed: u64,
    pub messages_dropped: u64,
    pub bytes_routed: u64,
    pub uptime: Duration,
}

impl MetricsSnapshot {
    pub fn delta(&self, previous: &MetricsSnapshot, interval_secs: u64) -> MetricsDelta {
        let messages_diff = self.messages_routed.saturating_sub(previous.messages_routed);
        let bytes_diff = self.bytes_routed.saturating_sub(previous.bytes_routed);

        MetricsDelta {
            messages_per_sec: messages_diff as f64 / interval_secs as f64,
            kbytes_per_sec: (bytes_diff as f64 / 1024.0) / interval_secs as f64,
            total_mb: self.bytes_routed as f64 / 1024.0 / 1024.0,
        }
    }
}

#[derive(Debug)]
pub struct MetricsDelta {
    pub messages_per_sec: f64,
    pub kbytes_per_sec: f64,
    pub total_mb: f64,
}
