# Kira Desktop V2

**Ветка:** `kira-desktop-v2` · **Статус:** архитектура принята, Lua → Rust/GTK транслятор ещё не реализован.

Kira Desktop V2 — нативная оболочка для **NixOS / Niri / Wayland**. Виджеты, стили, состояние и UI-логику автор пишет на **Lua**. Транслятор на **Rust** разбирает ограниченный Lua 5.4 DSL и генерирует **Rust/gtk4-rs + GTK CSS напрямую без внешних CSS-препроцессоров**. Обычный компилятор Rust собирает нативный интерфейс без Lua VM и WebKit.

```text
Lua widget / style / state / event handlers
                     |
                     v
Rust translator: parser → typed IR → codegen
                     |
                     v
Generated Rust GTK4 widgets + GTK CSS
                     |
                     v
Native GTK4 Shell → Rust Core → Niri / Linux
```

**Это целевая архитектура, пока не рабочая сборка.** Существующие `crates/kira-shell`, `frontend/`, npm, WebKitGTK и GSAP унаследованы от V1 и временно сохраняются как работающий миграционный базис. Их наличие не означает отказ от решения V2. Удалять их следует после проверенной нативной замены.

## Основные документы

- [Архитектура V2](docs/V2_ARCHITECTURE.md)
- [Lua Language v0.2](docs/V2_LANGUAGE.md) — удобные `ui.*` виджеты и типизированное состояние.
- [Lua Style DSL](docs/V2_STYLES.md) — темы, токены, композиция, GTK-состояния без препроцессоров.
- [ADR-0001: Lua → Rust → GTK](docs/adr/0001-lua-rust-gtk.md)
- [План миграции](docs/V2_MIGRATION.md)
- [Ревью и очистка](docs/V2_REVIEW.md)
- [Lua-пример counter](examples/v2/counter.lua) и [его Lua-тема](examples/v2/styles.lua) — **ещё не исполняются**.
- [Правила работы с веткой V2](AGENTS.md)

## Что есть в репозитории сейчас

| Часть | Текущая реализация | Цель V2 |
| --- | --- | --- |
| `kira-core`, `kira-ipc` | Rust и типизированный IPC | Сохранить |
| `kira-shell` | GTK4 хост с WebKitGTK | Нативные GTK4-виджеты |
| `frontend/` | Svelte 5 / TypeScript / Vite | Заменить Lua UI-исходниками |
| `kira-lua-compiler` | **Отсутствует** | Добавить Rust-компилятор |
| `ui/` | **Отсутствует** | Lua-компоненты и стили |
| Nix/Cargo/CI | Пока собирают и проверяют V1 | Мигрировать поэтапно |

## Проверки унаследованного V1

```sh
nix develop --command task check
nix build
nix develop --command task dev:preview
```

Эти команды проверяют **только V1**, не компилятор нового UI. Подробные инструкции предыдущей реализации: [development](docs/DEVELOPMENT.md), [V1 specification](docs/SPECIFICATION.md), [preview](docs/PREVIEW.md).

**Первая реальная цель V2:** `examples/v2/counter.lua` + `examples/v2/styles.lua` → AST → typed IR → сгенерированные Rust и GTK CSS → работающее нативное GTK4 окно с реакцией на нажатие. До этого функциональная готовность V2 не заявляется.

Ветка `main` не затрагивается.
