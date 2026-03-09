use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use chrono::Local;
use database as db;
use protocol::*;
use password_hash::rand_core::OsRng;
use rusqlite::{Connection, params};
use uuid::Uuid;

/// Argon2id хэш пароля.
pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .expect("Failed to hash password")
        .to_string()
}

/// Проверка Argon2id хэша.
pub fn verify_password(password: &str, hash: &str) -> bool {
    if let Ok(parsed_hash) = PasswordHash::new(hash) {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    } else {
        // Fallback for old SHA-256 for migration or just fail
        false
    }
}

/// Создать сессию (токен) для пользователя
pub fn create_session(conn: &Connection, user_id: i64, ip: &str, remember_me: bool) -> rusqlite::Result<String> {
    let token = Uuid::new_v4().to_string();
    let hours = if remember_me { 120 } else { 24 };
    let expires = Local::now()
        .checked_add_signed(chrono::Duration::hours(hours))
        .expect("Time addition must succeed")
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
            if let Some(stripped) = val.strip_prefix("Bearer ") {
                stripped.to_string()
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
fn json_response(code: u16, body: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let data = body.as_bytes().to_vec();
    tiny_http::Response::new(
        tiny_http::StatusCode(code),
        vec![
            tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
        ],
        std::io::Cursor::new(data.clone()),
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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: i64,
    pub user_id: i64,
    pub token: String,
    pub ip_address: String,
    pub created_at: String,
    pub expires_at: String,
}
