/// Pulse — периодические авто-вопросы «что делаете прямо сейчас?»
///
/// Логика:
///   1. Каждые N минут (по умолчанию 25) сервер создаёт запись pulse
///      со статусом pending для каждого активного пользователя.
///   2. Браузер polling'ом (GET /pulse/pending) забирает вопрос и
///      показывает всплывающее окно.
///   3. Пользователь отвечает (POST /pulse/:id/answer).
///   4. Ответы агрегируются в journal и идут в CSV-отчёт.
///
/// API:
///   GET  /pulse/pending          → { id, asked_at } | null
///   POST /pulse/:id/answer       → { activity, task_id? }
///   GET  /pulse/settings         → { interval_min, enabled }
///   PUT  /pulse/settings         → { interval_min, enabled }
///   GET  /pulse/export?date=     → CSV (Content-Type: text/csv)

use rusqlite::{Connection, params};
use serde::{Serialize, Deserialize};
use chrono::Local;
use std::sync::{Arc, Mutex};

// ── Структуры ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct PulseQuestion {
    pub id        : i64,
    pub user_id   : i64,
    pub asked_at  : String,
    pub expires_at: String,   // если не ответил за 10 мин — пропущен
}

#[derive(Debug, Serialize, Clone)]
pub struct PulseAnswer {
    pub id          : i64,
    pub user_id     : i64,
    pub username    : String,
    pub asked_at    : String,
    pub answered_at : String,
    pub activity    : String,   // что делал
    pub task_id     : Option<i64>,
    pub task_title  : Option<String>,
    pub response_s  : i64,      // секунд до ответа
    pub skipped     : bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PulseSettings {
    pub interval_min: i64,    // 15 | 25 | 30 | 45 | 60
    pub enabled     : bool,
}

#[derive(Debug, Deserialize)]
pub struct AnswerRequest {
    pub activity: String,
    pub task_id : Option<i64>,
    pub skipped : Option<bool>,
}

// ── Инициализация ─────────────────────────────────────────────────────────────

pub fn create_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS pulse (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id     INTEGER NOT NULL,
            asked_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            expires_at  DATETIME NOT NULL,
            answered_at DATETIME,
            activity    TEXT,
            task_id     INTEGER,
            response_s  INTEGER,
            skipped     INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (user_id) REFERENCES users(id),
            FOREIGN KEY (task_id) REFERENCES tasks(id)
        );

        CREATE TABLE IF NOT EXISTS pulse_settings (
            user_id      INTEGER PRIMARY KEY,
            interval_min INTEGER NOT NULL DEFAULT 25,
            enabled      INTEGER NOT NULL DEFAULT 1,
            FOREIGN KEY (user_id) REFERENCES users(id)
        );

        CREATE INDEX IF NOT EXISTS idx_pulse_user
            ON pulse(user_id, asked_at);
    ")
}

// ── Настройки ─────────────────────────────────────────────────────────────────

pub fn get_settings(conn: &Connection, user_id: i64) -> rusqlite::Result<PulseSettings> {
    let result = conn.query_row(
        "SELECT interval_min, enabled FROM pulse_settings WHERE user_id=?1",
        params![user_id],
        |r| Ok(PulseSettings { interval_min: r.get(0)?, enabled: r.get::<_,i32>(1)?!=0 })
    );
    // Дефолт если нет записи
    match result {
        Ok(s) => Ok(s),
        Err(rusqlite::Error::QueryReturnedNoRows) =>
            Ok(PulseSettings { interval_min: 25, enabled: true }),
        Err(e) => Err(e),
    }
}

pub fn save_settings(conn: &Connection, user_id: i64, s: &PulseSettings) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO pulse_settings (user_id, interval_min, enabled)
         VALUES (?1, ?2, ?3)",
        params![user_id, s.interval_min, s.enabled as i32],
    )?;
    Ok(())
}

// ── Создать вопрос для пользователя ──────────────────────────────────────────

