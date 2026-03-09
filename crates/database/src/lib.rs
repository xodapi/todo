use protocol::*;
use rusqlite::{Connection, OptionalExtension, Result, params};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;

pub fn open<P: AsRef<Path>>(path: P) -> Result<Connection> {
    let conn = Connection::open(path)?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS users (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            username    TEXT NOT NULL UNIQUE,
            pass_hash   TEXT NOT NULL,
            role        TEXT NOT NULL,
            full_name   TEXT,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id    INTEGER NOT NULL,
            token      TEXT NOT NULL UNIQUE,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            expires_at DATETIME NOT NULL,
            last_ip    TEXT,
            FOREIGN KEY (user_id) REFERENCES users(id)
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            title        TEXT NOT NULL,
            description  TEXT,
            source       TEXT,
            status       TEXT NOT NULL DEFAULT 'inbox',
            priority     REAL DEFAULT 0.0,
            impact       INTEGER DEFAULT 3,
            effort       INTEGER DEFAULT 3,
            is_urgent    INTEGER DEFAULT 0,
            is_important INTEGER DEFAULT 1,
            approved_at  DATETIME,
            approved_by  INTEGER,
            assigned_to  INTEGER,
            created_by   INTEGER NOT NULL,
            created_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
            started_at   DATETIME,
            finished_at  DATETIME,
            deadline     DATETIME,
            FOREIGN KEY (created_by) REFERENCES users(id),
            FOREIGN KEY (assigned_to) REFERENCES users(id),
            FOREIGN KEY (approved_by) REFERENCES users(id)
        );

        CREATE TABLE IF NOT EXISTS windows_activity (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id      INTEGER NOT NULL,
            process_name TEXT NOT NULL,
            window_title TEXT NOT NULL,
            started_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            duration_s   INTEGER DEFAULT 0,
            is_private   INTEGER DEFAULT 0,
            FOREIGN KEY (user_id) REFERENCES users(id)
        );

        CREATE TABLE IF NOT EXISTS reflection_answers (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id     INTEGER NOT NULL,
            question    TEXT NOT NULL,
            answer      TEXT NOT NULL,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id)
        );

        CREATE TABLE IF NOT EXISTS knowledge_notes (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id     INTEGER NOT NULL,
            parent_id   INTEGER,
            title       TEXT NOT NULL,
            content     TEXT NOT NULL,
            aliases     TEXT DEFAULT '',
            is_archived INTEGER DEFAULT 0,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id),
            FOREIGN KEY (parent_id) REFERENCES knowledge_notes(id)
        );

        CREATE TABLE IF NOT EXISTS knowledge_tags (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS note_tags (
            note_id     INTEGER NOT NULL,
            tag_id      INTEGER NOT NULL,
            PRIMARY KEY (note_id, tag_id),
            FOREIGN KEY (note_id) REFERENCES knowledge_notes(id) ON DELETE CASCADE,
            FOREIGN KEY (tag_id) REFERENCES knowledge_tags(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS note_links (
            source_id   INTEGER NOT NULL,
            target_id   INTEGER NOT NULL,
            PRIMARY KEY (source_id, target_id),
            FOREIGN KEY (source_id) REFERENCES knowledge_notes(id) ON DELETE CASCADE,
            FOREIGN KEY (target_id) REFERENCES knowledge_notes(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS input_metrics (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id           INTEGER NOT NULL,
            key_count         INTEGER DEFAULT 0,
            mouse_distance_px INTEGER DEFAULT 0,
            measured_at       DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id)
        );

        CREATE TABLE IF NOT EXISTS time_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id     INTEGER,
            user_id     INTEGER NOT NULL,
            category    INTEGER NOT NULL,
            started_at  DATETIME NOT NULL,
            finished_at DATETIME,
            duration_s  INTEGER,
            note        TEXT,
            FOREIGN KEY (task_id) REFERENCES tasks(id),
            FOREIGN KEY (user_id) REFERENCES users(id)
        );

        CREATE TABLE IF NOT EXISTS messages (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id     INTEGER NOT NULL,
            task_id     INTEGER,
            body        TEXT NOT NULL,
            sent_at     DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id),
            FOREIGN KEY (task_id) REFERENCES tasks(id)
        );

        CREATE TABLE IF NOT EXISTS files (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id      INTEGER,
            uploaded_by  INTEGER NOT NULL,
            filename     TEXT NOT NULL,
            stored_name  TEXT NOT NULL,
            size_bytes   INTEGER,
            uploaded_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (task_id) REFERENCES tasks(id),
            FOREIGN KEY (uploaded_by) REFERENCES users(id)
        );
        INSERT OR IGNORE INTO users (username, pass_hash, role, full_name)
        VALUES ('admin', '$argon2id$v=19$m=19456,t=2,p=1$M6Ge9us1ftwIyNsWSQHG5Q$BBOo63u6x8k+khSFOyC/PBXgvhcl7BPrZJZ/JUkN7J8', 'admin', 'Administrator');
    ")?;
    Ok(())
}

pub fn find_user_by_credentials(
    conn: &Connection,
    username: &str,
    hash: &str,
) -> Result<Option<User>> {
    let mut stmt = conn.prepare("SELECT id, username, role, full_name, created_at FROM users WHERE username = ?1 AND pass_hash = ?2")?;
    let mut rows = stmt.query(params![username, hash])?;
    if let Some(row) = rows.next()? {
        Ok(Some(User {
            id: row.get(0)?,
            username: row.get(1)?,
            role: Role::from_str(&row.get::<usize, String>(2)?).unwrap_or(Role::Viewer),
            full_name: row.get(3)?,
            created_at: row.get(4)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn create_user(
    conn: &Connection,
    username: &str,
    hash: &str,
    role: &str,
    full_name: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO users (username, pass_hash, role, full_name) VALUES (?1, ?2, ?3, ?4)",
        params![username, hash, role, full_name],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_users(conn: &Connection) -> Result<Vec<User>> {
    let mut stmt = conn.prepare("SELECT id, username, role, full_name, created_at FROM users")?;
    let rows = stmt.query_map([], |row| {
        Ok(User {
            id: row.get(0)?,
            username: row.get(1)?,
            role: Role::from_str(&row.get::<usize, String>(2)?).unwrap_or(Role::Viewer),
            full_name: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    let mut users = Vec::new();
    for user in rows {
        users.push(user?);
    }
    Ok(users)
}

pub fn update_password(conn: &Connection, user_id: i64, new_hash: &str) -> Result<()> {
    conn.execute(
        "UPDATE users SET pass_hash = ?1 WHERE id = ?2",
        params![new_hash, user_id],
    )?;
    Ok(())
}

pub fn list_tasks(
    conn: &Connection,
    status: Option<&str>,
    assigned_to: Option<i64>,
) -> Result<Vec<Task>> {
    let mut query = "SELECT id, title, description, source, status, priority, impact, effort, is_urgent, is_important, approved_by, assigned_to, created_by, created_at, started_at, finished_at, deadline FROM tasks WHERE 1=1".to_string();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(s) = status {
        query.push_str(" AND status = ?");
        params_vec.push(Box::new(s.to_string()));
    }
    if let Some(a) = assigned_to {
        query.push_str(" AND assigned_to = ?");
        params_vec.push(Box::new(a));
    }

    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
        Ok(Task {
            id: row.get(0)?,
            title: row.get(1)?,
            description: row.get(2)?,
            source: row.get(3)?,
            status: row.get(4)?,
            priority: row.get(5)?,
            impact: row.get(6)?,
            effort: row.get(7)?,
            is_urgent: row.get::<usize, i32>(8)? != 0,
            is_important: row.get::<usize, i32>(9)? != 0,
            approved_by: row.get(10)?,
            assigned_to: row.get(11)?,
            created_by: row.get(12)?,
            created_at: row.get(13)?,
            started_at: row.get(14)?,
            finished_at: row.get(15)?,
            deadline: row.get(16)?,
        })
    })?;

    let mut tasks = Vec::new();
    for t in rows {
        tasks.push(t?);
    }
    Ok(tasks)
}

pub fn create_task(conn: &Connection, req: &CreateTaskRequest, created_by: i64) -> Result<i64> {
    conn.execute(
        "INSERT INTO tasks (title, description, is_urgent, is_important, created_by) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![req.title, req.description, req.is_urgent as i32, req.is_important as i32, created_by],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_task(conn: &Connection, id: i64) -> Result<Option<Task>> {
    let mut stmt = conn.prepare("SELECT id, title, description, source, status, priority, impact, effort, is_urgent, is_important, approved_by, assigned_to, created_by, created_at, started_at, finished_at, deadline FROM tasks WHERE id = ?1")?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(Task {
            id: row.get(0)?,
            title: row.get(1)?,
            description: row.get(2)?,
            source: row.get(3)?,
            status: row.get(4)?,
            priority: row.get(5)?,
            impact: row.get(6)?,
            effort: row.get(7)?,
            is_urgent: row.get::<usize, i32>(8)? != 0,
            is_important: row.get::<usize, i32>(9)? != 0,
            approved_by: row.get(10)?,
            assigned_to: row.get(11)?,
            created_by: row.get(12)?,
            created_at: row.get(13)?,
            started_at: row.get(14)?,
            finished_at: row.get(15)?,
            deadline: row.get(16)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn update_task(
    conn: &Connection,
    id: i64,
    req: &UpdateTaskRequest,
    _user_id: i64,
    _role: &Role,
) -> Result<bool> {
    // Упрощенная логика обновления
    let mut query = "UPDATE tasks SET id=id".to_string();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(t) = &req.title {
        query.push_str(", title = ?");
        params_vec.push(Box::new(t.clone()));
    }
    if let Some(s) = &req.status {
        query.push_str(", status = ?");
        params_vec.push(Box::new(s.clone()));
    }
    if let Some(a) = req.assigned_to {
        query.push_str(", assigned_to = ?");
        params_vec.push(Box::new(a));
    }
    if let Some(u) = req.is_urgent {
        query.push_str(", is_urgent = ?");
        params_vec.push(Box::new(u as i32));
    }
    if let Some(i) = req.is_important {
        query.push_str(", is_important = ?");
        params_vec.push(Box::new(i as i32));
    }

    query.push_str(" WHERE id = ?");
    params_vec.push(Box::new(id));

    let affected = conn.execute(&query, rusqlite::params_from_iter(params_vec))?;
    Ok(affected > 0)
}

pub fn get_user_by_id(conn: &Connection, id: i64) -> Result<Option<User>> {
    let mut stmt =
        conn.prepare("SELECT id, username, role, full_name, created_at FROM users WHERE id = ?1")?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(User {
            id: row.get(0)?,
            username: row.get(1)?,
            role: Role::from_str(&row.get::<usize, String>(2)?).unwrap_or(Role::Viewer),
            full_name: row.get(3)?,
            created_at: row.get(4)?,
        }))
    } else {
        Ok(None)
    }
}

// Timer & Reporting Logic
pub fn start_timer(
    conn: &Connection,
    user_id: i64,
    task_id: Option<i64>,
    category: i32,
    note: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO time_log (user_id, task_id, category, note, started_at) VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)",
        params![user_id, task_id, category, note],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn stop_timer(conn: &Connection, log_id: i64, user_id: i64) -> Result<bool> {
    let affected = conn.execute(
        r#"UPDATE time_log SET finished_at = CURRENT_TIMESTAMP, duration_s = (strftime('%s','now') - strftime('%s', started_at)) WHERE id = ?1 AND user_id = ?2 AND finished_at IS NULL"#,
        params![log_id, user_id],
    )?;
    Ok(affected > 0)
}

pub fn get_time_report(conn: &Connection, user_id: i64, days: i32) -> Result<serde_json::Value> {
    // Simplified report as JSON
    let mut stmt = conn.prepare(r#"SELECT category, SUM(duration_s) FROM time_log WHERE user_id = ?1 AND started_at >= date('now', '-' || ?2 || ' days') GROUP BY category"#)?;
    let rows = stmt.query_map(params![user_id, days], |row| {
        Ok((
            row.get::<usize, i32>(0)?,
            row.get::<usize, Option<i64>>(1)
                .unwrap_or_default()
                .unwrap_or(0),
        ))
    })?;

    let mut map = std::collections::HashMap::<i32, i64>::new();
    for r in rows {
        let (cat, sum) = r?;
        map.insert(cat, sum);
    }
    Ok(serde_json::to_value(map).unwrap())
}

pub fn send_message(
    conn: &Connection,
    user_id: i64,
    task_id: Option<i64>,
    body: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO messages (user_id, task_id, body) VALUES (?1, ?2, ?3)",
        params![user_id, task_id, body],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_messages(
    conn: &Connection,
    task_id: Option<i64>,
    since: Option<&str>,
) -> Result<Vec<serde_json::Value>> {
    let mut query = "SELECT id, user_id, body, sent_at FROM messages WHERE 1=1".to_string();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(t) = task_id {
        query.push_str(" AND task_id = ?");
        params_vec.push(Box::new(t));
    }
    if let Some(s) = since {
        query.push_str(" AND sent_at > ?");
        params_vec.push(Box::new(s.to_string()));
    }

    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
        Ok(serde_json::json!({
            "id": row.get::<usize, i64>(0)?,
            "user_id": row.get::<usize, i64>(1)?,
            "body": row.get::<usize, String>(2)?,
            "sent_at": row.get::<usize, String>(3)?,
        }))
    })?;

    let mut msgs = Vec::new();
    for m in rows {
        msgs.push(m?);
    }
    Ok(msgs)
}

pub fn register_file(
    conn: &Connection,
    task_id: Option<i64>,
    user_id: i64,
    filename: &str,
    stored: &str,
    size: i64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO files (task_id, uploaded_by, filename, stored_name, size_bytes) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![task_id, user_id, filename, stored, size],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_file(conn: &Connection, id: i64) -> Result<Option<serde_json::Value>> {
    let mut stmt = conn.prepare("SELECT filename, stored_name FROM files WHERE id = ?1")?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(serde_json::json!({
            "filename": row.get::<usize, String>(0)?,
            "stored_name": row.get::<usize, String>(1)?,
        })))
    } else {
        Ok(None)
    }
}

pub fn list_files(conn: &Connection, task_id: Option<i64>) -> Result<Vec<serde_json::Value>> {
    let mut query = "SELECT id, filename, uploaded_at FROM files WHERE 1=1".to_string();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(t) = task_id {
        query.push_str(" AND task_id = ?");
        params_vec.push(Box::new(t));
    }

    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
        Ok(serde_json::json!({
            "id": row.get::<usize, i64>(0)?,
            "filename": row.get::<usize, String>(1)?,
            "uploaded_at": row.get::<usize, String>(2)?,
        }))
    })?;

    let mut files = Vec::new();
    for f in rows {
        files.push(f?);
    }
    Ok(files)
}

// Activity Record Logic
pub fn record_activity(
    conn: &Connection,
    user_id: i64,
    process: &str,
    title: &str,
    duration: i64,
    is_private: bool,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO windows_activity (user_id, process_name, window_title, duration_s, is_private) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![user_id, process, title, duration, is_private as i32],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn save_reflection(conn: &Connection, user_id: i64, q: &str, a: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO reflection_answers (user_id, question, answer) VALUES (?1, ?2, ?3)",
        params![user_id, q, a],
    )?;
    Ok(())
}

// --- Knowledge Base Functions ---

pub fn create_note(
    conn: &Connection,
    user_id: i64,
    parent_id: Option<i64>,
    title: &str,
    content: &str,
    aliases: &str,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO knowledge_notes (user_id, parent_id, title, content, aliases, is_archived) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        params![user_id, parent_id, title, "", aliases],
    )?;
    let id = conn.last_insert_rowid();
    save_note_to_file(id, content).ok();
    Ok(id)
}

pub fn update_note(
    conn: &Connection,
    id: i64,
    title: &str,
    content: &str,
    parent_id: Option<i64>,
    aliases: &str,
    is_archived: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE knowledge_notes SET title=?1, content=?2, parent_id=?3, aliases=?4, is_archived=?5, updated_at=CURRENT_TIMESTAMP WHERE id=?6",
        params![title, "", parent_id, aliases, is_archived as i32, id],
    )?;
    save_note_to_file(id, content).ok();
    Ok(())
}

fn save_note_to_file(id: i64, content: &str) -> std::io::Result<()> {
    let dir = Path::new("kb_notes");
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    let path = dir.join(format!("{}.md", id));
    let mut file = fs::File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

fn read_note_from_file(id: i64) -> String {
    let path = Path::new("kb_notes").join(format!("{}.md", id));
    if let Ok(mut file) = fs::File::open(path) {
        let mut content = String::new();
        if file.read_to_string(&mut content).is_ok() {
            return content;
        }
    }
    String::new()
}

pub fn delete_note(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM knowledge_notes WHERE id=?1", params![id])?;
    let path = Path::new("kb_notes").join(format!("{}.md", id));
    if path.exists() {
        fs::remove_file(path).ok();
    }
    Ok(())
}

pub fn get_note(conn: &Connection, id: i64) -> rusqlite::Result<KnowledgeNote> {
    conn.query_row(
        "SELECT id, user_id, parent_id, title, content, created_at, updated_at, aliases, is_archived FROM knowledge_notes WHERE id=?1",
        params![id],
        |r| {
            let id: i64 = r.get(0)?;
            let mut content: String = r.get(4)?;
            if content.is_empty() {
                content = read_note_from_file(id);
            }
            let tags = get_note_tags(conn, id).unwrap_or_default();
            Ok(KnowledgeNote {
                id,
                user_id: r.get(1)?,
                parent_id: r.get(2)?,
                title: r.get(3)?,
                content,
                aliases: r.get(7)?,
                is_archived: r.get::<_, i32>(8)? != 0,
                tags,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        }
    )
}

pub fn get_note_tags(conn: &Connection, note_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT t.name FROM knowledge_tags t JOIN note_tags nt ON t.id = nt.tag_id WHERE nt.note_id = ?1")?;
    let tags = stmt
        .query_map(params![note_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(tags)
}

pub fn add_link(conn: &Connection, source: i64, target: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO note_links (source_id, target_id) VALUES (?1, ?2)",
        params![source, target],
    )?;
    Ok(())
}

pub fn remove_link(conn: &Connection, source: i64, target: i64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM note_links WHERE source_id=?1 AND target_id=?2",
        params![source, target],
    )?;
    Ok(())
}

pub fn get_graph(conn: &Connection, user_id: i64) -> rusqlite::Result<KbGraphData> {
    let mut stmt = conn.prepare("SELECT id, title FROM knowledge_notes WHERE user_id=?1")?;
    let nodes = stmt
        .query_map(params![user_id], |r| {
            Ok(KbNode {
                id: r.get(0)?,
                label: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = conn.prepare("SELECT source_id, target_id FROM note_links WHERE source_id IN (SELECT id FROM knowledge_notes WHERE user_id=?1)")?;
    let edges = stmt
        .query_map(params![user_id], |r| {
            Ok(KbEdge {
                from: r.get(0)?,
                to: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(KbGraphData { nodes, edges })
}

pub fn add_tag(conn: &Connection, note_id: i64, tag_name: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO knowledge_tags (name) VALUES (?1)",
        params![tag_name],
    )?;
    let tag_id: i64 = conn.query_row(
        "SELECT id FROM knowledge_tags WHERE name=?1",
        params![tag_name],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
        params![note_id, tag_id],
    )?;
    Ok(())
}

pub fn record_input(conn: &Connection, user_id: i64, keys: i64, mouse_px: i64) -> Result<i64> {
    // Check if there is an entry within the same minute
    let exist: Option<i64> = conn.query_row(
        "SELECT id FROM input_metrics 
         WHERE user_id = ?1 
         AND strftime('%Y-%m-%d %H:%M', measured_at) = strftime('%Y-%m-%d %H:%M', 'now', 'localtime')
         ORDER BY id DESC LIMIT 1",
        params![user_id],
        |row| row.get(0)
    ).optional()?;

    if let Some(id) = exist {
        conn.execute(
            "UPDATE input_metrics SET key_count = ?1, mouse_distance_px = ?2, measured_at = CURRENT_TIMESTAMP WHERE id = ?3",
            params![keys, mouse_px, id],
        )?;
        Ok(id)
    } else {
        conn.execute(
            "INSERT INTO input_metrics (user_id, key_count, mouse_distance_px) VALUES (?1, ?2, ?3)",
            params![user_id, keys, mouse_px],
        )?;
        Ok(conn.last_insert_rowid())
    }
}
pub fn get_journal_entries(conn: &Connection, user_id: i64) -> Result<Vec<JournalEntry>> {
    let mut stmt = conn.prepare(
        "SELECT j.id, j.user_id, u.username, j.event_type, j.task_id, t.title, j.detail, j.duration_s, j.created_at
         FROM journal j
         JOIN users u ON u.id = j.user_id
         LEFT JOIN tasks t ON t.id = j.task_id
         WHERE j.user_id = ?1
         ORDER BY j.created_at DESC LIMIT 100",
    )?;
    let rows = stmt.query_map(params![user_id], |row| {
        Ok(JournalEntry {
            id: row.get(0)?,
            user_id: row.get(1)?,
            username: row.get(2)?,
            event_type: row.get(3)?,
            task_id: row.get(4)?,
            task_title: row.get(5)?,
            detail: row.get(6)?,
            duration_s: row.get(7)?,
            category: None, // Not in simple journal table yet
            happened_at: row.get(8)?,
        })
    })?;

    let mut entries = Vec::new();
    for entry in rows {
        entries.push(entry?);
    }
    Ok(entries)
}
pub fn create_message(conn: &Connection, user_id: i64, body: &str, task_id: Option<i64>) -> Result<i64> {
    conn.execute(
        "INSERT INTO messages (user_id, body, task_id) VALUES (?1, ?2, ?3)",
        params![user_id, body, task_id],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_messages(conn: &Connection, since: Option<&str>) -> Result<Vec<ChatMessage>> {
    let mut query = "
        SELECT m.id, m.user_id, u.username, m.task_id, m.body, m.sent_at
        FROM messages m
        JOIN users u ON u.id = m.user_id
        WHERE 1=1
    ".to_string();
    
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(s) = since {
        query.push_str(" AND m.sent_at > ?");
        params_vec.push(Box::new(s.to_string()));
    }
    query.push_str(" ORDER BY m.sent_at ASC LIMIT 200");

    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
        Ok(ChatMessage {
            id: row.get(0)?,
            user_id: row.get(1)?,
            username: row.get(2)?,
            task_id: row.get(3)?,
            body: row.get(4)?,
            sent_at: row.get(5)?,
        })
    })?;

    let mut msgs = Vec::new();
    for msg in rows {
        msgs.push(msg?);
    }
    Ok(msgs)
}

pub fn get_message(conn: &Connection, id: i64) -> Result<Option<ChatMessage>> {
    let mut stmt = conn.prepare("
        SELECT m.id, m.user_id, u.username, m.task_id, m.body, m.sent_at
        FROM messages m
        JOIN users u ON u.id = m.user_id
        WHERE m.id = ?1
    ")?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(ChatMessage {
            id: row.get(0)?,
            user_id: row.get(1)?,
            username: row.get(2)?,
            task_id: row.get(3)?,
            body: row.get(4)?,
            sent_at: row.get(5)?,
        }))
    } else {
        Ok(None)
    }
}
pub fn list_notes(conn: &Connection, user_id: i64, include_archived: bool) -> rusqlite::Result<Vec<KnowledgeNote>> {
    let mut query = "SELECT id, user_id, parent_id, title, created_at, updated_at, aliases, is_archived FROM knowledge_notes WHERE user_id=?1".to_string();
    if !include_archived {
        query.push_str(" AND is_archived = 0");
    }
    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map(params![user_id], |row| {
        let id: i64 = row.get(0)?;
        Ok(KnowledgeNote {
            id,
            user_id: row.get(1)?,
            parent_id: row.get(2)?,
            title: row.get(3)?,
            content: read_note_from_file(id),
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
            aliases: row.get(6)?,
            is_archived: row.get::<usize, i32>(7)? != 0,
            tags: vec![], // Tags could be joined if needed
        })
    })?;

    let mut notes = Vec::new();
    for n in rows {
        notes.push(n?);
    }
    Ok(notes)
}

pub fn list_tags(conn: &Connection) -> rusqlite::Result<Vec<NoteTag>> {
    let mut stmt = conn.prepare("SELECT id, name FROM knowledge_tags")?;
    let rows = stmt.query_map([], |row| {
        Ok(NoteTag {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    let mut tags = Vec::new();
    for t in rows {
        tags.push(t?);
    }
    Ok(tags)
}

pub fn get_kb_graph(conn: &Connection) -> rusqlite::Result<KbGraphData> {
    let mut stmt = conn.prepare("SELECT id, title FROM knowledge_notes WHERE is_archived = 0")?;
    let nodes = stmt.query_map([], |row| {
        Ok(KbNode {
            id: row.get(0)?,
            label: row.get(1)?,
        })
    })?.collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = conn.prepare("SELECT source_id, target_id FROM note_links")?;
    let edges = stmt.query_map([], |row| {
        Ok(KbEdge {
            from: row.get(0)?,
            to: row.get(1)?,
        })
    })?.collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(KbGraphData { nodes, edges })
}
