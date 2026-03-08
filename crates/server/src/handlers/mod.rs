pub mod utils;
use crate::Resp;
use crate::auth;
use crate::db;
use crate::json_resp;
use monitor::ActivityMonitor;
use protocol::*;
use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};
use utils::*;

pub fn handle_login(req: &mut tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let mut body = String::new();
    req.as_reader().read_to_string(&mut body).ok();
    let login_req: LoginRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(_) => return json_resp(400, &ApiError::json("Invalid JSON")),
    };

    let conn = db.lock().unwrap();
    let hash = auth::hash_password(&login_req.password);
    match db::find_user_by_credentials(&conn, &login_req.username, &hash) {
        Ok(Some(user)) => {
            let ip = req.remote_addr().map(|a| a.to_string()).unwrap_or_default();
            let token = auth::create_session(&conn, user.id, &ip).unwrap_or_default();
            json_resp(
                200,
                &serde_json::to_string(&LoginResponse {
                    token,
                    role: user.role.as_str().to_string(),
                    username: user.username,
                    user_id: user.id,
                })
                .unwrap(),
            )
        }
        _ => json_resp(401, &ApiError::json("Invalid credentials")),
    }
}

pub fn handle_list_tasks(req: &tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let conn = db.lock().unwrap();
    let _user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let params = query_params(req.url());
    let status = params.get("status").map(|s| s.as_str());

    match db::list_tasks(&conn, status, None) {
        Ok(tasks) => json_resp(200, &serde_json::to_string(&tasks).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_create_task(req: &mut tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let mut body_str = String::new();
    req.as_reader().read_to_string(&mut body_str).ok();
    let create_req: CreateTaskRequest = match serde_json::from_str(&body_str) {
        Ok(r) => r,
        Err(_) => return json_resp(400, &ApiError::json("Invalid JSON")),
    };

    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };

    match db::create_task(&conn, &create_req, user.id) {
        Ok(id) => json_resp(201, &serde_json::json!({ "id": id }).to_string()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_start_timer(req: &mut tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let mut body_str = String::new();
    req.as_reader().read_to_string(&mut body_str).ok();
    let start_req: StartTimerRequest = match serde_json::from_str(&body_str) {
        Ok(r) => r,
        Err(_) => return json_resp(400, &ApiError::json("Invalid JSON")),
    };

    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };

    match db::start_timer(
        &conn,
        user.id,
        start_req.task_id,
        start_req.category,
        start_req.note.as_deref(),
    ) {
        Ok(id) => {
            // Also record in timeline
            crate::timeline::on_timer_start(
                &conn,
                user.id,
                start_req.task_id,
                "Active session",
                start_req.category,
            )
            .ok();
            json_resp(201, &serde_json::json!({ "id": id }).to_string())
        }
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_stop_timer(req: &tiny_http::Request, db: &Arc<Mutex<Connection>>, id: i64) -> Resp {
    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };

    match db::stop_timer(&conn, id, user.id) {
        Ok(true) => json_resp(200, &serde_json::json!({ "success": true }).to_string()),
        _ => json_resp(404, &ApiError::json("Timer not found or already stopped")),
    }
}

pub fn handle_journal_report(req: &tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let params = query_params(req.url());
    let date = params
        .get("date")
        .cloned()
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

    match crate::timeline::get_day_entries(&conn, user.id, &date) {
        Ok(entries) => json_resp(200, &serde_json::to_string(&entries).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_pulse_pending(req: &tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let conn = db.lock().unwrap();
    let _user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };

    match crate::pulse::get_pending(&conn, _user.id) {
        Ok(Some(q)) => json_resp(200, &serde_json::to_string(&q).unwrap()),
        Ok(None) => json_resp(200, "null"),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_toggle_privacy(
    req: &mut tiny_http::Request,
    db: &Arc<Mutex<Connection>>,
    monitor: &ActivityMonitor,
) -> Resp {
    let conn = db.lock().unwrap();
    let _user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };

    #[derive(serde::Deserialize)]
    struct PrivacyReq {
        enabled: bool,
    }
    let mut body = String::new();
    req.as_reader().read_to_string(&mut body).ok();
    if let Ok(p) = serde_json::from_str::<PrivacyReq>(&body) {
        monitor.set_privacy(p.enabled);
        json_resp(200, r#"{"success":true}"#)
    } else {
        json_resp(400, &ApiError::json("Invalid JSON"))
    }
}

pub fn handle_submit_reflection(req: &mut tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let mut body = String::new();
    req.as_reader().read_to_string(&mut body).ok();
    let r: SubmitReflectionRequest = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return json_resp(400, &ApiError::json("Invalid JSON")),
    };

    let conn = db.lock().unwrap();
    db::save_reflection(&conn, 1, &r.question, &r.answer).ok();
    crate::timeline::record(
        &conn,
        1,
        "reflection",
        None,
        &format!("Q: {}\nA: {}", r.question, r.answer),
        None,
        None,
    )
    .ok();

    json_resp(201, r#"{"success":true}"#)
}

// --- Knowledge Base Handlers ---

pub fn handle_kb_list(db: &Arc<Mutex<Connection>>) -> Resp {
    let conn = db.lock().unwrap();
    match db::list_notes(&conn, 1) {
        Ok(notes) => json_resp(200, &serde_json::to_string(&notes).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_kb_get(path_params: &[&str], db: &Arc<Mutex<Connection>>) -> Resp {
    let id: i64 = match path_params.first().and_then(|s| s.parse().ok()) {
        Some(v) => v,
        None => return json_resp(400, &ApiError::json("Missing ID")),
    };
    let conn = db.lock().unwrap();
    match db::get_note(&conn, id) {
        Ok(note) => json_resp(200, &serde_json::to_string(&note).unwrap()),
        Err(e) => json_resp(404, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_kb_save(req: &mut tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let mut body = String::new();
    req.as_reader().read_to_string(&mut body).ok();
    let note: KnowledgeNote = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return json_resp(400, &ApiError::json("Invalid JSON")),
    };

    let conn = db.lock().unwrap();
    if note.id == 0 {
        match db::create_note(&conn, 1, note.parent_id, &note.title, &note.content) {
            Ok(id) => json_resp(200, &format!("{{\"id\":{}}}", id)),
            Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
        }
    } else {
        match db::update_note(&conn, note.id, &note.title, &note.content, note.parent_id) {
            Ok(_) => json_resp(200, r#"{"success":true}"#),
            Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
        }
    }
}

pub fn handle_kb_delete(path_params: &[&str], db: &Arc<Mutex<Connection>>) -> Resp {
    let id: i64 = match path_params.first().and_then(|s| s.parse().ok()) {
        Some(v) => v,
        None => return json_resp(400, &ApiError::json("Missing ID")),
    };
    let conn = db.lock().unwrap();
    match db::delete_note(&conn, id) {
        Ok(_) => json_resp(200, r#"{"success":true}"#),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_kb_graph(db: &Arc<Mutex<Connection>>) -> Resp {
    let conn = db.lock().unwrap();
    match db::get_graph(&conn, 1) {
        Ok(graph) => json_resp(200, &serde_json::to_string(&graph).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_kb_link(req: &mut tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let mut body = String::new();
    req.as_reader().read_to_string(&mut body).ok();
    let link: NoteLink = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return json_resp(400, &ApiError::json("Invalid JSON")),
    };

    let conn = db.lock().unwrap();
    match db::add_link(&conn, link.source_id, link.target_id) {
        Ok(_) => json_resp(200, r#"{"success":true}"#),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_monitor_metrics(req: &tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };

    let metrics = conn.query_row(
        "SELECT key_count, mouse_distance_px FROM input_metrics WHERE user_id = ?1 ORDER BY measured_at DESC LIMIT 1",
        params![user.id],
        |row| Ok(serde_json::json!({
            "keys": row.get::<_, i64>(0)?,
            "mouse": row.get::<_, i64>(1)?,
        }))
    ).unwrap_or(serde_json::json!({ "keys": 0, "mouse": 0 }));

    let last_activity = conn.query_row(
        "SELECT process_name, window_title FROM windows_activity WHERE user_id = ?1 ORDER BY started_at DESC LIMIT 1",
        params![user.id],
        |row| Ok(serde_json::json!({
            "process": row.get::<_, String>(0)?,
            "window": row.get::<_, String>(1)?,
        }))
    ).unwrap_or(serde_json::json!({ "process": "—", "window": "—" }));

    json_resp(
        200,
        &serde_json::json!({
            "metrics": metrics,
            "last_activity": last_activity,
        })
        .to_string(),
    )
}

pub fn handle_logout(req: &tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let conn = db.lock().unwrap();
    if let Some(token) = auth::extract_token(req) {
        auth::destroy_session(&conn, &token).ok();
    }
    json_resp(200, r#"{"success":true}"#)
}

pub fn handle_list_users(req: &tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let conn = db.lock().unwrap();
    match auth::require_role(&conn, req, &Role::Admin) {
        Ok(_) => {}
        Err(r) => return r,
    }
    match db::list_users(&conn) {
        Ok(users) => json_resp(200, &serde_json::to_string(&users).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_create_user(req: &mut tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let mut body = String::new();
    req.as_reader().read_to_string(&mut body).ok();
    let conn = db.lock().unwrap();
    match auth::require_role(&conn, req, &Role::Admin) {
        Ok(_) => {}
        Err(r) => return r,
    }

    #[derive(serde::Deserialize)]
    struct CreateUser {
        username: String,
        full_name: String,
        password: String,
        role: String,
    }
    let u: CreateUser = match serde_json::from_str(&body) {
        Ok(u) => u,
        Err(_) => return json_resp(400, &ApiError::json("Invalid JSON")),
    };

    let hash = auth::hash_password(&u.password);
    match db::create_user(&conn, &u.username, &hash, &u.role, &u.full_name) {
        Ok(id) => json_resp(201, &serde_json::json!({ "id": id }).to_string()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_change_password(req: &mut tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let mut body = String::new();
    req.as_reader().read_to_string(&mut body).ok();
    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };

    #[derive(serde::Deserialize)]
    struct ChangePass {
        old_password: String,
        new_password: String,
    }
    let cp: ChangePass = match serde_json::from_str(&body) {
        Ok(u) => u,
        Err(_) => return json_resp(400, &ApiError::json("Invalid JSON")),
    };

    // Verify old pass (optional but good)
    let old_hash = auth::hash_password(&cp.old_password);
    if db::find_user_by_credentials(&conn, &user.username, &old_hash)
        .unwrap_or(None)
        .is_none()
    {
        return json_resp(401, &ApiError::json("Invalid old password"));
    }

    let new_hash = auth::hash_password(&cp.new_password);
    match db::update_password(&conn, user.id, &new_hash) {
        Ok(_) => json_resp(200, r#"{"success":true}"#),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_update_task(
    req: &mut tiny_http::Request,
    db: &Arc<Mutex<Connection>>,
    id: i64,
) -> Resp {
    let mut body = String::new();
    req.as_reader().read_to_string(&mut body).ok();
    let upd: UpdateTaskRequest = match serde_json::from_str(&body) {
        Ok(u) => u,
        Err(_) => return json_resp(400, &ApiError::json("Invalid JSON")),
    };

    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };

    match db::update_task(&conn, id, &upd, user.id, &user.role) {
        Ok(true) => json_resp(200, r#"{"success":true}"#),
        _ => json_resp(404, &ApiError::json("Task not found")),
    }
}

pub fn handle_time_report(req: &tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let params = query_params(req.url());
    let days = params
        .get("days")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(7);

    match db::get_time_report(&conn, user.id, days) {
        Ok(mut report) => {
            // Add extra fields for frontend
            let obj = report.as_object_mut().unwrap();
            let c1 = obj.get("1").and_then(|v| v.as_i64()).unwrap_or(0);
            let c2 = obj.get("2").and_then(|v| v.as_i64()).unwrap_or(0);
            let c3 = obj.get("3").and_then(|v| v.as_i64()).unwrap_or(0);
            let total = c1 + c2 + c3;
            let eff = if c1 + c2 > 0 {
                (c1 as f64 / (c1 + c2) as f64) * 100.0
            } else {
                0.0
            };

            let final_report = serde_json::json!({
                "cat1_seconds": c1, "cat2_seconds": c2, "cat3_seconds": c3,
                "total_seconds": total, "efficiency_pct": eff,
                "entries": db::get_messages(&conn, None, None).unwrap_or_default(), // Placeholder for logs
            });
            json_resp(200, &final_report.to_string())
        }
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

pub fn handle_chat(req: &mut tiny_http::Request, db: &Arc<Mutex<Connection>>) -> Resp {
    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };

    if req.method() == &tiny_http::Method::Get {
        let params = query_params(req.url());
        let since = params.get("since").cloned();
        match db::get_messages(&conn, None, since.as_deref()) {
            Ok(msgs) => json_resp(200, &serde_json::to_string(&msgs).unwrap()),
            Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
        }
    } else {
        let mut body = String::new();
        req.as_reader().read_to_string(&mut body).ok();
        let msg: SendMessageRequest = match serde_json::from_str(&body) {
            Ok(m) => m,
            Err(_) => return json_resp(400, &ApiError::json("Invalid JSON")),
        };
        match db::send_message(&conn, user.id, msg.task_id, &msg.body) {
            Ok(id) => json_resp(201, &serde_json::json!({ "id": id }).to_string()),
            Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
        }
    }
}

pub fn handle_files(
    req: &mut tiny_http::Request,
    db: &Arc<Mutex<Connection>>,
    files_dir: &str,
) -> Resp {
    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u) => u,
        Err(r) => return r,
    };

    if req.method() == &tiny_http::Method::Get {
        match db::list_files(&conn, None) {
            Ok(files) => json_resp(200, &serde_json::to_string(&files).unwrap()),
            Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
        }
    } else {
        // Upload
        let x_filename = req
            .headers()
            .iter()
            .find(|h| h.field.as_str().to_string().to_lowercase() == "x-filename")
            .map(|h| h.value.as_str().to_string())
            .unwrap_or_else(|| "upload.bin".to_string());

        let mut data = Vec::new();
        req.as_reader().read_to_end(&mut data).ok();
        let size = data.len() as i64;
        let stored_name = format!("{}_{}", chrono::Utc::now().timestamp(), x_filename);
        let path = std::path::Path::new(files_dir).join(&stored_name);

        if std::fs::write(&path, data).is_err() {
            return json_resp(500, &ApiError::json("Failed to write file"));
        }

        match db::register_file(&conn, None, user.id, &x_filename, &stored_name, size) {
            Ok(id) => json_resp(201, &serde_json::json!({ "id": id }).to_string()),
            Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
        }
    }
}
