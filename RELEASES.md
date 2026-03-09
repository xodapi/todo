# Release v2.0.0 - Real-time UI & Security Hardening (Phase 2)

We are excited to announce the release of **Todo v2.0.0**, marking the completion of Phase 2. This update transforms the application from a traditional polling-based web app into a modern, real-time productivity suite with hardened security.

## 🚀 What's New

### 📡 Real-time Synchronization (WebSockets)
- **Zero Latency Metrics**: Digital Pulse metrics (active windows, typing speed, mouse distance) now update instantly via WebSockets.
- **Unified Event Bus**: Architectural shift to a full async event-driven model.
- **Live Chat**: Real-time messaging between all connected clients.

### 🛡️ Hardened Security (CSRF & Argon2id)
- **Anti-CSRF Protection**: All state-changing API requests now require signed CSRF tokens.
- **Argon2id Hashing**: Migrated user authentication to use the state-of-the-art Argon2id hashing algorithm.
- **Improved Sessions**: Introduced a "Remember me" option for 5-day persistent sessions.

### 🧠 Knowledge Base & Files
- **Full API Parity**: Completely implemented backend handlers for Note management, Visual Graphs, and File uploads.
- **Archiving Support**: Ability to archive and hide old notes.

### ⚡ Performance & Architecture
- **Axum Framework**: Migrated the core server from `tiny-http` to **Axum** (hyper-based) for superior performance and safety.
- **Async Workers**: All background monitoring tasks now run as efficient `tokio` tasks.

---
## 📦 Installation
1. Download the `server.exe` from this release.
2. Run the executable in a directory with a `static/` folder and `todo.db`.
3. Open `http://localhost:8080`.

*Built with ❤️ on Rust 2024.*
