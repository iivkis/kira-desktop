# Kira Desktop V2 contributor and agent rules

**Active architecture for branch kira-desktop-v2:** [docs/V2_ARCHITECTURE.md](docs/V2_ARCHITECTURE.md). Lua subset: [docs/V2_LANGUAGE.md](docs/V2_LANGUAGE.md). Review: [docs/V2_REVIEW.md](docs/V2_REVIEW.md). Decision: [ADR-0001](docs/adr/0001-lua-rust-gtk.md). Migration: [docs/V2_MIGRATION.md](docs/V2_MIGRATION.md).

## This branch's authoritative direction

1. **Lua is the authoring language for widgets, styles, state and UI logic.** Produce a native GTK4 interface, not Svelte/HTML.
2. **A compiler/transpiler written in Rust converts a validated Lua subset into Rust gtk4-rs code and GTK CSS/resources at build time.** Do not implement V2 by embedding a Lua VM to create widgets at runtime.
3. Keep Linux, Niri, IPC, D-Bus, OS side effects, security and integration in trusted Rust. Generated UI must call only validated capabilities, never execute arbitrary OS commands.
4. Keep NixOS-first reproducible builds, cargo lockfiles, deterministic generated output and typed error diagnostics.
5. The current Svelte/WebKit sources, npm build and GSAP code were inherited from V1. They are **legacy migration baseline only**. Do not add new V2 features in them or assume green V1 CI validates V2.
6. Before the compiler exists, Lua examples and documents are specifications, **not runnable code**. Mark implementation status honestly.
7. Do not remove V1 modules until replacement parity and native GTK/Niri tests have passed. Keep the default main branch unchanged.

## Compiler implementation contract

- Parse Lua source without executing it; implement only the statically analyzable syntax described in docs/V2_LANGUAGE.md. Syntactically valid Lua is not automatically supported.
- Parse → AST → validated typed IR → Rust/GTK codegen. Never paste unchecked Lua strings into generated Rust or CSS.
- Reject unsupported Lua features rather than quietly falling back to evaluation/interpreter mode.
- Emit stable source diagnostics, limit memory/depth/input size and keep build results deterministic.
- Cover parse, type validation, Rust emission, diagnostics, negative/security cases, Nix build and actual native GTK smoke.
- Generated Rust is build output, not hand-maintained source; write to ignored build directories only.
- GTK widgets and reactive state must be mutated in GTK main context; updates from OS workers use typed messages.

## UI and design invariants

- Retain Kira Default's Black/Prussian Blue/Orange/Alabaster Grey/White identity and truthfully reported system states.
- GTK4 is now the actual widget renderer, not merely a WebKit host. Styling is GTK CSS generated from Lua styles.
- Preserve accessibility, keyboard, focus, reduced motion and display safety rollback.
- TENET principles remain a design goal; GSAP-specific mandatory rules of V1 **do not apply** to V2. A native Rust/GTK implementation and acceptance contract must be designed.
- No speculative claim that compilation makes UI faster/lighter until measured.

## Working with the legacy implementation

Documents explicitly labeled **V1 migration reference** describe inherited Svelte/WebKit implementation and may conflict with the new V2 decision. For **new V2 work**, this file and docs/V2_ARCHITECTURE.md take precedence. Existing tests and code are retained until migration milestones are complete.

Do not treat the initial architecture commit as completed compiler or widget migration.
