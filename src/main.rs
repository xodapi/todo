mod models;
mod db;
mod auth;

use std::sync::{Arc, Mutex};
use std::io::{Cursor, Read};
use tiny_http::{Server, Request, Response, Header, StatusCode};
use rusqlite::Connection;
use serde_json::json;
use crate::models::*;

// ─── Тип соединения с БД (shared, thread-safe) ───────────────────────────────
type Db = Arc<Mutex<Connection>>;

fn main() {
    // Конфигурация из переменных окружения (или значения по умолчанию)
    let port     = std::env::var("PORT").unwrap_or("8080".into());
    let db_path  = std::env::var("DB_PATH").unwrap_or("data/db.sqlite3".into());
    let files_dir = std::env::var("FILES_DIR").unwrap_or("data/files".into());

    // Создать директории если нет
    std::fs::create_dir_all("data").ok();
    std::fs::create_dir_all(&files_dir).ok();

    let conn = db::open(&db_path).expect("Не удалось открыть БД");
    let db: Db = Arc::new(Mutex::new(conn));

    let addr = format!("0.0.0.0:{}", port);
    let server = Server::http(&addr).expect("Не удалось запустить сервер");

    println!("╔══════════════════════════════════════╗");
    println!("║  Локальная Система Управления        ║");
    println!("║  Адрес: http://{}       ║", addr);
    println!("║  БД:    {}              ║", db_path);
    println!("╚══════════════════════════════════════╝");
    println!("  По умолчанию: admin / admin");

    // Чистим истёкшие сессии при старте
    {
        let conn = db.lock().unwrap();
        let cleaned = auth::purge_expired_sessions(&conn).unwrap_or(0);
        if cleaned > 0 {
            println!("[AUTH] Удалено {} истёкших сессий", cleaned);
        }
    }

    // Обрабатываем запросы в пуле потоков (tiny_http поддерживает это нативно)
    let db_clone = Arc::clone(&db);
    let files_dir_clone = files_dir.clone();

    for request in server.incoming_requests() {
        let db = Arc::clone(&db_clone);
        let files_dir = files_dir_clone.clone();
        std::thread::spawn(move || {
            let response = handle_request(request, db, &files_dir);
            // response уже отправлен внутри handle_request
            let _ = response;
        });
    }
}

fn handle_request(mut req: Request, db: Db, files_dir: &str) {
    // CORS для локальной разработки (если нужно отключить — убрать)
    let url   = req.url().to_string();
    let method = req.method().as_str().to_uppercase();

    // Логирование
    println!("[{}] {} {}", chrono::Local::now().format("%H:%M:%S"), method, url);

    // OPTIONS preflight
    if method == "OPTIONS" {
        let _ = req.respond(cors_ok());
        return;
    }

    // Роутинг: разбираем путь
    let path: Vec<&str> = url.splitn(2, '?').next().unwrap_or("")
                              .split('/').filter(|s| !s.is_empty()).collect();

    let resp = match (method.as_str(), path.as_slice()) {
        // ── Статика ──────────────────────────────────────────────────────────
        ("GET", []) | ("GET", ["index.html"]) => {
            serve_static("text/html; charset=utf-8", include_str!("../static/index.html"))
        }

        // ── Аутентификация ───────────────────────────────────────────────────
        ("POST", ["auth", "login"])  => handle_login(&mut req, &db),
        ("POST", ["auth", "logout"]) => handle_logout(&req, &db),

        // ── Пользователи ─────────────────────────────────────────────────────
        ("GET",  ["users"])       => handle_list_users(&req, &db),
        ("POST", ["users"])       => handle_create_user(&mut req, &req, &db),
        ("PUT",  ["users", "password"]) => handle_change_password(&mut req, &db),

        // ── Задачи ───────────────────────────────────────────────────────────
        ("GET",  ["tasks"])       => handle_list_tasks(&req, &db),
        ("POST", ["tasks"])       => handle_create_task(&mut req, &db),
        ("GET",  ["tasks", id])   => handle_get_task(id, &req, &db),
        ("PUT",  ["tasks", id])   => handle_update_task(id, &mut req, &db),

        // ── Хронометраж ──────────────────────────────────────────────────────
        ("POST", ["time", "start"])    => handle_start_timer(&mut req, &db),
        ("POST", ["time", "stop", id]) => handle_stop_timer(id, &req, &db),
        ("GET",  ["time", "report"])   => handle_time_report(&req, &db),

        // ── Чат ──────────────────────────────────────────────────────────────
        ("GET",  ["chat"])  => handle_get_messages(&req, &db),
        ("POST", ["chat"])  => handle_send_message(&mut req, &db),

        // ── Файлы ────────────────────────────────────────────────────────────
        ("GET",  ["files"])      => handle_list_files(&req, &db),
        ("POST", ["files", "upload"]) => handle_upload_file(&mut req, &db, files_dir),
        ("GET",  ["files", id])  => handle_download_file(id, &req, &db, files_dir),

        // ── Аналитика ────────────────────────────────────────────────────────
        ("GET", ["analytics", "eisenhower"])  => handle_eisenhower(&req, &db),
        ("GET", ["analytics", "lyubishchev"]) => handle_lyubishchev_summary(&req, &db),

        _ => json_resp(404, &ApiError::json("Маршрут не найден")),
    };

    let _ = req.respond(resp);
}

