# ══════════════════════════════════════════════════════════
#  Portable .exe — всё в одной папке, права не нужны
# ══════════════════════════════════════════════════════════

# Итоговая структура (скопировать коллегам на флешке):
#
#  task-server/
#  ├── server.exe          ← единственный файл для запуска
#  ├── data/
#  │   ├── db.sqlite3      ← создаётся автоматически при первом запуске
#  │   └── files/          ← загруженные файлы
#  └── (всё остальное встроено в .exe: HTML, CSS, JS, SQLite)

# ── Сборка на машине разработчика ────────────────────────────────────────────

# Целевая платформа: 64-бит Windows (если собираем на Linux/Mac — кросс-компиляция)
rustup target add x86_64-pc-windows-msvc   # на Windows
# rustup target add x86_64-pc-windows-gnu  # если собираем на Linux через MinGW

# Сборка release
cargo build --release --target x86_64-pc-windows-msvc

# Готовый бинарник:
# target/x86_64-pc-windows-msvc/release/server.exe  (~6-8 МБ)

# ── Подготовить portable-архив ────────────────────────────────────────────────

mkdir -p dist/task-server/data/files
cp target/x86_64-pc-windows-msvc/release/server.exe dist/task-server/

# README для коллег внутри архива
cat > dist/task-server/ЗАПУСК.txt << 'EOF'
╔══════════════════════════════════════════════════════╗
║           СИСТЕМА УПРАВЛЕНИЯ ЗАДАЧАМИ                ║
║                  Portable версия                     ║
╠══════════════════════════════════════════════════════╣
║                                                      ║
║  1. Запустить server.exe (двойной клик)              ║
║  2. В трее появится иконка 🔵                        ║
║  3. Двойной клик по иконке → открывает браузер       ║
║     или вручную: http://localhost:8080               ║
║                                                      ║
║  Первый вход:  логин: admin  пароль: admin           ║
║  !! Сменить пароль в разделе Админ после входа !!    ║
║                                                      ║
║  Остановить: правый клик по иконке → "Стоп"          ║
║                                                      ║
║  Данные хранятся в папке data/ рядом с .exe          ║
║  Бэкап = скопировать data/db.sqlite3                 ║
╚══════════════════════════════════════════════════════╝
EOF

# Упаковать в zip
cd dist && powershell Compress-Archive task-server task-server-portable.zip

# ── Смена порта без пересборки ────────────────────────────────────────────────
# Создать рядом с .exe файл config.env:
#   PORT=9090
# И читать его при старте (добавить в main.rs):

# В main.rs добавить перед определением port:
# if let Ok(cfg) = std::fs::read_to_string(exe_dir.join("config.env")) {
#     for line in cfg.lines() {
#         if let Some((k,v)) = line.split_once('=') {
#             std::env::set_var(k.trim(), v.trim());
#         }
#     }
# }

# ── Автостарт БЕЗ прав админа (через реестр текущего пользователя) ───────────
# Не требует прав администратора! Работает только для текущего пользователя.

# Добавить в автозагрузку (cmd.exe):
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" ^
    /v "TaskServer" ^
    /t REG_SZ ^
    /d "\"C:\Users\%USERNAME%\Desktop\task-server\server.exe\"" ^
    /f

# Убрать из автозагрузки:
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v "TaskServer" /f

# Или через PowerShell (то же самое, удобнее):
$exe = "$env:USERPROFILE\Desktop\task-server\server.exe"
Set-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" `
                 -Name "TaskServer" -Value $exe

# ── Доступ с других машин в локальной сети ────────────────────────────────────
# Брандмауэр Windows может блокировать входящие соединения.
# Без прав админа — попросить коллег подключаться по IP:
#   http://192.168.1.X:8080
#
# Если брандмауэр блокирует — один раз попросить айтишника добавить правило:
#   netsh advfirewall firewall add rule name="TaskServer" ^
#         protocol=TCP dir=in localport=8080 action=allow
#
# Узнать свой IP:
#   ipconfig | findstr IPv4
