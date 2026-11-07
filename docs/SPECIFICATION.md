# Kira Desktop · native GTK4 specification

**Version:** native-gtk architectural reset, 2026-11-07. Experimental migration branch, no claim of full MVP feature parity.

## 1. Назначение

Kira Desktop — модульная нативная оболочка рабочего стола для Niri/Wayland, ориентированная на NixOS. Задача проекта — обеспечить единый системный интерфейс с прозрачным владением состоянием, достоверными метриками и предсказуемыми действиями. Это не браузерный dashboard и не собственный оконный менеджер.

## 2. Обязательный стек

- Rust 2024, Cargo workspace: kira-core, kira-ipc, kira-shell.
- GTK4 + gtk4-layer-shell, GIO, GDK/GSK, CSS: единственная визуальная платформа.
- Tokio: асинхронные сервисы Core; Unix socket + JSON-RPC 2.0: типизированный протокол.
- Niri/Wayland: композитор, источники workspace/output state и оконная модель.
- KDL: конфигурация с проверкой и hot reload; Nix/Crane: воспроизводимые сборки.
- WebKitGTK, Svelte, Tailwind, Vite, JavaScript, ECharts и GSAP **не входят** в новую архитектуру.

## 3. Компоненты

**Kira Core** владеет Niri EventStream, KDL parser/validation, потоками состояния, health heartbeat и ограниченной авторизованной сменой workspace.

**Kira IPC** фиксирует контракты, ограничения размера сообщения, тайм-ауты, reconnect и типизированные ошибки.

**Kira Shell** создаёт нативный GTK4 Top Bar, отдельные popup surfaces, обычные Niri-managed окна, UI-контроллеры и GIO/Linux системные адаптеры. GTK не блокируется IO. Все объекты GTK принадлежат главному потоку.

## 4. Поверхности и жизненный цикл

Top Bar: 42 логических px, Layer Top, якоря top/left/right, exclusive zone 42. Popup: exclusive zone 0, один transient. Launcher расположен сверху по центру; остальные overlay предпочитают верхний правый угол с ограничением по геометрии вывода. Отстыкованные виджеты живут независимо как обычные Niri окна.

Виджеты создаются по запросу. Хранимое состояние принадлежит контроллеру; pop-out должен перемещать тот же экземпляр GTK-контента, когда обещается сохранение состояния. Сложные motion эффекты обязаны поддерживать прерывание, точный reset и reduced motion.

## 5. Принципы защиты

Никаких произвольных OS-команд из UI. Действие workspace проверяется Core по реальному текущему снимку. Приложения запускаются через GIO по идентичности AppInfo. Неподдерживаемый ресурс отображается как недоступный. Внешние KDL/Nix источники не переписываются автоматически. Display Settings: отдельные Apply, Confirm (с rollback) и Save to config.

Нативная динамическая загрузка стороннего кода без изоляции **не** является поддерживаемой sandbox-моделью расширений.

## 6. Визуальная система

Каноническая палитра: #000000, #14213d, #fca311, #e5e5e5, #ffffff. Семантические стили располагаются в GTK CSS. Данные в системных панелях фактические, а не декоративные. Motion подчиняется пяти законам TENET, но не JS API. Клавиатура, фокус, a11y и reduced motion обязательны.

## 7. Инженерная приемка

Cargo fmt, Clippy и tests, Nix-native build, nested Niri smoke. Доказать корректность мониторов и fractional scaling в живом окружении. Не считать зелёный unit test проверкой Wayland-графики. Переход разбит на gate-этапы, а недостающие функции перечислены в [MIGRATION_NATIVE_GTK.md](MIGRATION_NATIVE_GTK.md).

**Архитектурное правило:** системная логика принадлежит Rust Core/адаптерам; представление принадлежит GTK4; границы доверия принадлежат явно определённым API.
