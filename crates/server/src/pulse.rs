use chrono::Local;
use protocol::*;
use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};

// (Structures are now in protocol)

pub fn create_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
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
    ",
    )
}

pub fn get_settings(conn: &Connection, user_id: i64) -> rusqlite::Result<PulseSettings> {
    let result = conn.query_row(
        "SELECT interval_min, enabled FROM pulse_settings WHERE user_id=?1",
        params![user_id],
        |r| {
            Ok(PulseSettings {
                interval_min: r.get(0)?,
                enabled: r.get::<_, i32>(1)? != 0,
            })
        },
    );
    match result {
        Ok(s) => Ok(s),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(PulseSettings {
            interval_min: 25,
            enabled: true,
        }),
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

pub fn create_question(conn: &Connection, user_id: i64) -> rusqlite::Result<i64> {
    let now = Local::now();
    let expires = now + chrono::Duration::minutes(10);
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

pub fn get_pending(conn: &Connection, user_id: i64) -> rusqlite::Result<Option<PulseQuestion>> {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut stmt = conn.prepare(
        "SELECT id, user_id, asked_at, expires_at
         FROM pulse
         WHERE user_id=?1
           AND answered_at IS NULL
           AND skipped=0
           AND expires_at > ?2
         ORDER BY asked_at DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![user_id, now])?;
    if let Some(r) = rows.next()? {
        Ok(Some(PulseQuestion {
            id: r.get(0)?,
            user_id: r.get(1)?,
            asked_at: r.get(2)?,
            expires_at: r.get(3)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn answer(
    conn: &Connection,
    pulse_id: i64,
    user_id: i64,
    activity: &str,
    task_id: Option<i64>,
    skipped: bool,
) -> rusqlite::Result<bool> {
    let now = Local::now();
    let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();

    let asked: String = conn
        .query_row(
            "SELECT asked_at FROM pulse WHERE id=?1 AND user_id=?2",
            params![pulse_id, user_id],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| now_str.clone());

    let response_s = if let (Ok(a), Ok(n)) = (
        chrono::NaiveDateTime::parse_from_str(&asked, "%Y-%m-%d %H:%M:%S"),
        chrono::NaiveDateTime::parse_from_str(&now_str, "%Y-%m-%d %H:%M:%S"),
    ) {
        (n - a).num_seconds().max(0)
    } else {
        0
    };

    let activity_to_save = if skipped {
        "—skipped—".to_string()
    } else {
        activity.to_string()
    };

    let affected = conn.execute(
        "UPDATE pulse
         SET answered_at=?1, activity=?2, task_id=?3, response_s=?4, skipped=?5
         WHERE id=?6 AND user_id=?7 AND answered_at IS NULL",
        params![
            now_str,
            activity_to_save,
            task_id,
            response_s,
            skipped as i32,
            pulse_id,
            user_id
        ],
    )?;

    if affected > 0 && !skipped {
        crate::timeline::record(
            conn,
            user_id,
            "pulse_answer",
            task_id,
            &format!(
                "🎯 Pulse: «{}»",
                &activity_to_save[..activity_to_save.len().min(80)]
            ),
            None,
            None,
        )
        .ok();
    }
    Ok(affected > 0)
}

pub fn pulse_worker(db: Arc<Mutex<Connection>>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            let conn = db.lock().unwrap();
            let now = Local::now();

            let users: Vec<(i64, i64)> = {
                let mut stmt = match conn.prepare(
                    "SELECT u.id, COALESCE(ps.interval_min, 25)
                     FROM users u
                     LEFT JOIN pulse_settings ps ON ps.user_id=u.id
                     WHERE COALESCE(ps.enabled, 1)=1",
                ) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                stmt.query_map([], |r| {
                    Ok((r.get::<usize, i64>(0)?, r.get::<usize, i64>(1)?))
                })
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|r| r.ok())
                .collect()
            };

            for (uid, interval_min) in users {
                let last: Option<String> = conn
                    .query_row(
                        "SELECT asked_at FROM pulse WHERE user_id=?1
                     ORDER BY asked_at DESC LIMIT 1",
                        params![uid],
                        |r| r.get(0),
                    )
                    .ok();

                let should_ask = match last {
                    None => true,
                    Some(last_str) => {
                        chrono::NaiveDateTime::parse_from_str(&last_str, "%Y-%m-%d %H:%M:%S")
                            .map(|dt| {
                                let elapsed = (now.naive_local() - dt).num_minutes();
                                elapsed >= interval_min
                            })
                            .unwrap_or(true)
                    }
                };

                let hour = now.format("%H").to_string().parse::<i32>().unwrap_or(0);
                let is_work_hours = (8..19).contains(&hour);

                if should_ask && is_work_hours {
                    create_question(&conn, uid).ok();
                }
            }
        }
    });
}

pub fn export_csv(conn: &Connection, user_id: i64, date: &str) -> rusqlite::Result<String> {
    let mut stmt = conn.prepare(
        "
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
    ",
    )?;

    let mut csv = String::from("Дата;Время;Деятельность;Задача;Реакция;Сотрудник\n");

    let rows = stmt.query_map(params![user_id, date], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, i32>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, Option<String>>(8)?,
        ))
    })?;

    for row in rows.filter_map(|r| r.ok()) {
        let (asked, _, activity, resp_s, _, task, username, t_asked, _) = row;
        let date_part = &asked[..10];
        let act = activity.unwrap_or_default().replace(';', ",");
        let task_title = task.unwrap_or_default().replace(';', ",");
        csv += &format!(
            "{};{};{};{};{};{}\n",
            date_part,
            t_asked,
            act,
            task_title,
            resp_s.unwrap_or(0),
            username
        );
    }

    Ok("\u{FEFF}".to_string() + &csv)
}
