/// Система уведомлений — хранит события в БД, клиент забирает polling'ом
/// GET /notifications          → непрочитанные для текущего пользователя
/// POST /notifications/:id/read → пометить прочитанным

use rusqlite::{Connection, params};
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct Notification {
    pub id        : i64,
    pub user_id   : i64,       // кому
    pub kind      : String,    // task_assigned | deadline_soon | task_review | mention
    pub title     : String,
    pub body      : String,
    pub task_id   : Option<i64>,
    pub is_read   : bool,
    pub created_at: String,
}

/// Инициализация таблицы (вызывается из db::migrate)
pub fn create_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS notifications (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id    INTEGER NOT NULL,
            kind       TEXT NOT NULL,
            title      TEXT NOT NULL,
            body       TEXT NOT NULL,
            task_id    INTEGER,
            is_read    INTEGER NOT NULL DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id),
            FOREIGN KEY (task_id) REFERENCES tasks(id)
        );
        CREATE INDEX IF NOT EXISTS idx_notif_user
            ON notifications(user_id, is_read);
    ")
}

/// Создать уведомление (внутренняя функция)
pub fn push(conn: &Connection, user_id: i64, kind: &str,
            title: &str, body: &str, task_id: Option<i64>) -> rusqlite::Result<()>
{
    conn.execute(
        "INSERT INTO notifications (user_id, kind, title, body, task_id)
         VALUES (?1,?2,?3,?4,?5)",
        params![user_id, kind, title, body, task_id],
    )?;
    Ok(())
}

/// Получить непрочитанные уведомления пользователя
pub fn get_unread(conn: &Connection, user_id: i64) -> rusqlite::Result<Vec<Notification>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, kind, title, body, task_id, is_read, created_at
         FROM notifications
         WHERE user_id=?1 AND is_read=0
         ORDER BY created_at DESC
         LIMIT 50"
    )?;
    let rows = stmt.query_map(params![user_id], |r| Ok(Notification {
        id:         r.get(0)?,
        user_id:    r.get(1)?,
        kind:       r.get(2)?,
        title:      r.get(3)?,
        body:       r.get(4)?,
        task_id:    r.get(5)?,
        is_read:    r.get::<_,i32>(6)? != 0,
        created_at: r.get(7)?,
    }))?;
    rows.collect()
}

/// Пометить прочитанным
pub fn mark_read(conn: &Connection, notif_id: i64, user_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE notifications SET is_read=1 WHERE id=?1 AND user_id=?2",
        params![notif_id, user_id],
    )?;
    Ok(())
}

/// Пометить все прочитанными
pub fn mark_all_read(conn: &Connection, user_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE notifications SET is_read=1 WHERE user_id=?1",
        params![user_id],
    )?;
    Ok(())
}

// ── Триггеры бизнес-логики ───────────────────────────────────────────────────

/// Вызывается когда задача назначена исполнителю
pub fn on_task_assigned(conn: &Connection, task_id: i64, task_title: &str,
                         assigned_to: i64, assigned_by_name: &str) -> rusqlite::Result<()>
{
    push(conn, assigned_to,
        "task_assigned",
        &format!("Вам назначена задача"),
        &format!("«{}» — назначил: {}", task_title, assigned_by_name),
        Some(task_id),
    )
}

/// Вызывается когда задача переведена в статус "на проверке"
pub fn on_task_review(conn: &Connection, task_id: i64, task_title: &str,
                       expert_id: i64, engineer_name: &str) -> rusqlite::Result<()>
{
    push(conn, expert_id,
        "task_review",
        "Задача ожидает проверки",
        &format!("«{}» — выполнил: {}", task_title, engineer_name),
        Some(task_id),
    )
}

/// Вызывается когда задача утверждена
pub fn on_task_approved(conn: &Connection, task_id: i64, task_title: &str,
                         assigned_to: i64, expert_name: &str) -> rusqlite::Result<()>
{
    push(conn, assigned_to,
        "task_approved",
        "Задача утверждена — можно брать в работу",
        &format!("«{}» — утвердил: {}", task_title, expert_name),
        Some(task_id),
    )
}

/// @упоминание в чате — парсим "@username" из текста сообщения
pub fn on_mention(conn: &Connection, task_id: Option<i64>, sender_name: &str,
                   body: &str) -> rusqlite::Result<()>
{
    // Ищем все @username в тексте
    for word in body.split_whitespace() {
        if let Some(username) = word.strip_prefix('@') {
            let username = username.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            // Находим user_id по username
            let uid: rusqlite::Result<i64> = conn.query_row(
                "SELECT id FROM users WHERE username=?1",
                params![username],
                |r| r.get(0),
            );
            if let Ok(uid) = uid {
                push(conn, uid,
                    "mention",
                    &format!("Вас упомянул {}", sender_name),
                    &format!("{}: {}", sender_name, &body[..body.len().min(100)]),
                    task_id,
                )?;
            }
        }
    }
    Ok(())
}

// ── Фоновый воркер: проверка дедлайнов ──────────────────────────────────────
/// Запускается в отдельном потоке при старте сервера.
/// Каждые 30 минут проверяет задачи с истекающим дедлайном.
pub fn deadline_watcher(db: std::sync::Arc<std::sync::Mutex<Connection>>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1800)); // 30 мин
            let conn = db.lock().unwrap();
            check_deadlines(&conn).ok();
        }
    });
}

fn check_deadlines(conn: &Connection) -> rusqlite::Result<()> {
    // Задачи с дедлайном через 24 часа, не завершённые, ещё не уведомляли
    let mut stmt = conn.prepare("
        SELECT t.id, t.title, t.assigned_to, t.deadline
        FROM tasks t
        WHERE t.deadline IS NOT NULL
          AND t.status NOT IN ('done','cancelled')
          AND t.assigned_to IS NOT NULL
          AND datetime(t.deadline) BETWEEN datetime('now') AND datetime('now','+24 hours')
          AND NOT EXISTS (
              SELECT 1 FROM notifications n
              WHERE n.task_id = t.id
                AND n.kind = 'deadline_soon'
                AND n.created_at >= datetime('now','-25 hours')
          )
    ")?;

    let tasks: Vec<(i64, String, i64, String)> = stmt.query_map([], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })?.filter_map(|r| r.ok()).collect();

    for (task_id, title, assigned_to, deadline) in tasks {
        let deadline_short = &deadline[..16.min(deadline.len())];
        push(conn, assigned_to,
            "deadline_soon",
            "Дедлайн истекает через 24 часа",
            &format!("«{}» — срок: {}", title, deadline_short),
            Some(task_id),
        )?;
        println!("[NOTIFY] Дедлайн: задача #{} → пользователь #{}", task_id, assigned_to);
    }
    Ok(())
}
