mod auth;
mod handlers;
mod pulse;
mod timeline;
mod tray;

use chrono::{Local, Timelike};
use database as db;
use event_bus::EventBus;
use monitor::ActivityMonitor;
use protocol::*;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Response, Server, StatusCode};

type Db = Arc<Mutex<rusqlite::Connection>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Setup
    let port = std::env::var("PORT").unwrap_or("8080".into());
    let db_path = std::env::var("DB_PATH").unwrap_or("data/db.sqlite3".into());
    let files_dir = std::env::var("FILES_DIR").unwrap_or("data/files".into());

    std::fs::create_dir_all("data").ok();
    std::fs::create_dir_all(&files_dir).ok();

    let conn = db::open(&db_path)?;

    // Initialize tables
    pulse::create_table(&conn).ok();
    timeline::create_table(&conn).ok();

    let db: Db = Arc::new(Mutex::new(conn));
    let bus = Arc::new(EventBus::new());

    // 2. Start Activity Monitor for the default admin (id=1)
    let monitor = Arc::new(ActivityMonitor::new(bus.clone(), 1));
    monitor.run().await;

    // 3. Start Event Processor (Background save to DB)
    let db_for_events = db.clone();
    let mut receiver = bus.subscribe();
    tokio::spawn(async move {
        let mut last_input_minute = -1i32;
        while let Ok(event) = receiver.recv().await {
            use event_bus::AppEvent;
            let conn = db_for_events.lock().unwrap();
            match event {
                AppEvent::WindowsActivityRecorded(a) => {
                    db::record_activity(
                        &conn,
                        a.user_id,
                        &a.process_name,
                        &a.window_title,
                        a.duration_s,
                        a.is_private,
                    )
                    .ok();
                    // Also record in Journal for the timeline view
                    let detail = if a.is_private {
                        "Private Activity".to_string()
                    } else {
                        format!("{}: {}", a.process_name, a.window_title)
                    };
                    crate::timeline::record(
                        &conn,
                        a.user_id,
                        "win_activity",
                        None,
                        &detail,
                        Some(a.duration_s),
                        None,
                    )
                    .ok();
                }
                AppEvent::InputMetricsRecorded(m) => {
                    db::record_input(&conn, m.user_id, m.key_count, m.mouse_distance_px).ok();

                    // Only record in Journal once per minute to avoid spam
                    let current_minute = Local::now().minute() as i32;
                    if current_minute != last_input_minute {
                        let detail = format!("Activity: {} keys (pulse)", m.key_count);
                        crate::timeline::record(
                            &conn,
                            m.user_id,
                            "input_metrics",
                            None,
                            &detail,
                            None,
                            None,
                        )
                        .ok();
                        last_input_minute = current_minute;
                    }
                }
                _ => {}
            }
        }
    });

    // 4. Start Pulse Worker
    pulse::pulse_worker(db.clone());

    // 4. Start HTTP Server
    let addr = format!("0.0.0.0:{}", port);
    let server = Server::http(&addr).expect("Failed to start server");

    println!("Server running at http://{}", addr);

    let db_clone = db.clone();
    let files_dir_clone = files_dir.clone();

    // 5. Start Tray Icon in a separate thread (actually should be main on Windows, but let's try thread)
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();
    let port_val = port.parse::<u16>().unwrap_or(8080);
    std::thread::spawn(move || {
        crate::tray::run_tray(running_clone, port_val);
    });

    for mut request in server.incoming_requests() {
        let db = db_clone.clone();
        let monitor = monitor.clone();
        let _files_dir = files_dir_clone.clone();
        std::thread::spawn(move || {
            let url = request.url().to_string();
            let method = request.method().as_str().to_uppercase();

            if method == "OPTIONS" {
                let _ = request.respond(cors_ok());
                return;
            }

            let path: Vec<&str> = url.split('?')
                .next()
                .unwrap_or("")
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();

            let resp = match (method.as_str(), path.as_slice()) {
                ("GET", []) | ("GET", ["index.html"]) => serve_static(
                    "text/html; charset=utf-8",
                    include_str!("../../../static/index.html"),
                ),
                ("POST", ["auth", "login"]) => handlers::handle_login(&mut request, &db),
                ("POST", ["auth", "privacy"]) => {
                    handlers::handle_toggle_privacy(&mut request, &db, &monitor)
                }
                ("POST", ["reflection"]) => handlers::handle_submit_reflection(&mut request, &db),

                // KB
                ("GET", ["kb", "list"]) | ("GET", ["kb"]) => handlers::handle_kb_list(&db),
                ("GET", ["kb", "get", ..]) | ("GET", ["kb", "note", ..]) => {
                    handlers::handle_kb_get(&path[2..], &db)
                }
                ("POST", ["kb", "save"]) => handlers::handle_kb_save(&mut request, &db),
                ("POST", ["kb", "delete", ..]) => handlers::handle_kb_delete(&path[2..], &db),
                ("GET", ["kb", "graph"]) => handlers::handle_kb_graph(&db),
                ("POST", ["kb", "link"]) => handlers::handle_kb_link(&mut request, &db),

                ("GET", ["tasks"]) => handlers::handle_list_tasks(&request, &db),
                ("POST", ["tasks"]) => handlers::handle_create_task(&mut request, &db),
                ("PUT", ["tasks", id]) => {
                    if let Ok(id_val) = id.parse::<i64>() {
                        handlers::handle_update_task(&mut request, &db, id_val)
                    } else {
                        json_resp(400, &ApiError::json("Invalid ID"))
                    }
                }

                ("POST", ["time", "start"]) | ("POST", ["timer", "start"]) => {
                    handlers::handle_start_timer(&mut request, &db)
                }
                ("POST", ["time", "stop", id]) | ("POST", ["timer", "stop", id]) => {
                    if let Ok(id_val) = id.parse::<i64>() {
                        handlers::handle_stop_timer(&request, &db, id_val)
                    } else {
                        json_resp(400, &ApiError::json("Invalid ID"))
                    }
                }
                ("GET", ["time", "report"]) | ("GET", ["timer", "report"]) => {
                    handlers::handle_time_report(&request, &db)
                }

                ("GET", ["journal", "report"]) => handlers::handle_journal_report(&request, &db),
                ("GET", ["pulse", "pending"]) => handlers::handle_pulse_pending(&request, &db),
                ("GET", ["monitor", "metrics"]) => handlers::handle_monitor_metrics(&request, &db),

                ("GET", ["users"]) => handlers::handle_list_users(&request, &db),
                ("POST", ["users"]) => handlers::handle_create_user(&mut request, &db),
                ("PUT", ["users", "password"]) => {
                    handlers::handle_change_password(&mut request, &db)
                }

                ("GET", ["chat"]) | ("POST", ["chat"]) => handlers::handle_chat(&mut request, &db),
                ("GET", ["files"]) | ("POST", ["files", "upload"]) => {
                    handlers::handle_files(&mut request, &db, &_files_dir)
                }
                ("GET", ["files", _id]) => {
                    json_resp(200, r#"{"info":"Download not implemented via HTTP yet"}"#)
                }

                // Static libraries (Offline support)
                ("GET", ["lib", "marked.min.js"]) => serve_static(
                    "application/javascript",
                    include_str!("../../../static/lib/marked.min.js"),
                ),
                ("GET", ["lib", "mermaid.min.js"]) => serve_static(
                    "application/javascript",
                    include_str!("../../../static/lib/mermaid.min.js"),
                ),

                _ => json_resp(404, &ApiError::json("Route not found")),
            };
            let _ = request.respond(resp);
        });
    }
    Ok(())
}

// Helpers (abstracted)
type Resp = Response<Cursor<Vec<u8>>>;

fn json_resp(code: u16, body: &str) -> Resp {
    let data = body.as_bytes().to_vec();
    Response::new(
        StatusCode(code),
        vec![
            Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap(),
            Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
        ],
        Cursor::new(data.clone()),
        Some(data.len()),
        None,
    )
}

fn serve_static(content_type: &str, body: &str) -> Resp {
    let data = body.as_bytes().to_vec();
    Response::new(
        StatusCode(200),
        vec![Header::from_bytes("Content-Type", content_type).unwrap()],
        Cursor::new(data.clone()),
        Some(data.len()),
        None,
    )
}

fn cors_ok() -> Resp {
    Response::new(
        StatusCode(204),
        vec![
            Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
            Header::from_bytes(
                "Access-Control-Allow-Methods",
                "GET,POST,PUT,DELETE,OPTIONS",
            )
            .unwrap(),
            Header::from_bytes(
                "Access-Control-Allow-Headers",
                "Authorization,Content-Type,X-Filename",
            )
            .unwrap(),
        ],
        Cursor::new(vec![]),
        Some(0),
        None,
    )
}
