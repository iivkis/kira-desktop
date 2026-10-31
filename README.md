# Kira Desktop

**Status:** v0.1 Draft. Архитектура и требования зафиксированы; реализация Core/Shell ещё не начата.

Kira Desktop — модульное декларативное окружение рабочего стола для **NixOS**, Linux/Wayland и композитора **Niri**. Rust отвечает за системные возможности, Svelte 5 — за интерфейс, KDL — за конфигурацию.

Полная техническая спецификация: [docs/SPECIFICATION.md](docs/SPECIFICATION.md).

## Инструменты проекта

- **NixOS-first:** `flake.nix`, `nix/package.nix`, воспроизводимое окружение `nix develop`.
- **Команды:** `Taskfile.yml` (`task check`, `task fmt`, `task lint`, `task test`, `task nix:check`, `task nix:build`).
- **CI:** GitHub Actions с минимальными проверками документации, Rust и frontend по мере их появления.

Сейчас репозиторий содержит первоначальную спецификацию и инфраструктурные заготовки. `nix build` и `task build` станут доступны после добавления Rust Workspace, frontend и lock-файлов; это не готовая реализация рабочего стола.
