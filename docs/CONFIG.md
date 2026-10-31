# Kira Desktop: KDL configuration (first functional profile)

Kira Core owns configuration parsing and validation. The GUI only receives
**validated, read-only state**. The first profile intentionally accepts a
limited set of KDL nodes/properties; **it is not yet a full KDL 2.0 parser**.
Unsupported syntax, nodes or properties fail validation rather than silently
changing the desktop.

## User configuration

Copy the sample to the normal XDG location:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/kira-desktop"
cp config/kira.kdl.example "${XDG_CONFIG_HOME:-$HOME/.config}/kira-desktop/config.kdl"
```

Default lookup is `$XDG_CONFIG_HOME/kira-desktop/config.kdl`, or
`$HOME/.config/kira-desktop/config.kdl` when XDG_CONFIG_HOME is unset.
An absolute `KIRA_CONFIG_FILE` environment variable takes priority.
Relative paths are rejected. Core **never creates or overwrites** the
user's config file.

An absent file on initial startup uses Kira's built-in defaults. Editing
or atomically replacing an existing file triggers a **hot reload without
restarting Core or Shell**. Current implementation checks the small file
every 250 ms and notifies Shell only when the validated state changes.

## Supported KDL profile

```kdl
kira {
    bar compact=#false title="KIRA DESKTOP"
    clock seconds=#false
    workspaces show-names=#true limit=12
}
```

| Node | Property | Type / range | Effect |
| --- | --- | --- | --- |
| `bar` | `compact` | `#true` / `#false` | Reduce horizontal padding while keeping Niri's 42px reserved bar |
| `bar` | `title` | quoted text, 1–32 printable characters | Center title; text is escaped by Svelte |
| `clock` | `seconds` | `#true` / `#false` | Show seconds in the Top Bar clock |
| `workspaces` | `show-names` | `#true` / `#false` | Show workspace names or only numeric indices |
| `workspaces` | `limit` | integer 1–32 | Maximum number of displayed workspaces; keep the focused one visible |

Formatting rules for this **minimal KDL profile**:
- Exactly one `kira {` opening line and a `}` closing line.
- Each child node on **one line**, properties written as `key=value`
  without whitespace around `=`. Child nodes can appear in any order.
- Values are KDL 2.0 booleans (`#true` / `#false`), unsigned integers, or single-line double-quoted strings with printable Unicode characters.
  Only escaped double quotes and escaped backslashes are supported in this
  initial title profile; all other escapes and multiline/raw strings are rejected.
- Blank lines and `//` comments outside quoted strings are allowed.
- Children, lists, arbitrary attributes, slashdash, multiline/raw strings,
  annotations, node arguments and additional KDL constructs are **not yet
  supported**. Unknown or duplicate keys/nodes are errors.

This limited profile is intentional for v0.1: configuration cannot execute
commands, spawn processes, change Niri config or load arbitrary extensions.

## Validate before applying

```sh
nix develop --command cargo run --locked -p kira-core -- \
  --check-config "$(pwd)/config/kira.kdl.example"
```

For a quick check of the committed example, `nix develop --command task config:check`
runs that same CLI. `task check` also validates the example in CI.

A valid file prints the normalized JSON UI settings and exits successfully.
An invalid file exits nonzero with a diagnostic. This **does not bind**
the Core socket or start Niri. For a custom file, pass its absolute path.

### Error handling

Core processes the entire file before publishing it. On an invalid edit
(including unsupported nodes, missing closing brace, excessive size or
unreadable file), it keeps **the last valid settings** and publishes an error
in `ConfigState.error`. The Top Bar shows `CONFIG !` with a tooltip;
it remains usable and its other widgets continue working. A subsequent
valid edit clears the warning and applies the new settings.

The maximum config size is **64 KiB** with at most **128 lines**. These
bounds apply to reads in the hot-reload loop as well as CLI validation.
On deleting a file after it was loaded, the last working settings remain
active with a diagnostic. On first startup with no file, defaults apply.

## Try it in the safe nested preview

```sh
nix develop --command task dev:preview
```

The preview writes a **separate temporary Kira config**, copied from the
sample. The terminal prints its `[preview] Live Kira KDL config:` path.
While the preview is open, edit **that exact path** using a text editor,
for example change `clock seconds=#false` to `clock seconds=#true` or
`bar compact=#false` to `bar compact=#true`. Changes appear in the nested
Top Bar without restarting. The temporary file disappears when the preview
closes, and the host user's `~/.config/kira-desktop/` is untouched.

To test a persistent custom file in nested preview instead, set
`KIRA_CONFIG_FILE=/absolute/path/config.kdl` before invoking the task.
The preview reads it but never modifies it.

## Internal data flow

`config.kdl` → Core bounded parser + last-good watcher →
`get_config` / internal `subscribe_config` → Shell worker thread →
WebKit `kira:config` CustomEvent → typed Svelte SDK → live components.

The `subscribe_config` method is private to the Core Unix socket and is
**not** exposed via the WebKit JSON-RPC allowlist. Only `get_config`
is available to the UI, along with the existing read-only methods.
Changing config cannot invoke Niri `Action`. Existing workspaces remain
connected even if the config becomes invalid.

## Scope beyond this first increment

Full KDL grammar, schema migrations, themes, layout/monitor geometry,
widget ordering, per-output panels and secure user-provided modules are
separate future milestones. Do not treat this profile as general KDL
evaluation or an extension/command system.
