use axum::{
    extract::{State, WebSocketUpgrade},
    extract::ws::{Message, WebSocket},
    response::IntoResponse,
    routing::{get, post, put, delete},
    Router,
    middleware::{self, Next},
};
use chrono::{Local, Timelike};
use database as db;
use event_bus::EventBus;
use monitor::ActivityMonitor;
use protocol::*;
use std::sync::{Arc, Mutex};
use tower_http::{cors::CorsLayer, services::ServeDir};
use axum::http::{Request, StatusCode, header};
use tracing::{info, Level};

mod auth;
mod handlers;
mod pulse;
mod timeline;
mod tray;

type Db = Arc<Mutex<rusqlite::Connection>>;

struct AppState {
    db: Db,
    monitor: Arc<ActivityMonitor>,
    bus: Arc<EventBus>,
    files_dir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "todo.db".to_string());
    let files_dir = std::env::var("FILES_DIR").unwrap_or_else(|_| "data/files".to_string());

    std::fs::create_dir_all("data").ok();
    std::fs::create_dir_all(&files_dir).ok();
    
    let conn = db::open(&db_path)?;
    // Initialize tables
    pulse::create_table(&conn).ok();
    timeline::create_table(&conn).ok();

    let db = Arc::new(Mutex::new(conn));
    let bus = Arc::new(EventBus::new());
    let monitor = Arc::new(ActivityMonitor::new(bus.clone(), 1));

    let state = Arc::new(AppState {
        db: db.clone(),
        monitor: monitor.clone(),
        bus: bus.clone(),
        files_dir: files_dir.clone(),
    });

    // 1. Start Activity Monitor
    let m_clone = monitor.clone();
    tokio::spawn(async move {
        m_clone.run().await;
    });

    // 2. Start Event Processor (Background save to DB)
    let db_for_events = db.clone();
    let mut receiver = bus.subscribe();
    tokio::spawn(async move {
        let mut last_input_minute = -1i32;
        while let Ok(event) = receiver.recv().await {
            use event_bus::AppEvent;
            let conn = db_for_events.lock().unwrap();
            match event {
                AppEvent::WindowsActivityRecorded(a) => {
                    db::record_activity(&conn, a.user_id, &a.process_name, &a.window_title, a.duration_s, a.is_private).ok();
                    let detail = if a.is_private { "Private Activity".into() } else { format!("{}: {}", a.process_name, a.window_title) };
                    timeline::record(&conn, a.user_id, "win_activity", None, &detail, Some(a.duration_s), None).ok();
                }
                AppEvent::InputMetricsRecorded(m) => {
                    db::record_input(&conn, m.user_id, m.key_count, m.mouse_distance_px).ok();
                    let current_minute = Local::now().minute() as i32;
                    if current_minute != last_input_minute {
                        let detail = format!("Activity: {} keys (pulse)", m.key_count);
                        timeline::record(&conn, m.user_id, "input_metrics", None, &detail, None, None).ok();
                        last_input_minute = current_minute;
                    }
                }
                _ => {}
            }
        }
    });

    // 3. Start Pulse Worker
    pulse::pulse_worker(db.clone());

    // 4. Start Tray Icon (Threaded)
    let m_tray = monitor.clone();
    let port_val = port.parse::<u16>().unwrap_or(8080);
    std::thread::spawn(move || {
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        tray::run_tray(running, port_val);
    });

    // 5. Axum Server
    let app = Router::new()
        .nest_service("/", ServeDir::new("static"))
        .route("/ws", get(ws_handler))
        .route("/auth/login", post(handlers::handle_login))
        .route("/auth/privacy", post(handlers::handle_toggle_privacy))
        .route("/monitor/metrics", get(handlers::handle_monitor_metrics))
        .route("/monitor/stop", post(handlers::handle_stop_monitoring))
        .route("/monitor/clear", post(handlers::handle_clear_metrics))
        .route("/admin/shutdown", post(handlers::handle_shutdown))
        .route("/journal/report", get(handlers::handle_journal_report))
        .route("/chat", get(handlers::handle_list_messages).post(handlers::handle_send_message))
        .route("/users", get(handlers::handle_list_users))
        .route("/tasks", get(handlers::handle_list_tasks).post(handlers::handle_create_task))
        .route("/tasks/:id", put(handlers::handle_update_task))
        .route("/kb/notes", get(handlers::handle_list_kb_notes).post(handlers::handle_create_kb_note))
        .route("/kb/notes/:id", get(handlers::handle_get_kb_note).put(handlers::handle_update_kb_note).delete(handlers::handle_delete_kb_note))
        .route("/kb/graph", get(handlers::handle_kb_graph))
        .route("/kb/tags", get(handlers::handle_list_kb_tags))
        .route("/files", get(handlers::handle_list_files))
        .route("/files/upload", post(handlers::handle_upload_file))
        .layer(middleware::from_fn(csrf_middleware))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("Server running on http://localhost:{}", port);
    axum::serve(listener, app).await?;

    Ok(())
}

async fn csrf_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<axum::response::Response, StatusCode> {
    let method = req.method();
    let path = req.uri().path();
    if method == "GET" || method == "HEAD" || method == "OPTIONS" || path == "/auth/login" {
        return Ok(next.run(req).await);
    }

    let cookie_token = req.headers()
        .get(header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| {
            s.split(';')
                .find(|part| part.trim().starts_with("csrf_token="))
                .map(|part| part.trim()["csrf_token=".len()..].to_string())
        });

    let header_token = req.headers()
        .get("X-CSRF-Token")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    match (cookie_token, header_token) {
        (Some(c), Some(h)) if c == h => Ok(next.run(req).await),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.bus.subscribe();
    while let Ok(event) = rx.recv().await {
        let msg = match serde_json::to_string(&event) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if socket.send(Message::Text(msg)).await.is_err() {
            break;
        }
    }
}
