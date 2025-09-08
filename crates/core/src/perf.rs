use std::time::{Duration, Instant};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Performance metrics tracker for the terminal
pub struct PerfMonitor {
    frame_times: Arc<Mutex<VecDeque<Duration>>>,
    input_latencies: Arc<Mutex<VecDeque<Duration>>>,
    render_times: Arc<Mutex<VecDeque<Duration>>>,
    max_samples: usize,
    enabled: bool,
}

#[derive(Debug, Clone)]
pub struct PerfStats {
    pub avg_frame_time_ms: f32,
    pub p99_frame_time_ms: f32,
    pub fps: f32,
    pub avg_input_latency_ms: f32,
    pub avg_render_time_ms: f32,
    pub memory_usage_mb: f32,
}

impl PerfMonitor {
    pub fn new() -> Self {
        Self {
            frame_times: Arc::new(Mutex::new(VecDeque::with_capacity(120))),
            input_latencies: Arc::new(Mutex::new(VecDeque::with_capacity(120))),
            render_times: Arc::new(Mutex::new(VecDeque::with_capacity(120))),
            max_samples: 120,
            enabled: cfg!(debug_assertions), // Enable in debug builds by default
        }
    }
    
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    pub fn record_frame(&self, duration: Duration) {
        if !self.enabled { return; }
        
        let mut times = self.frame_times.lock().unwrap();
        if times.len() >= self.max_samples {
            times.pop_front();
        }
        times.push_back(duration);
    }
    
    pub fn record_input_latency(&self, duration: Duration) {
        if !self.enabled { return; }
        
        let mut latencies = self.input_latencies.lock().unwrap();
        if latencies.len() >= self.max_samples {
            latencies.pop_front();
        }
        latencies.push_back(duration);
    }
    
    pub fn record_render(&self, duration: Duration) {
        if !self.enabled { return; }
        
        let mut times = self.render_times.lock().unwrap();
        if times.len() >= self.max_samples {
            times.pop_front();
        }
        times.push_back(duration);
    }
    
    pub fn get_stats(&self) -> PerfStats {
        let frame_times = self.frame_times.lock().unwrap();
        let input_latencies = self.input_latencies.lock().unwrap();
        let render_times = self.render_times.lock().unwrap();
        
        // Calculate frame time stats
        let avg_frame_time_ms = if !frame_times.is_empty() {
            let sum: Duration = frame_times.iter().sum();
            sum.as_secs_f32() * 1000.0 / frame_times.len() as f32
        } else {
            0.0
        };
        
        let p99_frame_time_ms = if !frame_times.is_empty() {
            let mut sorted: Vec<_> = frame_times.iter().cloned().collect();
            sorted.sort();
            let index = ((sorted.len() as f32 * 0.99) as usize).min(sorted.len() - 1);
            sorted[index].as_secs_f32() * 1000.0
        } else {
            0.0
        };
        
        let fps = if avg_frame_time_ms > 0.0 {
            1000.0 / avg_frame_time_ms
        } else {
            0.0
        };
        
        // Calculate input latency
        let avg_input_latency_ms = if !input_latencies.is_empty() {
            let sum: Duration = input_latencies.iter().sum();
            sum.as_secs_f32() * 1000.0 / input_latencies.len() as f32
        } else {
            0.0
        };
        
        // Calculate render time
        let avg_render_time_ms = if !render_times.is_empty() {
            let sum: Duration = render_times.iter().sum();
            sum.as_secs_f32() * 1000.0 / render_times.len() as f32
        } else {
            0.0
        };
        
        // Get memory usage (macOS specific)
        let memory_usage_mb = Self::get_memory_usage_mb();
        
        PerfStats {
            avg_frame_time_ms,
            p99_frame_time_ms,
            fps,
            avg_input_latency_ms,
            avg_render_time_ms,
            memory_usage_mb,
        }
    }
    
    #[cfg(target_os = "macos")]
    fn get_memory_usage_mb() -> f32 {
        use std::mem;
        use std::os::raw::c_int;
        
        #[repr(C)]
        struct TaskBasicInfo {
            virtual_size: u64,
            resident_size: u64,
            resident_size_max: u64,
            user_time: [i64; 2],
            system_time: [i64; 2],
            policy: c_int,
            suspend_count: c_int,
        }
        
        extern "C" {
            fn mach_task_self() -> u32;
            fn task_info(
                target_task: u32,
                flavor: c_int,
                task_info_out: *mut TaskBasicInfo,
                task_info_count: *mut u32,
            ) -> c_int;
        }
        
        const TASK_BASIC_INFO: c_int = 5;
        
        unsafe {
            let mut info: TaskBasicInfo = mem::zeroed();
            let mut count = mem::size_of::<TaskBasicInfo>() as u32 / 4;
            
            let result = task_info(
                mach_task_self(),
                TASK_BASIC_INFO,
                &mut info as *mut _ as *mut TaskBasicInfo,
                &mut count,
            );
            
            if result == 0 {
                (info.resident_size as f32) / (1024.0 * 1024.0)
            } else {
                0.0
            }
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    fn get_memory_usage_mb() -> f32 {
        // Placeholder for other platforms
        0.0
    }
}

/// Timer for measuring specific operations
pub struct PerfTimer {
    start: Instant,
    name: String,
}

impl PerfTimer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            name: name.into(),
        }
    }
    
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
    
    pub fn elapsed_ms(&self) -> f32 {
        self.elapsed().as_secs_f32() * 1000.0
    }
}

impl Drop for PerfTimer {
    fn drop(&mut self) {
        if cfg!(debug_assertions) {
            let elapsed = self.elapsed_ms();
            if elapsed > 1.0 {
                tracing::debug!("{} took {:.2}ms", self.name, elapsed);
            }
        }
    }
}

/// Macro for timing a block of code
#[macro_export]
macro_rules! perf_time {
    ($name:expr, $code:block) => {{
        let _timer = $crate::perf::PerfTimer::new($name);
        $code
    }};
}