pub fn create_question(conn: &Connection, user_id: i64) -> rusqlite::Result<i64> {
    let now     = Local::now();
    let expires = now + chrono::Duration::minutes(10); // 10 мин на ответ
    conn.execute(
        "INSERT INTO pulse (user_id, asked_at, expires_at)
         VALUES (?1, ?2, ?3)",
        params![
            user_id,
            now.format("%Y-%m-%d %H:%M:%S").to_string(),
            expires.format("%Y-%m-%d %H:%M:%S").to_string(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

// ── Получить активный (непросроченный, без ответа) вопрос ────────────────────

pub fn get_pending(conn: &Connection, user_id: i64)
    -> rusqlite::Result<Option<PulseQuestion>>
{
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut stmt = conn.prepare(
        "SELECT id, user_id, asked_at, expires_at
         FROM pulse
         WHERE user_id=?1
           AND answered_at IS NULL
           AND skipped=0
           AND expires_at > ?2
         ORDER BY asked_at DESC LIMIT 1"
    )?;
    let mut rows = stmt.query(params![user_id, now])?;
    if let Some(r) = rows.next()? {
        Ok(Some(PulseQuestion {
            id:         r.get(0)?,
            user_id:    r.get(1)?,
            asked_at:   r.get(2)?,
            expires_at: r.get(3)?,
        }))
    } else {
        Ok(None)
    }
}

// ── Записать ответ ────────────────────────────────────────────────────────────

pub fn answer(conn: &Connection, pulse_id: i64, user_id: i64,
              req: &AnswerRequest) -> rusqlite::Result<bool>
{
    let now = Local::now();
    let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();

    // Считаем response_s
    let asked: String = conn.query_row(
        "SELECT asked_at FROM pulse WHERE id=?1 AND user_id=?2",
        params![pulse_id, user_id], |r| r.get(0)
    ).unwrap_or_else(|_| now_str.clone());

    let response_s = if let (Ok(a), Ok(n)) = (
        chrono::NaiveDateTime::parse_from_str(&asked, "%Y-%m-%d %H:%M:%S"),
        chrono::NaiveDateTime::parse_from_str(&now_str, "%Y-%m-%d %H:%M:%S"),
    ) {
        (n - a).num_seconds().max(0)
    } else { 0 };

    let skipped = req.skipped.unwrap_or(false);
    let activity = if skipped { "—пропущено—".to_string() } else { req.activity.clone() };

    let affected = conn.execute(
        "UPDATE pulse
         SET answered_at=?1, activity=?2, task_id=?3, response_s=?4, skipped=?5
         WHERE id=?6 AND user_id=?7 AND answered_at IS NULL",
        params![now_str, activity, req.task_id, response_s, skipped as i32,
                pulse_id, user_id],
    )?;

    // Пишем в journal
    if affected > 0 && !skipped {
        crate::timeline::record(
            conn, user_id, "pulse_answer", req.task_id,
            &format!("🎯 Pulse: «{}»", &activity[..activity.len().min(80)]),
            None, None,
        ).ok();
    }
    Ok(affected > 0)
}

// ── Фоновый воркер: генерация вопросов по расписанию ────────────────────────

pub fn pulse_worker(db: Arc<Mutex<Connection>>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60)); // проверка каждую минуту

            let conn = db.lock().unwrap();
            let now  = Local::now();

            // Получаем всех пользователей с включённым pulse
            let users: Vec<(i64, i64)> = {
                let mut stmt = match conn.prepare(
                    "SELECT u.id, COALESCE(ps.interval_min, 25)
                     FROM users u
                     LEFT JOIN pulse_settings ps ON ps.user_id=u.id
                     WHERE COALESCE(ps.enabled, 1)=1"
                ) { Ok(s) => s, Err(_) => continue };

                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                    .unwrap_or_else(|_| Box::new(std::iter::empty()))
                    .filter_map(|r| r.ok())
                    .collect()
            };

            for (uid, interval_min) in users {
                // Когда был последний вопрос?
                let last: Option<String> = conn.query_row(
                    "SELECT asked_at FROM pulse WHERE user_id=?1
                     ORDER BY asked_at DESC LIMIT 1",
                    params![uid], |r| r.get(0)
                ).ok();

                let should_ask = match last {
                    None => true, // первый раз
                    Some(last_str) => {
                        chrono::NaiveDateTime::parse_from_str(&last_str, "%Y-%m-%d %H:%M:%S")
                            .map(|dt| {
                                let elapsed = (now.naive_local() - dt).num_minutes();
                                elapsed >= interval_min
                            })
                            .unwrap_or(true)
                    }
                };

                // Не беспокоить вне рабочего времени (8:00–19:00)
                let hour = now.format("%H").to_string().parse::<i32>().unwrap_or(0);
                let is_work_hours = hour >= 8 && hour < 19;

                if should_ask && is_work_hours {
                    create_question(&conn, uid).ok();
                }
            }
        }
    });
}

