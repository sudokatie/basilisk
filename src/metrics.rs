//! Performance metrics tracking
//!
//! Provides frame time, input latency, and memory usage tracking.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Maximum samples to keep for averaging
const MAX_SAMPLES: usize = 100;

/// Performance metrics tracker
pub struct Metrics {
    /// Frame time samples (nanoseconds)
    frame_times: VecDeque<u64>,
    /// Input latency samples (nanoseconds)
    input_latencies: VecDeque<u64>,
    /// Last frame start time
    last_frame_start: Option<Instant>,
    /// Last input event time
    last_input_time: Option<Instant>,
    /// Total frames rendered
    total_frames: u64,
    /// Creation time for uptime calculation
    start_time: Instant,
    /// Whether metrics collection is enabled
    enabled: bool,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// Create a new metrics tracker
    pub fn new() -> Self {
        Self {
            frame_times: VecDeque::with_capacity(MAX_SAMPLES),
            input_latencies: VecDeque::with_capacity(MAX_SAMPLES),
            last_frame_start: None,
            last_input_time: None,
            total_frames: 0,
            start_time: Instant::now(),
            enabled: false,
        }
    }

    /// Enable or disable metrics collection
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.reset();
        }
    }

    /// Check if metrics are enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Reset all metrics
    pub fn reset(&mut self) {
        self.frame_times.clear();
        self.input_latencies.clear();
        self.last_frame_start = None;
        self.last_input_time = None;
        self.total_frames = 0;
        self.start_time = Instant::now();
    }

    /// Record start of frame
    pub fn frame_start(&mut self) {
        if self.enabled {
            self.last_frame_start = Some(Instant::now());
        }
    }

    /// Record end of frame
    pub fn frame_end(&mut self) {
        if !self.enabled {
            return;
        }

        if let Some(start) = self.last_frame_start.take() {
            let elapsed = start.elapsed().as_nanos() as u64;
            
            if self.frame_times.len() >= MAX_SAMPLES {
                self.frame_times.pop_front();
            }
            self.frame_times.push_back(elapsed);
            self.total_frames += 1;
        }
    }

    /// Record input event time
    pub fn input_received(&mut self) {
        if self.enabled {
            self.last_input_time = Some(Instant::now());
        }
    }

    /// Record when input was rendered/processed
    pub fn input_rendered(&mut self) {
        if !self.enabled {
            return;
        }

        if let Some(input_time) = self.last_input_time.take() {
            let latency = input_time.elapsed().as_nanos() as u64;
            
            if self.input_latencies.len() >= MAX_SAMPLES {
                self.input_latencies.pop_front();
            }
            self.input_latencies.push_back(latency);
        }
    }

    /// Get average frame time in milliseconds
    pub fn avg_frame_time_ms(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.frame_times.iter().sum();
        (sum as f64 / self.frame_times.len() as f64) / 1_000_000.0
    }

    /// Get minimum frame time in milliseconds
    pub fn min_frame_time_ms(&self) -> f64 {
        self.frame_times
            .iter()
            .min()
            .map(|&ns| ns as f64 / 1_000_000.0)
            .unwrap_or(0.0)
    }

    /// Get maximum frame time in milliseconds
    pub fn max_frame_time_ms(&self) -> f64 {
        self.frame_times
            .iter()
            .max()
            .map(|&ns| ns as f64 / 1_000_000.0)
            .unwrap_or(0.0)
    }

    /// Get average input latency in milliseconds
    pub fn avg_input_latency_ms(&self) -> f64 {
        if self.input_latencies.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.input_latencies.iter().sum();
        (sum as f64 / self.input_latencies.len() as f64) / 1_000_000.0
    }

    /// Get frames per second (based on recent frame times)
    pub fn fps(&self) -> f64 {
        let avg_ms = self.avg_frame_time_ms();
        if avg_ms > 0.0 {
            1000.0 / avg_ms
        } else {
            0.0
        }
    }

    /// Get total frames rendered
    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    /// Get uptime
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get current memory usage estimate in bytes
    /// Note: This is an estimate based on process memory, not precise
    #[cfg(unix)]
    pub fn memory_usage_bytes(&self) -> usize {
        use std::fs;
        
        // Read from /proc/self/statm on Linux
        if let Ok(content) = fs::read_to_string("/proc/self/statm") {
            if let Some(rss) = content.split_whitespace().nth(1) {
                if let Ok(pages) = rss.parse::<usize>() {
                    // Multiply by page size (typically 4096)
                    return pages * 4096;
                }
            }
        }
        
        // macOS fallback - use rusage
        unsafe {
            let mut usage: libc::rusage = std::mem::zeroed();
            if libc::getrusage(libc::RUSAGE_SELF, &mut usage) == 0 {
                // maxrss is in kilobytes on macOS
                return (usage.ru_maxrss as usize) * 1024;
            }
        }
        
        0
    }

    #[cfg(not(unix))]
    pub fn memory_usage_bytes(&self) -> usize {
        0 // Not implemented for non-Unix
    }

    /// Get memory usage in megabytes
    pub fn memory_usage_mb(&self) -> f64 {
        self.memory_usage_bytes() as f64 / (1024.0 * 1024.0)
    }

    /// Generate a metrics summary string
    pub fn summary(&self) -> String {
        if !self.enabled {
            return "Metrics disabled".to_string();
        }

        format!(
            "FPS: {:.1} | Frame: {:.2}ms (min {:.2}, max {:.2}) | Input latency: {:.2}ms | Memory: {:.1}MB | Frames: {} | Uptime: {:?}",
            self.fps(),
            self.avg_frame_time_ms(),
            self.min_frame_time_ms(),
            self.max_frame_time_ms(),
            self.avg_input_latency_ms(),
            self.memory_usage_mb(),
            self.total_frames,
            self.uptime(),
        )
    }

    /// Check if current performance meets spec targets
    pub fn meets_targets(&self) -> MetricsReport {
        MetricsReport {
            frame_time_ok: self.avg_frame_time_ms() < 1.0,
            input_latency_ok: self.avg_input_latency_ms() < 5.0,
            memory_ok: self.memory_usage_mb() < 100.0,
            fps: self.fps(),
            avg_frame_time_ms: self.avg_frame_time_ms(),
            avg_input_latency_ms: self.avg_input_latency_ms(),
            memory_mb: self.memory_usage_mb(),
        }
    }
}

