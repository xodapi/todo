/// HTTP-обработчики для журнала рабочего дня.
/// Добавить в main.rs роутер:
///   ("GET", ["journal", "report"]) => handle_journal_report(&req, &db),
///   ("GET", ["journal", "week"])   => handle_journal_week(&req, &db),
///   ("GET", ["journal"])           => handle_journal_raw(&req, &db),

use crate::{Db, Resp, json_resp, query_params, auth, timeline, models::ApiError};

/// GET /journal?date=2026-03-07 → сырые события за день
pub fn handle_journal_raw(req: &tiny_http::Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u)  => u,
        Err(r) => return r,
    };
    let params = query_params(req.url());
    let date   = params.get("date")
        .map(|s| s.clone())
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

    // Expert/Admin могут смотреть журнал любого пользователя
    let uid: i64 = if user.role.can_approve() {
        params.get("user").and_then(|u| u.parse().ok()).unwrap_or(user.id)
    } else { user.id };

    match timeline::get_day_entries(&conn, uid, &date) {
        Ok(entries) => json_resp(200, &serde_json::to_string(&entries).unwrap()),
        Err(e)      => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

/// GET /journal/report?date=2026-03-07 → структурированный дневной отчёт
pub fn handle_journal_report(req: &tiny_http::Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u)  => u,
        Err(r) => return r,
    };
    let params = query_params(req.url());
    let date   = params.get("date")
        .map(|s| s.clone())
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

    let uid: i64 = if user.role.can_approve() {
        params.get("user").and_then(|u| u.parse().ok()).unwrap_or(user.id)
    } else { user.id };

    match timeline::build_day_report(&conn, uid, &date) {
        Ok(report) => json_resp(200, &serde_json::to_string(&report).unwrap()),
        Err(e)     => json_resp(500, &ApiError::json(&e.to_string())),
    }
}

/// GET /journal/week → недельная сводка текущего пользователя
pub fn handle_journal_week(req: &tiny_http::Request, db: &Db) -> Resp {
    let conn = db.lock().unwrap();
    let user = match auth::require_user(&conn, req) {
        Ok(u)  => u,
        Err(r) => return r,
    };
    let params = query_params(req.url());
    let uid: i64 = if user.role.can_approve() {
        params.get("user").and_then(|u| u.parse().ok()).unwrap_or(user.id)
    } else { user.id };

    match timeline::build_week_summary(&conn, uid) {
        Ok(summary) => json_resp(200, &serde_json::to_string(&summary).unwrap()),
        Err(e)      => json_resp(500, &ApiError::json(&e.to_string())),
    }
}


// ════════════════════════════════════════════════════════
//  ПАТЧ: куда добавить вызовы timeline::record(...)
//  в уже существующих handlers (main.rs)
// ════════════════════════════════════════════════════════

// ─ handle_login ──────────────────────────────────────────
// После успешного входа добавить:
//
//   let ip = req.remote_addr().map(|a|a.to_string()).unwrap_or_default();
//   timeline::on_login(&conn, user.id, &ip).ok();

// ─ handle_create_task ────────────────────────────────────
// После Ok(id) добавить:
//
//   timeline::on_task_created(&conn, user.id, id, &body.title).ok();

// ─ handle_update_task ────────────────────────────────────
// После успешного update_task добавить:
//
//   if let Some(new_status) = &body.status {
//       if let Ok(Some(task)) = db::get_task(&conn, id) {
//           timeline::on_status_change(
//               &conn, user.id, id,
//               &task.title,
//               task.status.as_str(),
//               new_status,
//           ).ok();
//       }
//   }

// ─ handle_start_timer ────────────────────────────────────
// После Ok(log_id) добавить:
//
//   let task_title = body.task_id
//       .and_then(|tid| db::get_task(&conn, tid).ok().flatten())
//       .map(|t| t.title)
//       .unwrap_or_else(|| "без задачи".to_string());
//   timeline::on_timer_start(&conn, user.id, body.task_id,
//                             &task_title, body.category).ok();

// ─ handle_stop_timer ─────────────────────────────────────
// После Ok(true) добавить:
//
//   // Берём данные из time_log по log_id
//   if let Ok(Some(log)) = db::get_time_log_entry(&conn, id) {
//       let task_title = log.task_id
//           .and_then(|tid| db::get_task(&conn, tid).ok().flatten())
//           .map(|t| t.title)
//           .unwrap_or_else(|| "без задачи".to_string());
//       timeline::on_timer_stop(
//           &conn, user.id, log.task_id,
//           &task_title, log.category,
//           log.duration_s.unwrap_or(0),
//       ).ok();
//   }

// ─ handle_send_message ───────────────────────────────────
// После Ok(id) — только для сообщений привязанных к задаче:
//
//   if let Some(tid) = body.task_id {
//       if let Ok(Some(task)) = db::get_task(&conn, tid) {
//           timeline::on_comment(&conn, user.id, tid,
//                                 &task.title, &body.body).ok();
//           // @упоминания → уведомления (уже есть в notifications)
//           notifications::on_mention(&conn, Some(tid),
//                                      &user.username, &body.body).ok();
//       }
//   }

// ─ handle_upload_file ────────────────────────────────────
// После успешной записи в БД добавить:
//
//   timeline::on_file_upload(&conn, user.id, task_id,
//                             &safe_name, size / 1024).ok();