// ── CSV-экспорт ───────────────────────────────────────────────────────────────

pub fn export_csv(conn: &Connection, user_id: i64, date: &str) -> rusqlite::Result<String> {
    let mut stmt = conn.prepare("
        SELECT
            p.asked_at,
            p.answered_at,
            p.activity,
            p.response_s,
            p.skipped,
            t.title,
            u.username,
            strftime('%H:%M', p.asked_at)  as time_asked,
            strftime('%H:%M', p.answered_at) as time_ans
        FROM pulse p
        JOIN users u ON u.id=p.user_id
        LEFT JOIN tasks t ON t.id=p.task_id
        WHERE p.user_id=?1
          AND date(p.asked_at)=?2
        ORDER BY p.asked_at ASC
    ")?;

    let mut csv = String::from(
        "Дата;Время вопроса;Время ответа;Чем занят;Задача;Время реакции (сек);Пропущен;Сотрудник\n"
    );

    let rows = stmt.query_map(params![user_id, date], |r| {
        Ok((
            r.get::<_,String>(0)?,  // asked_at
            r.get::<_,Option<String>>(1)?,  // answered_at
            r.get::<_,Option<String>>(2)?,  // activity
            r.get::<_,Option<i64>>(3)?,     // response_s
            r.get::<_,i32>(4)?,             // skipped
            r.get::<_,Option<String>>(5)?,  // task title
            r.get::<_,String>(6)?,          // username
            r.get::<_,String>(7)?,          // time_asked HH:MM
            r.get::<_,Option<String>>(8)?,  // time_ans
        ))
    })?;

    for row in rows.filter_map(|r| r.ok()) {
        let (asked, _answered, activity, resp_s, skipped, task, username, t_asked, t_ans) = row;
        let date_part = &asked[..10];
        let activity  = activity.unwrap_or_default().replace(';', ",");
        let task      = task.unwrap_or_default().replace(';', ",");
        let t_ans     = t_ans.unwrap_or_else(|| "—".to_string());
        let resp      = resp_s.map(|s| s.to_string()).unwrap_or_default();
        let skip_str  = if skipped != 0 { "да" } else { "нет" };

        csv += &format!("{};{};{};{};{};{};{};{}\n",
            date_part, t_asked, t_ans, activity, task, resp, skip_str, username);
    }

    // BOM для корректного открытия в Excel (кириллица)
    Ok("\u{FEFF}".to_string() + &csv)
}

// Полный CSV за период (для экспорта по диапазону дат)
pub fn export_csv_range(conn: &Connection, user_id: i64,
                         date_from: &str, date_to: &str) -> rusqlite::Result<String>
{
    let mut stmt = conn.prepare("
        SELECT
            date(p.asked_at)                as day,
            strftime('%H:%M', p.asked_at)   as t_ask,
            strftime('%H:%M', p.answered_at) as t_ans,
            COALESCE(p.activity, '—')        as activity,
            COALESCE(t.title, '')            as task_title,
            COALESCE(p.response_s, 0)        as resp_s,
            p.skipped,
            u.username
        FROM pulse p
        JOIN users u ON u.id = p.user_id
        LEFT JOIN tasks t ON t.id = p.task_id
        WHERE p.user_id=?1
          AND date(p.asked_at) BETWEEN ?2 AND ?3
        ORDER BY p.asked_at ASC
    ")?;

    let mut csv = String::from(
        "Дата;Время вопроса;Время ответа;Чем занят;Задача;Реакция (сек);Пропущен;Сотрудник\n"
    );

    let rows = stmt.query_map(params![user_id, date_from, date_to], |r| {
        Ok((
            r.get::<_,String>(0)?,
            r.get::<_,String>(1)?,
            r.get::<_,Option<String>>(2)?.unwrap_or_else(||"—".into()),
            r.get::<_,String>(3)?.replace(';', ","),
            r.get::<_,String>(4)?.replace(';', ","),
            r.get::<_,i64>(5)?,
            r.get::<_,i32>(6)?,
            r.get::<_,String>(7)?,
        ))
    })?;

    for (day,ta,tb,act,task,resp,skip,user) in rows.filter_map(|r|r.ok()) {
        csv += &format!("{};{};{};{};{};{};{};{}\n",
            day, ta, tb, act, task, resp,
            if skip!=0{"да"}else{"нет"}, user);
    }
    Ok("\u{FEFF}".to_string() + &csv)
}
