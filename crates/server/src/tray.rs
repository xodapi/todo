/// Трей-иконка для Windows.
/// Показывает статус сервера, порт, ссылку «Открыть», кнопку «Стоп».
/// Работает без прав администратора — чистый Win32 API через tray-icon.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::{
    TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

/// Запустить трей в текущем потоке (должен быть главный поток на Windows).
/// `running` — флаг: false = сервер должен остановиться.
/// `port`    — порт для отображения в подсказке и пункте меню.
pub fn run_tray(running: Arc<AtomicBool>, port: u16) {
    // ── Иконка (встроенная PNG 32x32, закодированная прямо в коде) ──────────
    // Минималистичная иконка: синий круг с белым треугольником «play»
    // В реальном проекте: include_bytes!("../assets/icon.ico")
    let icon = build_icon();

    // ── Меню ─────────────────────────────────────────────────────────────────
    let menu = Menu::new();

    let item_title = MenuItem::new(format!("⚙  Сервер запущен  :{}", port), false, None);
    let item_open = MenuItem::new("🌐  Открыть в браузере", true, None);
    let item_sep = PredefinedMenuItem::separator();
    let item_stop = MenuItem::new("■  Остановить сервер", true, None);

    // Сохраняем id для обработки кликов
    let id_open = item_open.id().clone();
    let id_stop = item_stop.id().clone();

    menu.append_items(&[&item_title, &item_open, &item_sep, &item_stop])
        .ok();

    // ── Иконка в трее ────────────────────────────────────────────────────────
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(format!("Система задач — порт {}", port))
        .with_icon(icon)
        .build()
        .expect("Не удалось создать иконку трея");

    // ── Event loop (winit) ───────────────────────────────────────────────────
    // Используем EventLoop без окна — только для обработки событий трея
    use tao::platform::windows::EventLoopBuilderExtWindows;
    let event_loop = EventLoopBuilder::new().with_any_thread(true).build();
    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Обработка кликов по меню
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == id_open {
                open_browser(port);
            }
            if event.id == id_stop {
                running.store(false, Ordering::SeqCst);
                *control_flow = ControlFlow::Exit;
            }
        }

        // Двойной клик по иконке
        if let Ok(TrayIconEvent::DoubleClick { .. }) = TrayIconEvent::receiver().try_recv() {
            open_browser(port);
        }

        if !running.load(Ordering::SeqCst) {
            *control_flow = ControlFlow::Exit;
        }
    });
}

// ── Открыть браузер без прав ──────────────────────────────────────────────────
fn open_browser(port: u16) {
    let url = format!("http://127.0.0.1:{}", port);
    // start "" — стандартный способ открыть URL из cmd без прав
    std::process::Command::new("cmd")
        .args(["/c", "start", "", &url])
        .spawn()
        .ok();
}

// ── Генерация иконки 32×32 прямо в коде (без файла) ─────────────────────────
// Тёмный фон + белая буква "T" (task). Заменить на include_bytes! если есть .ico
fn build_icon() -> tray_icon::Icon {
    const SIZE: usize = 32;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let i = (y * SIZE + x) * 4;
            let cx = x as i32 - 16;
            let cy = y as i32 - 16;
            let dist = ((cx * cx + cy * cy) as f32).sqrt();

            if dist < 14.0 {
                // Синий круг
                rgba[i] = 79; // R
                rgba[i + 1] = 142; // G
                rgba[i + 2] = 247; // B
                rgba[i + 3] = 255; // A
            } else {
                // Прозрачный фон
                rgba[i + 3] = 0;
            }

            // Белая буква "T" по центру
            let in_horiz = (-10..=-6).contains(&cy) && (-8..=8).contains(&cx);
            let in_vert = (-2..=2).contains(&cx) && (-10..=8).contains(&cy);
            if in_horiz || in_vert {
                rgba[i] = 255;
                rgba[i + 1] = 255;
                rgba[i + 2] = 255;
                rgba[i + 3] = 255;
            }
        }
    }

    tray_icon::Icon::from_rgba(rgba, SIZE as u32, SIZE as u32).expect("Ошибка создания иконки")
}
