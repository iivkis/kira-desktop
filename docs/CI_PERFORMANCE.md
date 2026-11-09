# CI performance: two-minute fast lane and manual full verification

## Decision

Previously, the optimized source-changing run [#42589985764](https://github.com/iivkis/kira-desktop/actions/runs/42589985764) completed successfully but its GTK/Nix job took **4m32s**, comprising GTK Clippy/tests (1m48s), Nix build (2m21s), and real nested-Niri smoke (11s). A prior pipeline required 7m36s for the native job plus another 5m12s of duplicate graphical work.

Even the single Nix build can exceed two minutes. There is no honest way to promise a successful, *fresh* complete GTK/Nix build within two minutes on arbitrary ephemeral runners without an already-populated binary cache. Therefore the normal required workflow is short and the complete native acceptance is explicit and manual.

## Every push and pull request: fast signal

`.github/workflows/ci.yml` has **one job**, still named `Rust Core and IPC`, with an enforced **`timeout-minutes: 2`**. It runs:

- `cargo fmt --all -- --check` for the entire Rust workspace;
- `cargo clippy` and `cargo test` for `kira-core` and `kira-ipc` (all targets);
- Python syntax and all `scripts/test_*.py` tests, pinned parity verification and removal checks for the old WebKit frontend.

No Nix install, GTK compilation, GTK/service/shell tests, Nix package build, Wayland compositor, screenshots, upload actions, downstream jobs or change classifier run automatically.

The previous fast Core/IPC and Python lane completed in about **23 seconds** in [#42591055942](https://github.com/iivkis/kira-desktop/actions/runs/42591055942). This is a historical measurement, **not** a cold-run success guarantee. GitHub runner queue time is outside the job timeout.

## On-demand full native gate

Dispatch **Full native GTK/Nix/Wayland validation (manual)** in GitHub Actions, located at `.github/workflows/graphical.yml`, on the **exact code commit** before merging or shipping GTK work. It performs:

1. Full Rust workspace Clippy and tests (including `kira-ui`, `kira-services` and `kira-shell`) in a Nix GTK environment.
2. `nix build .#default -L` for the packaged native desktop.
3. Isolated display-supervisor rollback against fake Niri, private D-Bus SNI, and two-peer Notifications policy smoke.
4. Real Weston → nested Niri → packaged GTK visual/pixel validation, preserving the native notification transition checks.
5. The 34-frame smoke mode or opt-in 104-frame detailed motion review. Screenshot artifact upload requires separate explicit consent and has a two-day lifetime.

This full run can take several minutes. The independent old `.github/workflows/nix.yml` workflow and unused `ci_change_scope.py`/`test_ci_change_scope.py` were removed.

**Risk explicitly retained:** a passing *fast* CI run does not catch GTK compilation errors, GUI test failures or broken Nix packaging. It is **not equivalent** to the manual full gate. Run that gate before shipping native changes; if it has not run on the target commit, report verification as incomplete.

## Acceptance criteria

- Ordinary fast job must execute within its two-minute runtime cap. An exceptionally cold runner may **fail** the cap; this does not guarantee success or exclude queue delay.
- There must be no automatically started heavy Nix/Wayland jobs or uploaded screenshot archives.
- The manual full workflow must remain runnable and keep its GTK/Wayland/D-Bus safety checks.
- All 15 widget parity flags remain false until independent source-main and native visual, motion, keyboard and accessibility acceptance.

This architecture deliberately buys fast feedback by *changing the scope of automatic CI*, not by falsely claiming that Nix itself compiles in two minutes.
