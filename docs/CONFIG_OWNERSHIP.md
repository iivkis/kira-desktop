# Kira configuration ownership and transactions

**Status:** normative design contract. The native-gtk branch now wires the recovered Niri display adapter to reusable `kira-ui::DisplaySettings`; nested Niri graphical acceptance and ownership-conflict testing are still required before production use.

## Goal

A declarative base and an interactive Settings GUI must not race to own the
same settings. Read, change, confirm, and persist are **different operations**.
A GUI must never silently edit Nix store paths, Home Manager symlinks,
unrecognized KDL, or a user-maintained configuration block.

## Layers and authority

| Layer | Authority | Durability |
| --- | --- | --- |
| Base | NixOS, Home Manager, user KDL, system defaults | User-defined, typically declarative |
| Managed | Explicitly opted-in, generated Kira KDL | Persisted and version-checked |
| Session | Niri IPC action, after explicit Apply | Until Niri reload/restart |
| Observed | Compositor-provided live output state | Truth for UI; not necessarily desired state |

1. The **observed** layer is presented immediately. No stale GUI cache can
   overwrite it. The editor builds a draft from live output data, never from
   guessed hardware capabilities.
2. **Apply** updates the session through typed native commands. Displays require
   15-second confirmation; the worker performs best-effort rollback on timeout
   **while the Shell process is alive**. Hiding Settings requests an early revert.
   Terminating the Shell process also terminates this watchdog, so the current
   native-gtk implementation disables output mutations outside the isolated
   preview. Production use requires a Core-owned or external crash-resilient
   rollback supervisor; a shell worker is not such a supervisor. It does *not* change persistent files.
3. **Confirm** authorizes the session state, not persistence.
4. **Save to config** is a separate user action, available only for the last
   Niri-confirmed, still-current display configuration. Backend re-reads output
   state before writing. This action is never run automatically.
5. First save opts in a writable, regular user Niri config by adding exactly
   one include of Kira's fragment, and then writing that fragment. It never
   edits unrelated sections. Nix-managed or symlinked main configs require an
   explicit include declaration from their owner.
6. **A setting has one writable owner.** Niri's multipart \`output\` sections
   are not mergeable field-by-field. If the source (including other includes)
   declares any \`output\` blocks, automatic handover fails closed. The user
   must move/remove those blocks manually or manage displays entirely in the
   original source. Kira doesn't opportunistically create duplicate outputs.
7. Kira's managed file is fully generated and owns only the selected output
   sections. A sidecar receipt stores its exact last-written bytes. If somebody
   edits the fragment or receipt outside Kira, future GUI saves reject it as a
   version conflict. Manual edits to unrelated root sections are left alone.
8. Only plain, bounded and parseable KDL inputs are accepted for automated
   reconciliation. Recursive includes are scanned up to eight levels with
   cycle detection; ambiguous syntax or conflicting output definitions stop
   the operation instead of making a risky guess.
9. Output modes, connectors, VRR capability, coordinates, transform, and
   state are validated on the native side. The renderer cannot provide a
   filesystem path or an executable command.
10. Files are written with temporary-file + atomic rename, preserved
    permissions, and a Niri configuration validation step; validation failure
    restores previous bytes. No shell is involved. Crash between fragment and
    receipt update is detected as a conflict next time.
11. Externally authored or generated configuration takes precedence over
    Kira if an ownership conflict is detected. The UI must show the conflict,
    not claim settings were permanently saved.
12. Preview mode uses exactly the same component as detached Niri window;
    native state is generation-scoped to the active Settings GTK controller.

## File contract

Defaults when no NIRI_CONFIG override is present:

- User entry point: \`$XDG_CONFIG_HOME/niri/config.kdl\`.
- Owned content: \`$XDG_CONFIG_HOME/niri/kira-managed-outputs.kdl\`.
- Conflict receipt: \`$XDG_CONFIG_HOME/niri/kira-managed-outputs.receipt\`.

The managed file is **not** the user's main configuration. Its contents are
ordinary, inspectable Niri KDL; no hidden database is required to run Niri.
The receipt is a write-conflict guard, not part of Niri's parser.

For an editable main config, the first successful GUI Save appends:

\`\`\`kdl
// KIRA MANAGED OUTPUTS (explicit opt-in)
include "kira-managed-outputs.kdl"
\`\`\`

Niri watches included files and live-reloads changes. This requires Niri
version supporting includes (25.11+).

### Declarative Nix / Home Manager

Do not turn a Nix-generated \`config.kdl\` into an imperative file. Instead,
declare an include for the *user-writable* managed fragment in the Niri config
**after** base settings, and move any \`output\` sections owned by the GUI out
of the base definition. Example KDL fragment added through Nix configuration:

\`\`\`kdl
include "/home/USER/.config/niri/kira-managed-outputs.kdl"
\`\`\`

Replace USER with your actual home directory. Prefer a Nix expression
interpolating \`config.home.homeDirectory\` rather than hardcoding it. Ensure
the included file exists *before* Niri parses the include; with Niri 26.04+
you may use \`include optional=true "/home/USER/..."\` and let Kira create
the fragment on first save. No startup mutation of a store path is attempted.

If NIRI_CONFIG points to an alternate absolute path, Kira respects that entry
point but keeps the fragment under the user's XDG Niri config directory and
uses an absolute include path. Niri launched with an unrelated \`--config\`
which is not reflected in NIRI_CONFIG cannot be discovered reliably: use a
declarative manual include and verify it targets the active file.

## Conflict and recovery table

| Situation | Required response |
| --- | --- |
| All monitors disabled by draft | Reject before touching Niri |
| Unsupported mode/scale/rotation/VRR | Reject via native validation |
| Confirm window expires | Replay saved runtime baseline |
| Niri output changed after confirm | Refuse persistence, ask for refresh |
| Existing output stanza in base or other includes | Refuse to create duplicate; manual ownership handover |
| Main config is Nix symlink and include absent | Show explicit include instruction |
| Generated fragment was manually edited | Reject overwrite; manual reconciliation |
| KDL parse/validation fails | Restore previous KDL, report validation error |
| File replaced concurrently | Recheck ownership/receipt before write; conflicts require resolution |
| Niri IPC unavailable | No fake success, no config save without a confirmed plan |

## Not yet automatic

This first implementation covers Niri outputs only. Future adapters for
appearance, keybinds and Kira's own KDL must implement the **same ownership
interface and transaction contract**, not inherit write access to arbitrary
KDL. A universal KDL AST-aware reconciler and GUI-assisted migration of
existing output blocks remain separate features.

### Verification

Before accepting the native display editor, run \`task check\`, \`task shell:check\`,
and a nested Niri session to test: writable user KDL, symlinked Nix config with
explicit include, external edits, missing fragment, invalid KDL,
partial commit/rollback, a disconnected monitor, and a 15-second timeout.
Do not claim that CI tests demonstrate real physical hotplug recovery.

See [ACCEPTANCE_NATIVE_G2.md](ACCEPTANCE_NATIVE_G2.md) for the mandatory nested Niri safety matrix.