/// Report on whether metrics meet spec targets
#[derive(Debug, Clone)]
pub struct MetricsReport {
    /// Frame time < 1ms
    pub frame_time_ok: bool,
    /// Input latency < 5ms
    pub input_latency_ok: bool,
    /// Memory < 100MB
    pub memory_ok: bool,
    /// Current FPS
    pub fps: f64,
    /// Average frame time
    pub avg_frame_time_ms: f64,
    /// Average input latency
    pub avg_input_latency_ms: f64,
    /// Memory usage in MB
    pub memory_mb: f64,
}

impl MetricsReport {
    /// Check if all targets are met
    pub fn all_ok(&self) -> bool {
        self.frame_time_ok && self.input_latency_ok && self.memory_ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn metrics_disabled_by_default() {
        let metrics = Metrics::new();
        assert!(!metrics.is_enabled());
    }

    #[test]
    fn metrics_enable() {
        let mut metrics = Metrics::new();
        metrics.set_enabled(true);
        assert!(metrics.is_enabled());
    }

    #[test]
    fn metrics_frame_time() {
        let mut metrics = Metrics::new();
        metrics.set_enabled(true);

        for _ in 0..10 {
            metrics.frame_start();
            sleep(Duration::from_micros(100)); // ~0.1ms
            metrics.frame_end();
        }

        assert!(metrics.avg_frame_time_ms() > 0.0);
        assert_eq!(metrics.total_frames(), 10);
    }

    #[test]
    fn metrics_input_latency() {
        let mut metrics = Metrics::new();
        metrics.set_enabled(true);

        metrics.input_received();
        sleep(Duration::from_micros(500)); // ~0.5ms
        metrics.input_rendered();

        assert!(metrics.avg_input_latency_ms() > 0.0);
    }

    #[test]
    fn metrics_fps() {
        let mut metrics = Metrics::new();
        metrics.set_enabled(true);

        // Simulate 60fps (16.67ms frames)
        for _ in 0..60 {
            metrics.frame_start();
            sleep(Duration::from_millis(1));
            metrics.frame_end();
        }

        // FPS should be roughly proportional to frame time
        assert!(metrics.fps() > 0.0);
    }

    #[test]
    fn metrics_summary() {
        let metrics = Metrics::new();
        let summary = metrics.summary();
        assert!(summary.contains("disabled"));
    }

    #[test]
    fn metrics_report() {
        let mut metrics = Metrics::new();
        metrics.set_enabled(true);

        metrics.frame_start();
        metrics.frame_end();

        let report = metrics.meets_targets();
        // With nearly instant frame, should pass frame time target
        assert!(report.frame_time_ok);
    }

    #[test]
    fn metrics_reset() {
        let mut metrics = Metrics::new();
        metrics.set_enabled(true);

        for _ in 0..10 {
            metrics.frame_start();
            metrics.frame_end();
        }

        assert_eq!(metrics.total_frames(), 10);

        metrics.reset();
        assert_eq!(metrics.total_frames(), 0);
    }
}
