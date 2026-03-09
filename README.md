# 📔 Todo & Knowledge Base (KB)

[English](#english) | [Русский](#русский)

---

## English

### Overview
**Todo** is a modern, high-performance personal productivity suite built with **Rust 2024**. It integrates task management, deep activity monitoring, and a Zettelkasten-inspired Knowledge Base into a single, cohesive workflow designed for developers and researchers.

### 🌟 Key Features
- **🧠 Knowledge Base (KB)**
  - File-based storage (`.md` files) for maximum portability.
  - Full **Markdown** support with interactive **Mermaid** diagrams.
  - Zettelkasten implementation with bidirectional links and a **Visual Graph**.
  - Hierarchical note organization and archiving support.
- **🛡️ Activity Monitoring & Security**
  - Granular window and process tracking on Windows.
  - **Real-time Synchronization**: Powered by **WebSockets** for instant UI updates.
  - **CSRF Protection**: Hardened API security with token validation.
  - **Session Persistence**: Optional 5-day session ("Remember me").
  - **Privacy Mode**: One-click toggle between "Work" and "Home" modes to protect sensitive activity.
  - **Daily Reflection**: Automated end-of-day prompts for professional self-reflection.
- **✅ Task Management**
  - Flexible status tracking (Inbox, Backlog, In Progress, Done).
  - Integrated time logging and reporting.
- **📦 Architecture**
  - **Workspace-based**: Modular design with separate crates for `server`, `database`, `protocol`, and `monitor`.
  - **WebSockets / Event-Driven**: Real-time event broadcasting to all connected clients.
  - **Offline-First**: Local hosting with bundled assets for complete data sovereignty.

### 🛠️ Tech Stack
- **Backend:** Rust 2024, Tokio, Axum (Real-time WebSockets), SQLite (rusqlite).
- **Frontend:** Vanilla JS, HTML5, CSS3 (Modern Glassmorphism UI).
- **Libraries:** Marked.js, Mermaid.js, Tracing, Serde.

### 🚀 Getting Started
1. **Prerequisites**: [Rust Toolchain](https://rustup.rs/) (v1.85+).
2. **Build & Run**:
   ```powershell
   cargo run -p server
   ```
3. **Access**: Open `http://localhost:8080` in your browser.

---

## Русский

### Обзор
**Todo** — это современный высокопроизводительный инструмент для личной продуктивности на базе **Rust 2024**. Он объединяет управление задачами, глубокий мониторинг активности и базу знаний Zettelkasten в единый рабочий процесс, оптимизированный для разработчиков и исследователей.

### 🌟 Основные возможности
- **🧠 База Знаний (KB)**
  - Файловое хранение (`.md`) для максимальной мобильности данных.
  - Поддержка **Markdown** с интерактивными диаграммами **Mermaid**.
  - Реализация Zettelkasten с двусторонними ссылками и **визуальным графом**.
  - Иерархическая организация заметок и поддержка архивации.
- **🛡️ Мониторинг и Безопасность**
  - Детальное отслеживание окон и процессов в Windows.
  - **Синхронизация в реальном времени**: Использование **WebSockets** для мгновенных обновлений.
  - **Защита CSRF**: Усиленная безопасность API с проверкой токенов.
  - **Стойкие сессии**: Опциональный вход на 5 дней («Запомнить меня»).
  - **Режим Приватности**: Переключение одним кликом между режимами «Работа» и «Дом».
  - **Ежедневная Рефлексия**: Автоматические опросы в конце дня для анализа продуктивности.
- **✅ Управление Задачами**
  - Гибкое отслеживание статусов (Inbox, Backlog, In Progress, Done).
  - Встроенный учет времени и генерация отчетов.
- **📦 Архитектура**
  - **Workspace**: Модульный дизайн (`server`, `database`, `protocol`, `monitor`).
  - **WebSockets / Event-Driven**: Вещание событий всем подключенным клиентам в real-time.
  - **Offline-First**: Локальный сервер с автономными ресурсами.

### 🛠️ Стек технологий
- **Backend:** Rust 2024, Tokio, Axum (WebSockets), SQLite.
- **Frontend:** Vanilla JS, HTML5, CSS3 (современный стекляный интерфейс).
- **Библиотеки:** Marked.js, Mermaid.js, Tracing, Serde.

### 🚀 Быстрый старт
1. **Требования**: [Rust Toolchain](https://rustup.rs/) (версия 1.85+).
2. **Запуск**:
   ```powershell
   cargo run -p server
   ```
3. **Доступ**: Откройте `http://localhost:8080` в браузере.

---

## 🏗️ Project Structure
```text
.
├── crates/
│   ├── server/       # Main entry point, HTTP API, and UI delivery
│   ├── database/     # SQLite schema and CRUD operations
│   ├── monitor/      # Windows activity tracking engine
│   ├── protocol/     # Shared data structures and types
│   └── event_bus/    # Internal pub/sub for cross-crate events
├── static/           # Frontend assets (HTML, CSS, JS)
├── kb_notes/         # Your knowledge base entries (.md)
└── data/             # SQLite database file and uploads
```

## 📜 Development Rules
- Use `cargo clippy` and `cargo fmt` before every commit.
- Follow **Conventional Commits**.
- Errors must be handled via `thiserror` (lib) or `anyhow` (bin).
- No direct `unwrap()` in library code.

---
*Created with ❤️ for high-performing teams.*
