use sha2::{Sha256, Digest};
use rusqlite::{Connection, params};
use uuid::Uuid;
use chrono::Local;
use crate::models::{User, Role};
use crate::db;

/// SHA-256 хэш пароля. Без соли — для MVP в закрытом контуре достаточно.
/// Для production: добавить pepper (константа в коде) + bcrypt.
pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

/// Создать сессию (токен) для пользователя
pub fn create_session(conn: &Connection, user_id: i64, ip: &str) -> rusqlite::Result<String> {
    let token = Uuid::new_v4().to_string();
    // Сессия живёт 8 часов (рабочий день)
    let expires = Local::now()
        .checked_add_signed(chrono::Duration::hours(8))
        .unwrap()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    conn.execute(
        "INSERT OR REPLACE INTO sessions (token, user_id, expires_at, ip_address)
         VALUES (?1, ?2, ?3, ?4)",
        params![token, user_id, expires, ip],
    )?;
    Ok(token)
}

/// Проверить токен → вернуть пользователя или None
pub fn validate_token(conn: &Connection, token: &str) -> rusqlite::Result<Option<User>> {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut stmt = conn.prepare(
        "SELECT user_id FROM sessions
         WHERE token=?1 AND expires_at > ?2"
    )?;
    let mut rows = stmt.query(params![token, now])?;
    if let Some(row) = rows.next()? {
        let uid: i64 = row.get(0)?;
        db::get_user_by_id(conn, uid)
    } else {
        Ok(None)
    }
}

/// Удалить сессию (logout)
pub fn destroy_session(conn: &Connection, token: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM sessions WHERE token=?1", params![token])?;
    Ok(())
}

/// Удалить истёкшие сессии (вызывать при старте и раз в час)
pub fn purge_expired_sessions(conn: &Connection) -> rusqlite::Result<usize> {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute("DELETE FROM sessions WHERE expires_at <= ?1", params![now])
}

/// Извлечь Bearer-токен из заголовка Authorization
/// "Authorization: Bearer <token>" → Some("<token>")
pub fn extract_token(req: &tiny_http::Request) -> Option<String> {
    for header in req.headers() {
        if header.field.as_str().eq_ignore_ascii_case("authorization") {
            let val = header.value.as_str();
            if let Some(tok) = val.strip_prefix("Bearer ") {
                return Some(tok.trim().to_string());
            }
        }
    }
    // Fallback: токен в cookie (для браузера без JS-fetch)
    for header in req.headers() {
        if header.field.as_str().eq_ignore_ascii_case("cookie") {
            for part in header.value.as_str().split(';') {
                let part = part.trim();
                if let Some(tok) = part.strip_prefix("token=") {
                    return Some(tok.to_string());
                }
            }
        }
    }
    None
}

/// Guard: получить текущего пользователя или вернуть 401
/// Используется в каждом handler'е:
///   let user = match auth::require_user(&conn, &req) { Ok(u) => u, Err(r) => return r };
pub fn require_user(conn: &Connection, req: &tiny_http::Request)
    -> Result<User, tiny_http::Response<std::io::Cursor<Vec<u8>>>>
{
    let token = match extract_token(req) {
        Some(t) => t,
        None    => return Err(json_401("Требуется авторизация")),
    };
    match validate_token(conn, &token) {
        Ok(Some(u)) => Ok(u),
        _           => Err(json_401("Сессия истекла или недействительна")),
    }
}

/// Guard: требует конкретную роль
pub fn require_role(conn: &Connection, req: &tiny_http::Request, min_role: &Role)
    -> Result<User, tiny_http::Response<std::io::Cursor<Vec<u8>>>>
{
    let user = require_user(conn, req)?;
    let ok = match min_role {
        Role::Viewer   => true,
        Role::Engineer => !matches!(user.role, Role::Viewer),
        Role::Expert   => matches!(user.role, Role::Expert | Role::Admin),
        Role::Admin    => matches!(user.role, Role::Admin),
    };
    if ok { Ok(user) } else { Err(json_403("Недостаточно прав")) }
}

// ─── Вспомогательные ответы ──────────────────────────────────────────────────

fn json_response(code: u16, body: &str)
    -> tiny_http::Response<std::io::Cursor<Vec<u8>>>
{
    let data   = body.as_bytes().to_vec();
    let cursor = std::io::Cursor::new(data.clone());
    tiny_http::Response::new(
        tiny_http::StatusCode(code),
        vec![
            tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
            tiny_http::Header::from_bytes("Content-Length",
                data.len().to_string().as_bytes()).unwrap(),
        ],
        cursor,
        Some(data.len()),
        None,
    )
}

pub fn json_401(msg: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    json_response(401, &format!(r#"{{"error":"{}"}}"#, msg))
}

pub fn json_403(msg: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    json_response(403, &format!(r#"{{"error":"{}"}}"#, msg))
}
