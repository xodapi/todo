use chrono::{Local, Timelike};
use event_bus::{AppEvent, EventBus};
use protocol::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use windows_sys::Win32::Foundation::MAX_PATH;
use windows_sys::Win32::System::ProcessStatus::GetModuleFileNameExW;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};

pub struct ActivityMonitor {
    event_bus: Arc<EventBus>,
    user_id: i64,
    privacy_enabled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    mouse_dist: Arc<AtomicI64>,
    key_count: Arc<AtomicI64>,
}

impl ActivityMonitor {
    pub fn new(event_bus: Arc<EventBus>, user_id: i64) -> Self {
        Self {
            event_bus,
            user_id,
            privacy_enabled: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(true)),
            mouse_dist: Arc::new(AtomicI64::new(0)),
            key_count: Arc::new(AtomicI64::new(0)),
        }
    }

    pub fn set_privacy(&self, enabled: bool) {
        self.privacy_enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_privacy_enabled(&self) -> bool {
        self.privacy_enabled.load(Ordering::SeqCst)
    }

    pub fn get_metrics(&self) -> InputMetrics {
        InputMetrics {
            id: 0,
            user_id: self.user_id,
            key_count: self.key_count.load(Ordering::SeqCst),
            mouse_distance_px: self.mouse_dist.load(Ordering::SeqCst),
            measured_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    pub fn clear_counters(&self) {
        self.mouse_dist.store(0, Ordering::SeqCst);
        self.key_count.store(0, Ordering::SeqCst);
    }

    pub async fn run(&self) {
        let bus = self.event_bus.clone();
        let uid = self.user_id;
        let privacy = self.privacy_enabled.clone();
        let running = self.running.clone();
        let mouse_atomic = self.mouse_dist.clone();
        let keys_atomic = self.key_count.clone();

        tokio::spawn(async move {
            let (mut last_title, mut last_process) = get_active_window_info();
            let mut start_time = Local::now();

            if !last_process.is_empty() {
                let is_private = privacy.load(Ordering::SeqCst);
                bus.publish(AppEvent::WindowsActivityRecorded(WindowsActivity {
                    id: 0,
                    user_id: uid,
                    process_name: last_process.clone(),
                    window_title: if is_private {
                        "Private Activity".to_string()
                    } else {
                        last_title.clone()
                    },
                    started_at: start_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                    duration_s: 0,
                    is_private,
                }));
            }

            let mut last_mouse_pos = (0i32, 0i32);
            let mut last_minute_tick = Local::now().minute();

            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                sleep(Duration::from_millis(100)).await;
                let now = Local::now();

                // 1. Mouse
                unsafe {
                    let mut pos = std::mem::zeroed();
                    if GetCursorPos(&mut pos) != 0 {
                        if last_mouse_pos != (0, 0) {
                            let dx = (pos.x - last_mouse_pos.0) as f64;
                            let dy = (pos.y - last_mouse_pos.1) as f64;
                            let d = (dx * dx + dy * dy).sqrt() as i64;
                            if d > 2 {
                                mouse_atomic.fetch_add(d, Ordering::SeqCst);
                            }
                        }
                        last_mouse_pos = (pos.x, pos.y);
                    }
                }

                // 2. Keys
                for vk in 0..256 {
                    unsafe {
                        if (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 {
                            keys_atomic.fetch_add(1, Ordering::SeqCst);
                            break;
                        }
                    }
                }

                // 3. Activity (every 5s or on change)
                if now.second().is_multiple_of(5) {
                    let (title, proc) = get_active_window_info();
                    if proc != last_process || title != last_title {
                        let duration = (now - start_time).num_seconds();
                        if duration > 0 {
                            let is_private = privacy.load(Ordering::SeqCst);
                            bus.publish(AppEvent::WindowsActivityRecorded(WindowsActivity {
                                id: 0,
                                user_id: uid,
                                process_name: last_process.clone(),
                                window_title: if is_private {
                                    "Private Activity".to_string()
                                } else {
                                    last_title.clone()
                                },
                                started_at: start_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                                duration_s: duration,
                                is_private,
                            }));
                            last_title = title;
                            last_process = proc;
                            start_time = now;
                        }
                    }
                }

                // 4. Input Metrics (every minute)
                if now.minute() != last_minute_tick {
                    last_minute_tick = now.minute();
                    let keys = keys_atomic.load(Ordering::SeqCst);
                    let m_dist = mouse_atomic.load(Ordering::SeqCst);
                    bus.publish(AppEvent::InputMetricsRecorded(InputMetrics {
                        id: 0,
                        user_id: uid,
                        key_count: keys,
                        mouse_distance_px: m_dist,
                        measured_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
                    }));
                }
            }
        });
    }
}

fn get_active_window_info() -> (String, String) {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return ("".into(), "".into());
        }

        let mut buffer = [0u16; 512];
        let len = GetWindowTextW(hwnd, buffer.as_mut_ptr(), 512);
        let title = String::from_utf16_lossy(&buffer[..len as usize]);

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        let mut process_name = "unknown".to_string();
        if !handle.is_null() {
            let mut proc_buffer = [0u16; MAX_PATH as usize];
            let len = GetModuleFileNameExW(
                handle,
                std::ptr::null_mut(),
                proc_buffer.as_mut_ptr(),
                MAX_PATH,
            );
            if len > 0 {
                let full_path = String::from_utf16_lossy(&proc_buffer[..len as usize]);
                process_name = full_path
                    .split('\\')
                    .next_back()
                    .unwrap_or("unknown")
                    .to_string();
            }
            windows_sys::Win32::Foundation::CloseHandle(handle);
        }

        (title, process_name)
    }
}
