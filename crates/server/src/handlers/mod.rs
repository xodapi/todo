use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
    http::StatusCode,
};
use crate::AppState;
use crate::auth;
use crate::pulse;
use database as db;
use protocol::*;
use std::sync::Arc;
use rusqlite::params;
use event_bus::AppEvent;
use tracing::info;

pub async fn handle_login(
    State(state): State<Arc<AppState>>,
    Json(login): Json<LoginRequest>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let mut stmt = match conn.prepare("SELECT id, username, pass_hash, role, full_name, created_at FROM users WHERE username = ?1") {
        Ok(s) => s,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("Database error"))).into_response(),
    };
    let mut rows = stmt.query(params![login.username]).unwrap();
    
    if let Some(row) = rows.next().unwrap() {
        let pass_hash: String = row.get(2).unwrap();
        if auth::verify_password(&login.password, &pass_hash) {
            let user_id: i64 = row.get(0).unwrap();
            let token = auth::create_session(&conn, user_id, "127.0.0.1", login.remember_me.unwrap_or(false)).unwrap();
            let csrf_token = uuid::Uuid::new_v4().to_string();
            
            let mut response = (StatusCode::OK, Json(LoginResponse {
                token,
                role: row.get::<usize, String>(3).unwrap(),
                username: row.get(1).unwrap(),
                user_id,
            })).into_response();

            response.headers_mut().insert(
                axum::http::header::SET_COOKIE,
                axum::http::HeaderValue::from_str(&format!("csrf_token={}; Path=/; HttpOnly; SameSite=Lax", csrf_token)).unwrap()
            );
            // Also send it in a header so the frontend can easily grab it and store it if needed
            response.headers_mut().insert(
                "X-CSRF-Token",
                axum::http::HeaderValue::from_str(&csrf_token).unwrap()
            );

            return response;
        }
    }
    
    (StatusCode::UNAUTHORIZED, Json(ApiError::new("Invalid credentials"))).into_response()
}

pub async fn handle_toggle_privacy(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let current = state.monitor.is_privacy_enabled();
    state.monitor.set_privacy(!current);
    Json(serde_json::json!({"success": true, "private": !current}))
}

pub async fn handle_monitor_metrics(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let metrics = state.monitor.get_metrics();
    Json(metrics)
}

pub async fn handle_clear_metrics(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    state.monitor.clear_counters();
    let conn = state.db.lock().unwrap();
    conn.execute("DELETE FROM input_metrics WHERE user_id = 1", []).ok();
    conn.execute("DELETE FROM windows_activity WHERE user_id = 1", []).ok();
    Json(serde_json::json!({"success": true}))
}

pub async fn handle_shutdown(
    State(_state): State<Arc<AppState>>,
) -> impl IntoResponse {
    info!("Shutdown requested");
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        std::process::exit(0);
    });
    Json(serde_json::json!({"success": true}))
}

pub async fn handle_journal_report(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let entries = db::get_journal_entries(&conn, 1).unwrap_or_default();
    Json(entries)
}

pub async fn handle_stop_monitoring(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    state.monitor.stop();
    Json(serde_json::json!({"success": true}))
}

pub async fn handle_list_tasks(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let tasks = db::list_tasks(&conn, None, None).unwrap_or_default();
    Json(tasks)
}

pub async fn handle_create_task(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    match db::create_task(&conn, &req, 1) {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({"id": id}))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("Failed to create task"))).into_response(),
    }
}

pub async fn handle_update_task(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateTaskRequest>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    match db::update_task(&conn, id, &req, 1, &Role::Admin) {
        Ok(_) => Json(serde_json::json!({"success": true})).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("Failed to update task"))).into_response(),
    }
}

pub async fn handle_pulse_pending(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let pending = pulse::get_pending(&conn, 1).unwrap_or(None);
    Json(pending)
}

pub async fn handle_list_users(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let users = db::list_users(&conn).unwrap_or_default();
    Json(users)
}
pub async fn handle_list_messages(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let since = params.get("since").map(|s| s.as_str());
    let msgs = db::list_messages(&conn, since).unwrap_or_default();
    Json(msgs)
}

pub async fn handle_send_message(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    // In a real app we'd get user_id from session/claims
    let user_id = 1; 
    match db::create_message(&conn, user_id, &req.body, req.task_id) {
        Ok(id) => {
            if let Ok(Some(msg)) = db::get_message(&conn, id) {
                state.bus.publish(AppEvent::ChatMessageSent(JournalEntry {
                    id: msg.id,
                    user_id: msg.user_id,
                    username: msg.username,
                    event_type: "chat".to_string(),
                    task_id: msg.task_id,
                    task_title: None, // Need to fetch if needed
                    detail: msg.body,
                    duration_s: None,
                    category: None,
                    happened_at: msg.sent_at,
                }));
            }
            Json(serde_json::json!({"success": true, "id": id})).into_response()
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("Failed to send message"))).into_response(),
    }
}

// --- Knowledge Base Handlers ---

pub async fn handle_list_kb_notes(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let notes = db::list_notes(&conn, 1, false).unwrap_or_default();
    Json(notes)
}

pub async fn handle_create_kb_note(
    State(state): State<Arc<AppState>>,
    Json(req): Json<KnowledgeNote>, // Reusing KnowledgeNote for simplicity or a subset if needed
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    match db::create_note(&conn, 1, req.parent_id, &req.title, &req.content, &req.aliases) {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({"id": id}))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("Failed to create note"))).into_response(),
    }
}

pub async fn handle_get_kb_note(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    match db::get_note(&conn, id) {
        Ok(note) => Json(note).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(ApiError::new("Note not found"))).into_response(),
    }
}

pub async fn handle_update_kb_note(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<KnowledgeNote>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    match db::update_note(&conn, id, &req.title, &req.content, req.parent_id, &req.aliases, req.is_archived) {
        Ok(_) => Json(serde_json::json!({"success": true})).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("Failed to update note"))).into_response(),
    }
}

pub async fn handle_delete_kb_note(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    match db::delete_note(&conn, id) {
        Ok(_) => Json(serde_json::json!({"success": true})).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("Failed to delete note"))).into_response(),
    }
}

pub async fn handle_kb_graph(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let graph = db::get_kb_graph(&conn).unwrap_or(KbGraphData { nodes: vec![], edges: vec![] });
    Json(graph)
}

pub async fn handle_list_kb_tags(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let tags = db::list_tags(&conn).unwrap_or_default();
    Json(tags)
}

// --- File Handlers ---

pub async fn handle_list_files(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let files = db::list_files(&conn, None).unwrap_or_default();
    Json(files)
}

pub async fn handle_upload_file(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let filename = headers.get("X-Filename")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unnamed");
    
    let stored_name = format!("{}_{}", uuid::Uuid::new_v4(), filename);
    let path = std::path::Path::new(&state.files_dir).join(&stored_name);
    
    if let Err(e) = std::fs::write(&path, &body) {
        info!("Failed to write file: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("Failed to save file"))).into_response();
    }

    let conn = state.db.lock().unwrap();
    match db::register_file(&conn, None, 1, filename, &stored_name, body.len() as i64) {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({"id": id}))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("Failed to register file"))).into_response(),
    }
}
