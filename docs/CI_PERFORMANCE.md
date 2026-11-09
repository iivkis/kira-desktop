# CI optimization: baseline, safeguards, and evidence

## Baseline on November 9, 2026

- Native GTK job on GitHub Actions run 1: **7m36s**.
  Its Nix package build took **4m11s**; separate Cargo check, Clippy,
  and tests took **3m10s**.
- Graphical job on run 2: another **5m12s**,
  including a **second 4m41s Nix build** of the same package.
- Both jobs together consumed **12m48s** of runner execution on
  qualifying pushes (plus the separate fast Core/IPC job).
- Graphical evidence was **104 PNGs** per run: 42 entry + 42 exit +
  20 live SNI, automatically uploaded (~928 KB compressed), even
  without a parity review.

## Biggest, low-risk reductions implemented

1. **No duplicate Nix packaging.** The ordinary native job already
   builds 3. All existing fake-Niri
   display rollback, SNI and Notifications smokes still run, followed
   by the **real Weston → nested Niri pixel check on that same runner**.
   4 is manually dispatched only.
2. **Docs-only bypass.** 5 compares
   the complete push or PR base against its head, never just HEAD^.
   If history cannot be fetched or parsed, the build is **required**.
   Modifications to crates, scripts, Cargo, Nix, config and CI controls
   cause a full native check. The fast Core/IPC plus Python/parity
   tests always run.
3. **No redundant plain Cargo check.** Core/IPC Clippy and tests remain
   in the fast job. Native GTK/services/shell Clippy 6
   already performs typechecking and is followed by all native tests.
4. **Bounded screenshot density and zero automatic uploads.**
   Normal CI captures **14 entry + 14 exit + 6 tray = 34**
   genuine PNGs, **67.3% fewer** than 104. The observation window
   remains ~650 ms, then the evidence is deleted with the runner.
   No replacement mock frames are generated. The visual check still
   requires real bar paint, entry and exit pixels, and D-Bus events.
5. **Manual full review is preserved.** 7
   supports profile 8 (34 PNGs) and 9
   (104 PNGs). Uploaded evidence requires explicit
   10 and is kept for **2 days**.
   Nightly duplicate 11 is removed;
   that workflow is still available on manual dispatch.
6. **Repository hygiene.** 12 ignores
   the entire local generated 13 folder,
   not just one subfolder.

## How to measure the real outcome

Compare actual step timestamps on a completed new source-changing
14 run with the baseline above. The deleted
independent graphical build removes one 4m41s packaging invocation and
its runner; the in-job visual smoke adds a shorter real compositor
step. Cargo check savings must be read from CI durations, not assumed.

For documentation-only pushes, expect native-shell to be skipped,
while Core/IPC and parity checks still succeed. Unknown diff history
must **never** skip native validation. For human motion/pixel QA
dispatch the manual workflow with an explicit upload request.
All 15 source/main widget verified flags remain false.
