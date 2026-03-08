/// Журнал рабочего дня — автоматически собирает хронологию событий.
/// Каждое действие пользователя (старт таймера, смена статуса, комментарий,
/// загрузка файла) записывается как событие в journal.
/// API:
///   GET  /journal?date=2026-03-07          → события за день
///   GET  /journal/report?date=2026-03-07   → структурированный отчёт
///   GET  /journal/week                     → сводка за неделю

use rusqlite::{Connection, params};
use serde::Serialize;

// ── Типы событий ─────────────────────────────────────────────────────────────
#[derive(Debug, Serialize, Clone)]
pub struct JournalEntry {
    pub id         : i64,
    pub user_id    : i64,
    pub username   : String,
    pub event_type : String,   // timer_start|timer_stop|status_change|comment|file_upload|login
    pub task_id    : Option<i64>,
    pub task_title : Option<String>,
    pub detail     : String,   // человекочитаемое описание события
    pub duration_s : Option<i64>, // только для timer_stop
    pub category   : Option<i32>, // только для timer_*
    pub happened_at: String,
}

/// Дневной отчёт — агрегат по задачам и категориям
#[derive(Debug, Serialize)]
pub struct DayReport {
    pub date            : String,
    pub user_id         : i64,
    pub username        : String,
    pub work_start      : Option<String>,  // первый login / первый таймер
    pub work_end        : Option<String>,  // последнее событие
    pub total_tracked_s : i64,             // суммарное хронометрированное время
    pub cat1_s          : i64,
    pub cat2_s          : i64,
    pub cat3_s          : i64,
    pub efficiency_pct  : f64,
    pub tasks_touched   : Vec<TaskSummary>,
    pub timeline        : Vec<JournalEntry>,
    pub report_text     : String,          // готовый текст для копипасты в отчёт
}

#[derive(Debug, Serialize)]
pub struct TaskSummary {
    pub task_id    : i64,
    pub task_title : String,
    pub status     : String,
    pub time_spent_s: i64,
    pub actions    : Vec<String>,  // ["начал работу 09:15", "добавил комментарий 11:02", ...]
}

/// Недельная сводка
#[derive(Debug, Serialize)]
pub struct WeekSummary {
    pub days          : Vec<DayStat>,
    pub total_tracked_s: i64,
    pub top_tasks     : Vec<(String, i64)>,  // (название, секунды)
}

#[derive(Debug, Serialize)]
pub struct DayStat {
    pub date       : String,
    pub tracked_s  : i64,
    pub cat1_s     : i64,
    pub events_cnt : i64,
}

// ── Инициализация ─────────────────────────────────────────────────────────────
pub fn create_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("
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
    ")
}

// ── Запись события ────────────────────────────────────────────────────────────
pub fn record(conn: &Connection, user_id: i64, event_type: &str,
              task_id: Option<i64>, detail: &str,
              duration_s: Option<i64>, category: Option<i32>) -> rusqlite::Result<()>
{
    conn.execute(
        "INSERT INTO journal (user_id, event_type, task_id, detail, duration_s, category)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![user_id, event_type, task_id, detail, duration_s, category],
    )?;
    Ok(())
}

// Удобные обёртки — вызываются из handlers
pub fn on_login(conn: &Connection, user_id: i64, ip: &str) -> rusqlite::Result<()> {
    record(conn, user_id, "login", None,
           &format!("Вход в систему ({})", ip), None, None)
}
pub fn on_timer_start(conn: &Connection, user_id: i64, task_id: Option<i64>,
                      task_title: &str, category: i32) -> rusqlite::Result<()> {
    let cat_label = match category { 1=>"творческая", 2=>"вспомогательная", _=>"накладные" };
    record(conn, user_id, "timer_start", task_id,
           &format!("▶ Начал: «{}» [кат.{}  {}]", task_title, category, cat_label),
           None, Some(category))
}
pub fn on_timer_stop(conn: &Connection, user_id: i64, task_id: Option<i64>,
                     task_title: &str, category: i32, duration_s: i64) -> rusqlite::Result<()> {
    let h = duration_s / 3600;
    let m = (duration_s % 3600) / 60;
    let s = duration_s % 60;
    let dur_str = if h > 0 { format!("{}ч {:02}м {:02}с", h, m, s) }
                  else     { format!("{:02}м {:02}с", m, s) };
    record(conn, user_id, "timer_stop", task_id,
           &format!("■ Завершил: «{}» — {}", task_title, dur_str),
           Some(duration_s), Some(category))
}
pub fn on_status_change(conn: &Connection, user_id: i64, task_id: i64,
                        task_title: &str, old: &str, new: &str) -> rusqlite::Result<()> {
    let arrow = status_label(old) + " → " + &status_label(new);
    record(conn, user_id, "status_change", Some(task_id),
           &format!("↻ «{}»: {}", task_title, arrow), None, None)
}
pub fn on_comment(conn: &Connection, user_id: i64, task_id: i64,
                  task_title: &str, preview: &str) -> rusqlite::Result<()> {
    let short: String = preview.chars().take(60).collect();
    let ellipsis = if preview.len() > 60 { "…" } else { "" };
    record(conn, user_id, "comment", Some(task_id),
           &format!("💬 «{}»: «{}{}»", task_title, short, ellipsis), None, None)
}
pub fn on_file_upload(conn: &Connection, user_id: i64, task_id: Option<i64>,
                      filename: &str, size_kb: i64) -> rusqlite::Result<()> {
    record(conn, user_id, "file_upload", task_id,
           &format!("📎 Загружен файл «{}» ({} КБ)", filename, size_kb), None, None)
}
pub fn on_task_created(conn: &Connection, user_id: i64, task_id: i64,
                       title: &str) -> rusqlite::Result<()> {
    record(conn, user_id, "task_created", Some(task_id),
           &format!("+ Создана задача «{}»", title), None, None)
}

