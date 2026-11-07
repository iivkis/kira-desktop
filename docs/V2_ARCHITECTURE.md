# Kira Desktop V2 — Lua + mlua + Rust + GTK4

**Статус:** архитектура принята; **runtime ещё не реализован**.
**Приоритет:** [ADR-0002](adr/0002-embedded-lua-mlua.md) заменяет [ADR-0001](adr/0001-lua-rust-gtk.md).
**API:** [Lua widgets](V2_LANGUAGE.md) · [Lua styles](V2_STYLES.md) · [Миграция](V2_MIGRATION.md).

## Концепция

Kira Desktop V2 — NixOS-first desktop shell поверх Niri/Wayland, а не новый compositor. Пользовательские виджеты, темы, состояние и UI-логика пишутся на **обычном Lua 5.4**. Нативный Rust Shell **встраивает mlua** и выполняет Lua-файлы во время работы приложения.

Никакого компилятора Lua → Rust, AST/IR codegen для Lua и генерации `generated_ui.rs`. Lua не обязан быть статически ограниченным декларативным языком: поддерживаются локальные переменные, обычные функции/замыкания, условия, циклы и локальные модули в границах безопасности. При необходимости Lua может создавать новые элементы динамически через `ui` API.

```text
Lua scripts (widgets, theme, callbacks)
       |
       v
mlua Lua 5.4 VM in Rust Shell
       |
       +--> ui API (registered Rust functions / userdata)
       |      +--> WidgetSpec / StateHandle / StyleSpec
       |      +--> GTK4 native builder on GTK main thread
       |      +--> GtkCssProvider runtime style registry
       |
       +--> allowlisted high-level system capabilities
       |
       v
Rust Shell (GTK4 + gtk4-layer-shell + GIO/D-Bus)
       |
       +--> Rust Core & kira-ipc --> Niri / Linux
```

## Границы

**Lua:** UI composition, handlers, application state, conditional view logic, named styles and themes. `require("styles")` работает для разрешённых локальных модулей. `require("kira.ui")` возвращает Rust-реализованный UI API. Lua-код **действительно выполняется**, а не парсится как шаблон.

**mlua host (Rust):** owns each VM, registers module loader and `kira.ui`, exposes `ui.box/text/button/state/bind/compose/theme`, keeps Lua functions alive while widgets exist, routes GUI signals to Lua, reports errors and cleans up on unmount. On M1 `ui.box/text/button` produce validated `WidgetSpec` representations (Rust userdata or internal spec objects) and the renderer materializes native GTK widgets. This approach avoids exposing unrestricted GTK internals to Lua.

**GTK Shell (Rust):** GTK4/gtk4-layer-shell windows, native widget lifecycle, `GtkCssProvider`, focus, keyboard, accessibility, tray/notifications integration. Initial Lua VM and all GTK references are kept **on the GTK main thread**, with `mlua` built **without `send`**; no GTK userdata crosses thread boundaries. Background Core/IPC/Niri work delivers typed messages to the UI thread.

**Rust Core:** Niri integration, D-Bus/system operations, controlled capabilities, configuration safety; unchanged as architectural foundation.

**Nix/Cargo:** reproducible source and dependency resolution. Suggested future Cargo dependency for runtime crate or shell (after lockfile regeneration):

```toml
mlua = { version = "0.12", features = ["lua54", "vendored"] }
```

Use a pinned, lockfile-resolved version. `vendored` bundles the Lua C library for reproducibility; an external system Lua 5.4 may be chosen after testing the Nix build. Neither option makes Lua-generated Rust.

## Жизненный цикл виджета M1

1. GTK initializes on the application main thread.
2. Rust creates an `mlua::Lua` VM with only the required standard libraries and a controlled module loader; registers `kira.ui`.
3. Host executes the entry Lua module (e.g. `examples/v2/counter.lua`) and receives a `WidgetSpec` tree.
4. Rust builds native `gtk::Box`, `gtk::Label`, `gtk::Button`; validates input properties and styles. It retains `mlua::Function` callbacks through the VM registry/owned handles while widgets live.
5. GTK events call into Lua on the main thread. `ui.state(value):set(...)` updates the Rust-backed observable state and synchronizes all `ui.bind(state)` values. Lua is not re-evaluated in full for every click.
6. Removing a view releases GTK signal connections, callback handles, subscriptions and VM registry references. Lua errors are caught/logged as widget errors, not allowed to panic/abort the GTK signal callback.
7. Restarting/reloading Lua widgets happens after safe teardown, with rollback to the last working view where feasible; no guarantee of instantaneous code hot reload in M1.

## Стили

**Lua themes and styles only**; no Sass, SCSS, Grass, Tailwind, PostCSS or separate preprocessor. At runtime the Rust bridge resolves token/style tables and emits **GTK CSS** from a strict supported-properties allowlist, caches styles, creates CSS classes and applies them to native widgets. Only GTK CSS is generated; **no Rust source** is generated. [Подробно](V2_STYLES.md).

## Безопасность

The Lua VM is **not an OS sandbox**. M1 is for **trusted, locally installed UI scripts**. No implicit right to call arbitrary D-Bus/Niri, shell, Rust or GTK APIs. A trusted host-controlled module resolver, explicit capabilities and validated GTK property setters are required. Do not use unsafe `Lua::unsafe_new` or load arbitrary C modules for external plugins. Limit Lua VM memory and budget long-running callbacks/instruction use where supported; catch runtime errors and ensure they do not freeze the UI forever. Third-party untrusted widgets need **process isolation** and a separate threat model before being advertised as safe.

## Что удаляется, а что остаётся

- Отменяется создание `kira-lua-compiler`, статическая AST/IR/codegen архитектура и `generated_ui.rs`.
- V1 `frontend/`, WebKit and JS code temporarily remain for behavior parity and legacy CI; they are not the new UI target.
- NixOS-first, Kira Default, native GTK4 and TENET design principles remain. Native animation is a separate GTK/Rust effort, not GSAP.
- Runtime Lua scripts can be edited without **компиляции Rust**; reloading modules/UI safely may still require a view restart.

## Критерий первого M1

`counter.lua` + `styles.lua` are executed by the mlua host in a GTK4 app: button click invokes a **Lua closure**, updates a `ui.state`, updates GTK label via `ui.bind`, applies a Lua theme and `:hover` style, and unloads without leaking handles. Verify no WebKit or JS in that native UI path. Before that, these files remain API design examples, not runnable in the current repository.

No source or generated executable is a security boundary merely by using mlua.
