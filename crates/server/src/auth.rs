use chrono::Local;
use database as db;
use protocol::*;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// SHA-256 хэш пароля. Без соли — для MVP в закрытом контуре достаточно.
pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

/// Создать сессию (токен) для пользователя
pub fn create_session(conn: &Connection, user_id: i64, ip: &str) -> rusqlite::Result<String> {
    let token = Uuid::new_v4().to_string();
    let expires = Local::now()
        .checked_add_signed(chrono::Duration::hours(120)) // 5 days
        .unwrap()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    conn.execute(
        "INSERT INTO sessions (token, user_id, expires_at, last_ip)
         VALUES (?1, ?2, ?3, ?4)",
        params![token, user_id, expires, ip],
    )?;
    Ok(token)
}

/// Проверить токен → вернуть пользователя или None
pub fn validate_token(conn: &Connection, token: &str) -> rusqlite::Result<Option<User>> {
    if token.is_empty() {
        return Ok(None);
    }
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let res: rusqlite::Result<i64> = conn.query_row(
        "SELECT user_id FROM sessions WHERE token=?1 AND expires_at > ?2",
        params![token, now],
        |row| row.get(0),
    );

    match res {
        Ok(uid) => db::get_user_by_id(conn, uid),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Удалить сессию (logout)
pub fn destroy_session(conn: &Connection, token: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM sessions WHERE token=?1", params![token])?;
    Ok(())
}

/// Извлечь Bearer-токен из заголовка Authorization
pub fn extract_token(req: &tiny_http::Request) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.as_str().to_string().to_lowercase() == "authorization")
        .map(|h| {
            let val = h.value.as_str();
            if val.starts_with("Bearer ") {
                val[7..].to_string()
            } else {
                val.to_string() // fallback if no Bearer prefix
            }
        })
}

pub fn check_role(user: &User, min_role: &Role) -> bool {
    // Admin can do anything
    if user.role == Role::Admin {
        return true;
    }

    match min_role {
        Role::Admin => user.role == Role::Admin,
        Role::Expert => matches!(user.role, Role::Admin | Role::Expert),
        Role::Manager => matches!(user.role, Role::Admin | Role::Manager),
        Role::Engineer => !matches!(user.role, Role::Viewer),
        Role::Analyst | Role::Viewer => true,
    }
}

pub fn require_user(
    conn: &Connection,
    req: &tiny_http::Request,
) -> Result<User, tiny_http::Response<std::io::Cursor<Vec<u8>>>> {
    let token = match extract_token(req) {
        Some(t) => t,
        None => return Err(json_401("Auth required")),
    };
    match validate_token(conn, &token) {
        Ok(Some(u)) => Ok(u),
        _ => Err(json_401("Session expired")),
    }
}

pub fn require_role(
    conn: &Connection,
    req: &tiny_http::Request,
    min_role: &Role,
) -> Result<User, tiny_http::Response<std::io::Cursor<Vec<u8>>>> {
    let user = require_user(conn, req)?;
    let ok = match min_role {
        Role::Admin => matches!(user.role, Role::Admin),
        Role::Expert => matches!(user.role, Role::Admin | Role::Expert),
        Role::Manager => matches!(user.role, Role::Admin | Role::Manager),
        Role::Engineer => !matches!(user.role, Role::Viewer),
        Role::Analyst | Role::Viewer => true,
    };
    if ok {
        Ok(user)
    } else {
        Err(json_403("Forbidden"))
    }
}

fn json_response(code: u16, body: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let data = body.as_bytes().to_vec();
    tiny_http::Response::new(
        tiny_http::StatusCode(code),
        vec![tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap()],
        std::io::Cursor::new(data.clone()),
        Some(data.len()),
        None,
    )
}

pub fn json_401(msg: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    json_response(401, &ApiError::json(msg))
}

pub fn json_403(msg: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    json_response(403, &ApiError::json(msg))
}
