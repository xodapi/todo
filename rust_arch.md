# Архитектура: Автономная система управления задачами (Rust + HTML)

## Принципы проектирования
- Zero dependencies на внешние серверы — только LAN
- Единый бинарник (cargo build --release) — копируется на сервер без установки
- Минимум крейтов: только то, что реально нужно
- SQLite как БД — один файл, не требует СУБД-сервера

---

## Структура проекта

```
task_server/
├── Cargo.toml
├── data/
│   └── db.sqlite3          # единственный файл данных (бэкап = копия файла)
├── static/                 # HTML/CSS/JS — встраиваются в бинарник (include_str!)
│   ├── index.html
│   ├── tasks.html
│   ├── chat.html
│   └── files.html
└── src/
    ├── main.rs             # точка входа, HTTP-роутер
    ├── db.rs               # инициализация SQLite, миграции
    ├── auth.rs             # сессии, роли, хэши паролей
    ├── handlers/
    │   ├── tasks.rs        # CRUD задач, статусы, Kanban
    │   ├── chat.rs         # сообщения (polling, не WebSocket)
    │   ├── files.rs        # загрузка/скачивание файлов
    │   └── lyubishchev.rs  # хронометраж (перенос логики из VBA)
    └── models.rs           # структуры данных (Task, User, Message...)
```

---

## Схема базы данных (SQLite)

```sql
-- Пользователи и роли
CREATE TABLE users (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    username    TEXT UNIQUE NOT NULL,
    pass_hash   TEXT NOT NULL,          -- bcrypt/SHA-256
    role        TEXT NOT NULL,          -- 'admin' | 'expert' | 'engineer' | 'viewer'
    full_name   TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Сессии (простые токены, без JWT)
CREATE TABLE sessions (
    token       TEXT PRIMARY KEY,       -- UUID v4, генерируется при логине
    user_id     INTEGER NOT NULL,
    expires_at  DATETIME NOT NULL,
    ip_address  TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id)
);

-- Задачи (GTD + Kanban + Любищев)
CREATE TABLE tasks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    title        TEXT NOT NULL,
    description  TEXT,
    source       TEXT,                  -- откуда пришла (поручение, регламент...)
    status       TEXT DEFAULT 'inbox',  -- inbox|backlog|approved|in_progress|review|done
    priority     REAL,                  -- impact/effort из GTD
    impact       INTEGER DEFAULT 3,
    effort       INTEGER DEFAULT 3,
    is_urgent    BOOLEAN DEFAULT 0,
    is_important BOOLEAN DEFAULT 1,
    approved_by  INTEGER,               -- FK users.id (роль Expert)
    assigned_to  INTEGER,               -- FK users.id (роль Engineer)
    created_by   INTEGER NOT NULL,
    created_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
    started_at   DATETIME,
    finished_at  DATETIME,
    deadline     DATETIME,
    FOREIGN KEY (approved_by) REFERENCES users(id),
    FOREIGN KEY (assigned_to) REFERENCES users(id),
    FOREIGN KEY (created_by) REFERENCES users(id)
);

-- Хронометраж Любищева
CREATE TABLE time_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id     INTEGER,
    user_id     INTEGER NOT NULL,
    category    INTEGER NOT NULL,       -- 1=творчество, 2=рутина, 3=отдых/быт
    started_at  DATETIME NOT NULL,
    finished_at DATETIME,
    duration_s  INTEGER,               -- секунды (считается при STOP)
    note        TEXT,
    FOREIGN KEY (task_id) REFERENCES tasks(id),
    FOREIGN KEY (user_id) REFERENCES users(id)
);

-- Чат (простой, без комнат на старте)
CREATE TABLE messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL,
    task_id     INTEGER,               -- NULL = общий чат, число = привязан к задаче
    body        TEXT NOT NULL,
    sent_at     DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id),
    FOREIGN KEY (task_id) REFERENCES tasks(id)
);

-- Файлы
CREATE TABLE files (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id      INTEGER,              -- NULL = общий файл
    uploaded_by  INTEGER NOT NULL,
    filename     TEXT NOT NULL,
    stored_name  TEXT NOT NULL,        -- UUID.ext (чтобы не было конфликтов)
    size_bytes   INTEGER,
    uploaded_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (task_id) REFERENCES tasks(id),
    FOREIGN KEY (uploaded_by) REFERENCES users(id)
);
```