fn status_label(s: &str) -> String {
    match s {
        "inbox"=>"Входящие","backlog"=>"Бэклог","approved"=>"Утверждено",
        "in_progress"=>"В работе","review"=>"Проверка",
        "done"=>"Готово","cancelled"=>"Отменено",_ => s,
    }.to_string()
}

// ── Получить события за день ──────────────────────────────────────────────────
pub fn get_day_entries(conn: &Connection, user_id: i64, date: &str)
    -> rusqlite::Result<Vec<JournalEntry>>
{
    let mut stmt = conn.prepare("
        SELECT j.id, j.user_id, u.username, j.event_type,
               j.task_id,
               (SELECT title FROM tasks WHERE id=j.task_id),
               j.detail, j.duration_s, j.category, j.happened_at
        FROM journal j JOIN users u ON u.id=j.user_id
        WHERE j.user_id=?1
          AND date(j.happened_at)=?2
        ORDER BY j.happened_at ASC
    ")?;
    let rows = stmt.query_map(params![user_id, date], |r| Ok(JournalEntry {
        id:          r.get(0)?,
        user_id:     r.get(1)?,
        username:    r.get(2)?,
        event_type:  r.get(3)?,
        task_id:     r.get(4)?,
        task_title:  r.get(5)?,
        detail:      r.get(6)?,
        duration_s:  r.get(7)?,
        category:    r.get(8)?,
        happened_at: r.get(9)?,
    }))?;
    rows.collect()
}

// ── Дневной отчёт ─────────────────────────────────────────────────────────────
pub fn build_day_report(conn: &Connection, user_id: i64, date: &str)
    -> rusqlite::Result<DayReport>
{
    let user: (String,) = conn.query_row(
        "SELECT username FROM users WHERE id=?1", params![user_id], |r| Ok((r.get(0)?,))
    )?;
    let username = user.0;

    let entries = get_day_entries(conn, user_id, date)?;

    // Агрегация времени
    let (mut c1, mut c2, mut c3) = (0i64, 0i64, 0i64);
    for e in &entries {
        if e.event_type == "timer_stop" {
            let d = e.duration_s.unwrap_or(0);
            match e.category { Some(1)=>c1+=d, Some(2)=>c2+=d, _=>c3+=d }
        }
    }
    let total = c1 + c2 + c3;
    let eff = if c1+c2 > 0 { c1 as f64 / (c1+c2) as f64 * 100.0 } else { 0.0 };

    // Первое и последнее событие
    let work_start = entries.first().map(|e| e.happened_at[11..16].to_string());
    let work_end   = entries.last().map(|e| e.happened_at[11..16].to_string());

    // Сводка по задачам
    let mut task_map: std::collections::HashMap<i64, TaskSummary> =
        std::collections::HashMap::new();

    for e in &entries {
        if let Some(tid) = e.task_id {
            let title = e.task_title.clone().unwrap_or_else(|| format!("Задача #{}", tid));
            let entry = task_map.entry(tid).or_insert(TaskSummary {
                task_id: tid, task_title: title,
                status: String::new(),
                time_spent_s: 0, actions: vec![],
            });
            let time_str = &e.happened_at[11..16];
            entry.actions.push(format!("{} {}", time_str, e.detail));
            if e.event_type == "timer_stop" {
                entry.time_spent_s += e.duration_s.unwrap_or(0);
            }
            if e.event_type == "status_change" {
                // Последний статус из detail
                if let Some(s) = e.detail.split("→ ").last() {
                    entry.status = s.trim().to_string();
                }
            }
        }
    }
    let mut tasks_touched: Vec<TaskSummary> = task_map.into_values().collect();
    tasks_touched.sort_by(|a,b| b.time_spent_s.cmp(&a.time_spent_s));

    // Генерация текста отчёта для копипасты
    let report_text = build_report_text(
        &username, date, &work_start, &work_end,
        total, c1, c2, c3, eff, &tasks_touched, &entries,
    );

    Ok(DayReport {
        date: date.to_string(),
        user_id, username,
        work_start, work_end,
        total_tracked_s: total,
        cat1_s: c1, cat2_s: c2, cat3_s: c3,
        efficiency_pct: (eff * 10.0).round() / 10.0,
        tasks_touched,
        timeline: entries,
        report_text,
    })
}

// ── Текст отчёта ─────────────────────────────────────────────────────────────
fn build_report_text(username: &str, date: &str,
                     start: &Option<String>, end: &Option<String>,
                     total: i64, c1: i64, c2: i64, c3: i64, eff: f64,
                     tasks: &[TaskSummary], entries: &[JournalEntry]) -> String
{
    let fmt = |s: i64| {
        let h=s/3600; let m=(s%3600)/60;
        if h>0 { format!("{}ч {:02}м", h, m) } else { format!("{:02}м", m) }
    };

    let mut out = String::new();
    out += &format!("═══ ОТЧЁТ ЗА {} ═══\n", date);
    out += &format!("Сотрудник: {}\n", username);
    out += &format!("Время работы: {} — {}\n\n",
        start.as_deref().unwrap_or("—"),
        end.as_deref().unwrap_or("—"));

    out += "ЗАТРАЧЕННОЕ ВРЕМЯ:\n";
    out += &format!("  Всего хронометрировано : {}\n", fmt(total));
    out += &format!("  ① Творческое           : {}  ({:.0}%)\n",
        fmt(c1), if total>0 { c1 as f64/total as f64*100.0 } else { 0.0 });
    out += &format!("  ② Вспомогательное      : {}  ({:.0}%)\n",
        fmt(c2), if total>0 { c2 as f64/total as f64*100.0 } else { 0.0 });
    out += &format!("  ③ Накладные расходы    : {}  ({:.0}%)\n",
        fmt(c3), if total>0 { c3 as f64/total as f64*100.0 } else { 0.0 });
    out += &format!("  КПД творческого времени: {:.1}%\n\n", eff);

    out += "ЗАДАЧИ:\n";
    for t in tasks {
        out += &format!("  • {} — {}\n", t.task_title, fmt(t.time_spent_s));
    }
    out += "\n";

    out += "ХРОНОЛОГИЯ:\n";
    for e in entries {
        out += &format!("  {}  {}\n", &e.happened_at[11..16], e.detail);
    }
    out
}

// ── Недельная сводка ──────────────────────────────────────────────────────────
pub fn build_week_summary(conn: &Connection, user_id: i64) -> rusqlite::Result<WeekSummary> {
    // Статистика по дням
    let mut stmt = conn.prepare("
        SELECT date(happened_at) as d,
               SUM(CASE WHEN event_type='timer_stop' THEN duration_s ELSE 0 END),
               SUM(CASE WHEN event_type='timer_stop' AND category=1 THEN duration_s ELSE 0 END),
               COUNT(*)
        FROM journal
        WHERE user_id=?1
          AND happened_at >= datetime('now', '-7 days')
        GROUP BY d ORDER BY d DESC
    ")?;
    let days: Vec<DayStat> = stmt.query_map(params![user_id], |r| Ok(DayStat {
        date:       r.get(0)?,
        tracked_s:  r.get::<_,Option<i64>>(1)?.unwrap_or(0),
        cat1_s:     r.get::<_,Option<i64>>(2)?.unwrap_or(0),
        events_cnt: r.get(3)?,
    }))?.filter_map(|r|r.ok()).collect();

    let total = days.iter().map(|d| d.tracked_s).sum();

    // Топ задач за неделю
    let mut stmt2 = conn.prepare("
        SELECT t.title, SUM(j.duration_s) as total
        FROM journal j
        JOIN tasks t ON t.id = j.task_id
        WHERE j.user_id=?1
          AND j.event_type='timer_stop'
          AND j.happened_at >= datetime('now','-7 days')
        GROUP BY j.task_id
        ORDER BY total DESC
        LIMIT 5
    ")?;
    let top_tasks: Vec<(String,i64)> = stmt2.query_map(params![user_id], |r| {
        Ok((r.get(0)?, r.get::<_,Option<i64>>(1)?.unwrap_or(0)))
    })?.filter_map(|r|r.ok()).collect();

    Ok(WeekSummary { days, total_tracked_s: total, top_tasks })
}