// ─── Аутентификация ──────────────────────────────────────────────────────────

fn handle_login(req: &mut Request, db: &Db) -> Resp {
    let body: LoginRequest = match parse_json(req) {
        Ok(b)  => b,
        Err(e) => return json_resp(400, &ApiError::json(&e)),
    };
    let hash = auth::hash_password(&body.password);
    let conn = db.lock().unwrap();
    match db::find_user_by_credentials(&conn, &body.username, &hash) {
        Ok(Some(user)) => {
            let ip = req.remote_addr().map(|a| a.to_string()).unwrap_or_default();
            let token = auth::create_session(&conn, user.id, &ip).unwrap();
            let resp = LoginResponse {
                token, role: user.role.as_str().to_string(),
                username: user.username, user_id: user.id,
            };
            // Set-Cookie для браузера (HttpOnly)
            let body = serde_json::to_string(&resp).unwrap();
            let cookie = format!("token={}; Path=/; HttpOnly; SameSite=Strict", resp.token);
            response_with_cookie(200, &body, &cookie)
        }
        _ => json_resp(401, &ApiError::json("Неверный логин или пароль")),
    }
}

fn handle_logout(req: &Request, db: &Db) -> Resp {
    if let Some(token) = auth::extract_token(req) {
        let conn = db.lock().unwrap();
        auth::destroy_session(&conn, &token).ok();
    }
    json_resp(200, r#"{"ok":true}"#)
}

// ─── Пользователи ────────────────────────────────────────────────────────────

fn handle_list_users(req: &Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let _user = guard!(auth::require_user(&conn, req));
    match db::list_users(&conn) {
        Ok(users) => json_resp(200, &serde_json::to_string(&users).unwrap()),
        Err(e)    => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

fn handle_create_user(body_req: &mut Request, auth_req: &Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let actor = guard!(auth::require_role(&conn, auth_req, &Role::Admin));
    let _ = actor;

    #[derive(serde::Deserialize)]
    struct NewUser { username: String, password: String, role: String, full_name: String }
    let body: NewUser = match parse_json(body_req) {
        Ok(b)  => b,
        Err(e) => return json_resp(400, &ApiError::json(&e)),
    };
    let hash = auth::hash_password(&body.password);
    match db::create_user(&conn, &body.username, &hash, &body.role, &body.full_name) {
        Ok(id) => json_resp(201, &serde_json::to_string(&json!({"ok":true,"id":id})).unwrap()),
        Err(e) => json_resp(400, &ApiError::json(&e.to_string())),
    }
}

fn handle_change_password(req: &mut Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let user = guard!(auth::require_user(&conn, req));
    #[derive(serde::Deserialize)]
    struct Payload { old_password: String, new_password: String }
    let body: Payload = match parse_json(req) {
        Ok(b)  => b,
        Err(e) => return json_resp(400, &ApiError::json(&e)),
    };
    let old_hash = auth::hash_password(&body.old_password);
    match db::find_user_by_credentials(&conn, &user.username, &old_hash) {
        Ok(Some(_)) => {
            let new_hash = auth::hash_password(&body.new_password);
            db::update_password(&conn, user.id, &new_hash).ok();
            json_resp(200, r#"{"ok":true}"#)
        }
        _ => json_resp(403, &ApiError::json("Неверный текущий пароль")),
    }
}

// ─── Задачи ──────────────────────────────────────────────────────────────────

fn handle_list_tasks(req: &Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let _user = guard!(auth::require_user(&conn, req));
    let params = query_params(req.url());
    let status      = params.get("status").map(|s| s.as_str());
    let assigned_to = params.get("assigned").and_then(|s| s.parse().ok());
    match db::list_tasks(&conn, status, assigned_to) {
        Ok(tasks) => json_resp(200, &serde_json::to_string(&tasks).unwrap()),
        Err(e)    => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

fn handle_create_task(req: &mut Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let user = guard!(auth::require_role(&conn, req, &Role::Engineer));
    let body: CreateTaskRequest = match parse_json(req) {
        Ok(b)  => b,
        Err(e) => return json_resp(400, &ApiError::json(&e)),
    };
    match db::create_task(&conn, &body, user.id) {
        Ok(id) => json_resp(201, &serde_json::to_string(&json!({"ok":true,"id":id})).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

fn handle_get_task(id_str: &str, req: &Request, db: &Db) -> Resp {
    let id: i64 = match id_str.parse() {
        Ok(n)  => n,
        Err(_) => return json_resp(400, &ApiError::json("Неверный ID")),
    };
    let conn = db.lock().unwrap();
    let _user = guard!(auth::require_user(&conn, req));
    match db::get_task(&conn, id) {
        Ok(Some(t)) => json_resp(200, &serde_json::to_string(&t).unwrap()),
        Ok(None)    => json_resp(404, &ApiError::json("Задача не найдена")),
        Err(e)      => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

fn handle_update_task(id_str: &str, req: &mut Request, db: &Db) -> Resp {
    let id: i64 = match id_str.parse() {
        Ok(n)  => n,
        Err(_) => return json_resp(400, &ApiError::json("Неверный ID")),
    };
    let conn = db.lock().unwrap();
    let user = guard!(auth::require_role(&conn, req, &Role::Engineer));
    let body: UpdateTaskRequest = match parse_json(req) {
        Ok(b)  => b,
        Err(e) => return json_resp(400, &ApiError::json(&e)),
    };
    match db::update_task(&conn, id, &body, user.id, &user.role) {
        Ok(true)  => json_resp(200, r#"{"ok":true}"#),
        Ok(false) => json_resp(404, &ApiError::json("Задача не найдена")),
        Err(_)    => json_resp(403, &ApiError::json("Нет прав для этого действия")),
    }
}

// ─── Хронометраж ─────────────────────────────────────────────────────────────

fn handle_start_timer(req: &mut Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let user = guard!(auth::require_role(&conn, req, &Role::Engineer));
    let body: StartTimerRequest = match parse_json(req) {
        Ok(b)  => b,
        Err(e) => return json_resp(400, &ApiError::json(&e)),
    };
    match db::start_timer(&conn, user.id, body.task_id, body.category,
                          body.note.as_deref())
    {
        Ok(id) => json_resp(201, &serde_json::to_string(&json!({"ok":true,"log_id":id})).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

fn handle_stop_timer(id_str: &str, req: &Request, db: &Db) -> Resp {
    let id: i64 = match id_str.parse() {
        Ok(n)  => n,
        Err(_) => return json_resp(400, &ApiError::json("Неверный ID")),
    };
    let conn = db.lock().unwrap();
    let user = guard!(auth::require_role(&conn, req, &Role::Engineer));
    match db::stop_timer(&conn, id, user.id) {
        Ok(true)  => json_resp(200, r#"{"ok":true}"#),
        Ok(false) => json_resp(404, &ApiError::json("Запись не найдена или уже остановлена")),
        Err(e)    => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

fn handle_time_report(req: &Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let user = guard!(auth::require_user(&conn, req));
    let params = query_params(req.url());
    let days: i32 = params.get("days").and_then(|d| d.parse().ok()).unwrap_or(7);
    // engineer видит только себя; expert/admin — любого
    let uid: i64 = if user.role.can_approve() {
        params.get("user").and_then(|u| u.parse().ok()).unwrap_or(user.id)
    } else {
        user.id
    };
    match db::get_time_report(&conn, uid, days) {
        Ok(r)  => json_resp(200, &serde_json::to_string(&r).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

// ─── Чат ─────────────────────────────────────────────────────────────────────

fn handle_get_messages(req: &Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let _user = guard!(auth::require_user(&conn, req));
    let params = query_params(req.url());
    let task_id = params.get("task_id").and_then(|t| t.parse().ok());
    let since   = params.get("since").map(|s| s.as_str());
    match db::get_messages(&conn, task_id, since) {
        Ok(msgs) => json_resp(200, &serde_json::to_string(&msgs).unwrap()),
        Err(e)   => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

fn handle_send_message(req: &mut Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let user = guard!(auth::require_user(&conn, req));
    let body: SendMessageRequest = match parse_json(req) {
        Ok(b)  => b,
        Err(e) => return json_resp(400, &ApiError::json(&e)),
    };
    if body.body.trim().is_empty() {
        return json_resp(400, &ApiError::json("Пустое сообщение"));
    }
    match db::send_message(&conn, user.id, body.task_id, &body.body) {
        Ok(id) => json_resp(201, &serde_json::to_string(&json!({"ok":true,"id":id})).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

// ─── Файлы ───────────────────────────────────────────────────────────────────

fn handle_list_files(req: &Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let _user = guard!(auth::require_user(&conn, req));
    let params = query_params(req.url());
    let task_id = params.get("task_id").and_then(|t| t.parse().ok());
    match db::list_files(&conn, task_id) {
        Ok(files) => json_resp(200, &serde_json::to_string(&files).unwrap()),
        Err(e)    => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

fn handle_upload_file(req: &mut Request, db: &Db, files_dir: &str) -> Resp {
    let conn = db.lock().unwrap();
    let user = guard!(auth::require_role(&conn, req, &Role::Engineer));
    let params = query_params(req.url());
    let task_id: Option<i64> = params.get("task_id").and_then(|t| t.parse().ok());

    // Получаем оригинальное имя файла из заголовка X-Filename
    let filename = req.headers().iter()
        .find(|h| h.field.as_str().eq_ignore_ascii_case("x-filename"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_else(|| "file.bin".to_string());

    // Санируем имя файла
    let safe_name: String = filename.chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect();
    let ext = safe_name.rsplit('.').next().unwrap_or("bin");
    let stored = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let path = format!("{}/{}", files_dir, stored);

    let mut body = Vec::new();
    req.as_reader().read_to_end(&mut body).ok();
    let size = body.len() as i64;

    if size > 100 * 1024 * 1024 { // 100 МБ лимит
        return json_resp(413, &ApiError::json("Файл слишком большой (макс 100МБ)"));
    }

    std::fs::write(&path, &body).ok();

    match db::register_file(&conn, task_id, user.id, &safe_name, &stored, size) {
        Ok(id) => json_resp(201, &serde_json::to_string(&json!({"ok":true,"id":id})).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

fn handle_download_file(id_str: &str, req: &Request, db: &Db, files_dir: &str) -> Resp {
    let id: i64 = match id_str.parse() {
        Ok(n)  => n,
        Err(_) => return json_resp(400, &ApiError::json("Неверный ID")),
    };
    let conn = db.lock().unwrap();
    let _user = guard!(auth::require_user(&conn, req));
    match db::get_file(&conn, id) {
        Ok(Some(f)) => {
            let path = format!("{}/{}", files_dir, f.stored_name);
            match std::fs::read(&path) {
                Ok(data) => {
                    let cd = format!("attachment; filename=\"{}\"", f.filename);
                    Response::new(
                        StatusCode(200),
                        vec![
                            Header::from_bytes("Content-Type", "application/octet-stream").unwrap(),
                            Header::from_bytes("Content-Disposition", cd.as_bytes()).unwrap(),
                        ],
                        Cursor::new(data.clone()),
                        Some(data.len()),
                        None,
                    )
                }
                Err(_) => json_resp(404, &ApiError::json("Файл не найден на диске")),
            }
        }
        Ok(None) => json_resp(404, &ApiError::json("Запись не найдена")),
        Err(e)   => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

// ─── Аналитика ───────────────────────────────────────────────────────────────

fn handle_eisenhower(req: &Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let _user = guard!(auth::require_user(&conn, req));
    match db::list_tasks(&conn, None, None) {
        Ok(tasks) => {
            let matrix = json!({
                "q1": tasks.iter().filter(|t| t.is_urgent && t.is_important)
                           .collect::<Vec<_>>(),
                "q2": tasks.iter().filter(|t| !t.is_urgent && t.is_important)
                           .collect::<Vec<_>>(),
                "q3": tasks.iter().filter(|t| t.is_urgent && !t.is_important)
                           .collect::<Vec<_>>(),
                "q4": tasks.iter().filter(|t| !t.is_urgent && !t.is_important)
                           .collect::<Vec<_>>(),
            });
            json_resp(200, &serde_json::to_string(&matrix).unwrap())
        }
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

fn handle_lyubishchev_summary(req: &Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let user = guard!(auth::require_user(&conn, req));
    let params = query_params(req.url());
    let days: i32 = params.get("days").and_then(|d| d.parse().ok()).unwrap_or(30);
    let uid = if user.role.can_approve() {
        params.get("user").and_then(|u| u.parse().ok()).unwrap_or(user.id)
    } else { user.id };
    match db::get_time_report(&conn, uid, days) {
        Ok(r)  => json_resp(200, &serde_json::to_string(&r).unwrap()),
        Err(e) => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

// ─── Утилиты ─────────────────────────────────────────────────────────────────

type Resp = Response<Cursor<Vec<u8>>>;

macro_rules! guard {
    ($expr:expr) => {
        match $expr {
            Ok(u)  => u,
            Err(r) => return r,
        }
    };
}
use guard;

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
    Response::new(StatusCode(204), vec![
        Header::from_bytes("Access-Control-Allow-Origin",  "*").unwrap(),
        Header::from_bytes("Access-Control-Allow-Methods", "GET,POST,PUT,DELETE,OPTIONS").unwrap(),
        Header::from_bytes("Access-Control-Allow-Headers", "Authorization,Content-Type,X-Filename").unwrap(),
    ], Cursor::new(vec![]), Some(0), None)
}

fn response_with_cookie(code: u16, body: &str, cookie: &str) -> Resp {
    let data = body.as_bytes().to_vec();
    Response::new(
        StatusCode(code),
        vec![
            Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap(),
            Header::from_bytes("Set-Cookie", cookie.as_bytes()).unwrap(),
        ],
        Cursor::new(data.clone()),
        Some(data.len()),
        None,
    )
}

fn parse_json<T: serde::de::DeserializeOwned>(req: &mut Request) -> Result<T, String> {
    let mut buf = String::new();
    req.as_reader().read_to_string(&mut buf).map_err(|e| e.to_string())?;
    serde_json::from_str(&buf).map_err(|e| e.to_string())
}

fn query_params(url: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Some(qs) = url.splitn(2, '?').nth(1) {
        for pair in qs.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                map.insert(k.to_string(), v.to_string());
            }
        }
    }
    map
}