---

## Ролевая модель

| Роль      | Inbox | Backlog | Утверждать | Назначать | Хронометраж | Чат | Файлы | Аналитика |
|-----------|:-----:|:-------:|:----------:|:---------:|:-----------:|:---:|:-----:|:---------:|
| admin     | ✓     | ✓       | ✓          | ✓         | ✓           | ✓   | ✓     | ✓         |
| expert    | ✓     | ✓       | ✓          | ✓         | ✓           | ✓   | ✓     | ✓         |
| engineer  | ✓     | чтение  | ✗          | ✗         | ✓           | ✓   | ✓     | свои      |
| viewer    | ✗     | чтение  | ✗          | ✗         | ✗           | ✓   | ✗     | общая     |

---

## HTTP API (минималистичный REST)

```
POST /auth/login            → { token, role, username }
POST /auth/logout

GET  /tasks                 → список (фильтры: ?status=&assigned=&urgent=)
POST /tasks                 → создать задачу
PUT  /tasks/:id             → обновить (статус, назначить, утвердить)
GET  /tasks/:id/messages    → чат по задаче

GET  /chat?since=TIMESTAMP  → polling (новые сообщения с момента)
POST /chat                  → отправить сообщение { task_id?, body }

POST /time/start            → { task_id, category } → { log_id, started_at }
POST /time/stop/:log_id     → записывает duration_s
GET  /time/report?user=&days= → аналитика по Любищеву

POST /files/upload          → multipart/form-data
GET  /files/:id             → скачать файл
GET  /files?task_id=        → список файлов задачи

GET  /analytics/eisenhower  → матрица Эйзенхауэра (JSON)
GET  /analytics/lyubishchev → сводка по категориям
```

---

## Крейты (Cargo.toml)

```toml
[dependencies]
# HTTP-сервер (без async runtime — синхронный, проще и надёжнее для LAN)
tiny_http = "0.12"

# SQLite (статическая линковка — нет зависимости от DLL)
rusqlite = { version = "0.31", features = ["bundled"] }

# Сериализация JSON
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Хэши паролей (SHA-256, без внешних C-библиотек)
sha2 = "0.10"

# UUID для токенов и имён файлов
uuid = { version = "1", features = ["v4"] }

# Дата/время
chrono = { version = "0.4", features = ["serde"] }
```

**Итог: 7 крейтов, бинарник ~5-8 МБ, RAM ~10-30 МБ под нагрузкой.**

---

## Схема взаимодействия

```
Пользователь (браузер)
        │  HTTP :8080
        ▼
┌─────────────────────────────────┐
│       main.rs (роутер)          │
│  tiny_http → match request.url  │
├──────────┬──────────┬───────────┤
│ auth.rs  │tasks.rs  │ chat.rs   │
│ sessions │ CRUD     │ polling   │
└──────────┴──────────┴───────────┤
              db.rs (rusqlite)     │
              data/db.sqlite3 ◄───┘
```

---

## Интеграция с Excel VBA (существующие наработки)

Excel отправляет HTTP-запросы через MSXML2.XMLHTTP к серверу в LAN:

```vba
' Пример: старт таймера Любищева из Excel
Sub StartTimerViaAPI()
    Dim http As Object
    Set http = CreateObject("MSXML2.XMLHTTP")
    http.Open "POST", "http://192.168.1.100:8080/time/start", False
    http.setRequestHeader "Authorization", "Bearer " & GetStoredToken()
    http.setRequestHeader "Content-Type", "application/json"
    http.Send "{""task_id"":42, ""category"":1}"
    ' Ответ: { "log_id": 7, "started_at": "2026-03-06T09:15:00" }
End Sub
```

ТРИЗ-матрица остаётся в Excel как справочник — серверу она не нужна.
Диаграмма Ганта строится в Excel из данных, полученных через GET /tasks.
