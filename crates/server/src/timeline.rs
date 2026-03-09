use protocol::*;
use rusqlite::{Connection, params};

// (Structures are now in protocol, but for local usage we might keep specific summary types here if they aren't shared)

pub fn create_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS journal (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id     INTEGER NOT NULL,
            event_type  TEXT NOT NULL,
            task_id     INTEGER,
            detail      TEXT NOT NULL DEFAULT '',
            duration_s  INTEGER,
            category    INTEGER,
            happened_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id),
            FOREIGN KEY (task_id) REFERENCES tasks(id)
        );
        CREATE INDEX IF NOT EXISTS idx_journal_user_date
            ON journal(user_id, happened_at);
    ",
    )
}

pub fn record(
    conn: &Connection,
    user_id: i64,
    event_type: &str,
    task_id: Option<i64>,
    detail: &str,
    duration_s: Option<i64>,
    category: Option<i32>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO journal (user_id, event_type, task_id, detail, duration_s, category)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![user_id, event_type, task_id, detail, duration_s, category],
    )?;
    Ok(())
}

#[expect(dead_code, reason = "Used by upcoming API")]
pub fn on_login(conn: &Connection, user_id: i64, ip: &str) -> rusqlite::Result<()> {
    record(
        conn,
        user_id,
        "login",
        None,
        &format!("Login from {}", ip),
        None,
        None,
    )
}

pub fn on_timer_start(
    conn: &Connection,
    user_id: i64,
    task_id: Option<i64>,
    task_title: &str,
    category: i32,
) -> rusqlite::Result<()> {
    record(
        conn,
        user_id,
        "timer_start",
        task_id,
        &format!("Started: {}", task_title),
        None,
        Some(category),
    )
}

#[expect(dead_code, reason = "Used by upcoming API")]
pub fn on_timer_stop(
    conn: &Connection,
    user_id: i64,
    task_id: Option<i64>,
    task_title: &str,
    category: i32,
    duration_s: i64,
) -> rusqlite::Result<()> {
    record(
        conn,
        user_id,
        "timer_stop",
        task_id,
        &format!("Stopped: {} ({})", task_title, duration_s),
        Some(duration_s),
        Some(category),
    )
}

pub fn get_day_entries(
    conn: &Connection,
    user_id: i64,
    date: &str,
) -> rusqlite::Result<Vec<JournalEntry>> {
    // Implementation moved/shared with protocol
    let mut stmt = conn.prepare(
        "
        SELECT j.id, j.user_id, u.username, j.event_type,
               j.task_id,
               (SELECT title FROM tasks WHERE id=j.task_id),
               j.detail, j.duration_s, j.category, j.happened_at
        FROM journal j JOIN users u ON u.id=j.user_id
        WHERE j.user_id=?1
          AND date(j.happened_at)=?2
        ORDER BY j.happened_at ASC
    ",
    )?;
    let rows = stmt.query_map(params![user_id, date], |r| {
        Ok(JournalEntry {
            id: r.get(0)?,
            user_id: r.get(1)?,
            username: r.get(2)?,
            event_type: r.get(3)?,
            task_id: r.get(4)?,
            task_title: r.get(5)?,
            detail: r.get(6)?,
            duration_s: r.get(7)?,
            category: r.get(8)?,
            happened_at: r.get(9)?,
        })
    })?;
    rows.collect()
}
