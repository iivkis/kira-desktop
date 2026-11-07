# Kira Desktop V2 — архитектурное ревью и история уборки

**Ветка:** `kira-desktop-v2` · **Решение:** [ADR-0002](adr/0002-embedded-lua-mlua.md), заменяет [ADR-0001](adr/0001-lua-rust-gtk.md).
**Метод:** статический анализ дерева/манифестов/документации; новый runtime ещё не собирался.

## Новая архитектура

После исходной идеи Lua-to-Rust translator проект перешёл на **встраивание настоящего Lua 5.4 через mlua** в Rust GTK Shell. Компилятор `kira-lua-compiler` **отменён**; контракты `ui.*`, state, callbacks, themes и styles в [V2_LANGUAGE.md](V2_LANGUAGE.md) и [V2_STYLES.md](V2_STYLES.md) теперь описывают **исполняемый Lua runtime API**.

Ранее возникшие вопросы о static Lua AST/IR и генерации Rust из Lua больше не являются требованиями. Вместо них критичны GTK main-thread ownership, правильное хранение Lua callbacks, `mlua` VM lifecycle, модульный загрузчик и безопасность системных capabilities.

## Уборка репозитория V2

Уже удалены `.repospool` (~1,4 МБ), `.repospoolignore` и workflow публикации snapshots. `README.md` описывает актуальную архитектуру. Исторические документы V1 размечены как миграционные; они сохраняются для UI behavior regression и security references.

Намеренно сохранены `frontend/`, `crates/kira-shell/`, старые npm/Cargo/Nix lockfiles, Svelte/WebKit code и CI. Они нужны до появления работоспособного GTK+Lua replacement. Это **не выбранный стек V2**.

## Оставшиеся риски

- **P0:** в Cargo Workspace нет `mlua`, отсутствует `kira.ui` Rust bridge и реальный GTK-only Lua Shell.
- **P0:** callbacks и binding subscriptions должны отписываться при уничтожении view; GTK и Lua callback failures не должны приводить к panic/abort.
- **P0:** `mlua` не обеспечивает песочницу недоверенного кода; сторонние Lua-виджеты нельзя считать изолированными без дополнительного process sandbox.
- **P1:** Lua скрипты на main thread могут блокировать GTK, необходимы бюджеты исполнения и запрет синхронных OS операций в UI callback.
- **P1:** module loader должен быть явным и ограничивать видимые файлы/внешние библиотеки.
- **P1:** после подключения mlua надо проверить реальную Nix-сборку и locking системной/vendored Lua зависимости.
- **P1:** нативный TENET motion, визуальная/производительная parity и полноценный reload пока не реализованы.
- **P2:** отдельный файл LICENSE отсутствовал, несмотря на MIT в Cargo; правообладателя не следует угадывать.

## Критерий реализации

Реальный `mlua` хост исполняет [counter.lua](../examples/v2/counter.lua) с [styles.lua](../examples/v2/styles.lua), строит GTK4 Box/Label/Button, выполняет Lua callback при клике, обновляет Rust-backed state и GtkLabel, применяет стиль и безопасно освобождает подписки. Пока это не выполнено, V2 является спецификацией + V1 migration baseline.
