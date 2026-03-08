use chrono::{Local, Timelike};
use event_bus::{AppEvent, EventBus};
use protocol::{InputMetrics, WindowsActivity};
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use windows_sys::Win32::Foundation::MAX_PATH;
use windows_sys::Win32::System::ProcessStatus::GetModuleFileNameExW;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};

use std::sync::atomic::{AtomicBool, Ordering};

pub struct ActivityMonitor {
    event_bus: Arc<EventBus>,
    user_id: i64,
    privacy_enabled: Arc<AtomicBool>,
}

impl ActivityMonitor {
    pub fn new(event_bus: Arc<EventBus>, user_id: i64) -> Self {
        Self {
            event_bus,
            user_id,
            privacy_enabled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_privacy(&self, enabled: bool) {
        self.privacy_enabled.store(enabled, Ordering::SeqCst);
    }

    pub async fn run(&self) {
        let bus = self.event_bus.clone();
        let uid = self.user_id;
        let privacy = self.privacy_enabled.clone();

        tokio::spawn(async move {
            let (mut last_title, mut last_process) = get_active_window_info();
            let mut start_time = Local::now();

            // Immediate first record
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
            let mut mouse_dist = 0i64;
            let mut key_count = 0i64;
            let mut last_minute_tick = Local::now().minute();

            loop {
                sleep(Duration::from_millis(100)).await;
                let now = Local::now();

                // 1. Window & Process Tracking
                let (title, process) = get_active_window_info();

                // If window changed OR 1 minute passed (heartbeat)
                let minute_changed = now.minute() != last_minute_tick;

                if title != last_title || process != last_process || minute_changed {
                    let duration = (now - start_time).num_seconds();

                    if duration > 0 && !last_process.is_empty() {
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
                    }

                    if title != last_title || process != last_process {
                        last_title = title;
                        last_process = process;
                        start_time = now;
                    } else if minute_changed {
                        // Heartbeat: just update start_time and last_minute_tick to avoid double-counting duration later
                        start_time = now;
                    }
                    last_minute_tick = now.minute();
                }

                // 2. Input Metrics
                unsafe {
                    let mut pos = std::mem::zeroed();
                    if GetCursorPos(&mut pos) != 0 {
                        if last_mouse_pos != (0, 0) {
                            let dx = (pos.x - last_mouse_pos.0) as f64;
                            let dy = (pos.y - last_mouse_pos.1) as f64;
                            mouse_dist += (dx * dx + dy * dy).sqrt() as i64;
                        }
                        last_mouse_pos = (pos.x, pos.y);
                    }
                }

                for vk in 0..256 {
                    unsafe {
                        if (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 {
                            key_count += 1;
                            break;
                        }
                    }
                }

                // Publish metrics on minute change OR every 5 seconds if changed
                let five_seconds_passed = now.timestamp() % 5 == 0 && (now.timestamp() % 60 != 0);

                if minute_changed && (key_count > 0 || mouse_dist > 0) {
                    bus.publish(AppEvent::InputMetricsRecorded(InputMetrics {
                        id: 0,
                        user_id: uid,
                        key_count,
                        mouse_distance_px: mouse_dist,
                        measured_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
                    }));
                    key_count = 0;
                    mouse_dist = 0;
                } else if five_seconds_passed && (key_count > 0 || mouse_dist > 0) {
                    // Intermediate update
                    bus.publish(AppEvent::InputMetricsRecorded(InputMetrics {
                        id: 0,
                        user_id: uid,
                        key_count,
                        mouse_distance_px: mouse_dist,
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
