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
- **🛡️ Activity Monitoring & Privacy**
  - Granular window and process tracking on Windows.
  - **Privacy Mode**: One-click toggle between "Work" and "Home" modes to protect sensitive activity.
  - **Daily Reflection**: Automated end-of-day prompts for professional self-reflection.
- **✅ Task Management**
  - Flexible status tracking (Inbox, Backlog, In Progress, Done).
  - Integrated time logging and reporting.
- **📦 Architecture**
  - **Workspace-based**: Modular design with separate crates for `server`, `database`, `protocol`, and `monitor`.
  - **Event-Driven**: Internal event bus for real-time synchronization.
  - **Offline-First**: Local hosting with bundled assets for complete data sovereignty.

### 🛠️ Tech Stack
- **Backend:** Rust 2024, Tokio, Axum (customized tiny_http), SQLite (rusqlite).
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
- **🛡️ Мониторинг и Приватность**
  - Детальное отслеживание окон и процессов в Windows.
  - **Режим Приватности**: Переключение одним кликом между режимами «Работа» и «Дом» для защиты данных.
  - **Ежедневная Рефлексия**: Автоматические опросы в конце дня для анализа продуктивности.
- **✅ Управление Задачами**
  - Гибкое отслеживание статусов (Inbox, Backlog, In Progress, Done).
  - Встроенный учет времени и генерация отчетов.
- **📦 Архитектура**
  - **Workspace**: Модульный дизайн с разделением на `server`, `database`, `protocol` и `monitor`.
  - **Event-Driven**: Внутренняя шина событий для синхронизации в реальном времени.
  - **Offline-First**: Локальный сервер с автономными ресурсами для полной безопасности данных.

### 🛠️ Стек технологий
- **Backend:** Rust 2024, Tokio, Axum (на базе tiny_http), SQLite.
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
