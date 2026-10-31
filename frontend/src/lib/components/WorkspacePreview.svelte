<script lang="ts">
  import type { WorkspaceState } from '../sdk/core';

  let { state, showNames, limit, onFocus, pending }: {
    state: WorkspaceState | null;
    showNames: boolean;
    limit: number;
    onFocus: (id: string) => void;
    pending: boolean;
  } = $props();

  // Limit visible chips without hiding the active/focused workspace when
  // there are many outputs. This never changes Niri's actual workspaces.
  let visible = $derived.by(() => {
    const all = state?.workspaces ?? [];
    const count = Math.max(1, Math.min(32, limit));
    const focused = Math.max(0, all.findIndex((item) => item.is_focused));
    return all.slice(Math.max(0, focused - count + 1), Math.max(0, focused - count + 1) + count);
  });
</script>

{#if !state}
  <span class="workspace-status" role="status" aria-live="polite">NIRI…</span>
{:else if !state.connected}
  <span class="workspace-status" role="status" aria-live="polite" title="Niri IPC disconnected">NIRI OFFLINE</span>
{:else if state.workspaces.length === 0}
  <span class="workspace-status" role="status" aria-live="polite">NO WORKSPACES</span>
{:else}
  <ol class="workspace-preview" aria-label="Niri workspaces">
    {#each visible as workspace (workspace.id)}
      <li
        class="workspace-preview__item"
        class:preview-selected={workspace.is_focused}
        class:preview-active={workspace.is_active}
        class:preview-urgent={workspace.is_urgent}
      >
        <button
          type="button"
          class="workspace-preview__button"
          aria-label={'Focus workspace ' + (workspace.name || workspace.idx)
            + (workspace.output ? ' on ' + workspace.output : '')}
          aria-pressed={workspace.is_focused}
          title={'Workspace ' + (workspace.name || workspace.idx)
            + (workspace.output ? ' on ' + workspace.output : '')
            + (workspace.is_focused ? ' (focused)' : workspace.is_active ? ' (active)' : '')
            + (workspace.is_urgent ? ' (urgent)' : '')}
          disabled={pending}
          onclick={() => onFocus(workspace.id)}
        >
          <span class="workspace-preview__label">{showNames ? (workspace.name || workspace.idx) : workspace.idx}</span>
          {#if workspace.is_urgent}
            <span class="workspace-preview__urgent-mark" aria-hidden="true"></span>
          {/if}
        </button>
      </li>
    {/each}
  </ol>
{/if}

<style>
  .workspace-status {
    display: flex;
    align-items: center;
    height: 28px;
    color: var(--kira-color-muted);
    font-size: 10px;
    font-weight: 650;
    white-space: nowrap;
  }

  .workspace-preview {
    display: flex;
    align-items: center;
    flex: none;
    gap: 4px;
    max-width: min(40vw, 348px);
    margin: 0;
    padding: 0;
    overflow-x: auto;
    scrollbar-width: none;
    list-style: none;
  }

  .workspace-preview::-webkit-scrollbar { display: none; }

  .workspace-preview__item {
    position: relative;
    flex: none;
    min-width: 32px;
    max-width: 90px;
    height: 28px;
    border-radius: var(--kira-radius-sm);
    color: var(--kira-color-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    font-weight: 700;
    transition:
      color var(--kira-motion-fast) var(--kira-motion-easing),
      background-color var(--kira-motion-fast) var(--kira-motion-easing);
  }

  .workspace-preview__button {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    padding: 0 9px;
    border: 0;
    border-radius: inherit;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .workspace-preview__button:disabled { cursor: progress; }
  .workspace-preview__button:focus-visible {
    outline: 2px solid var(--kira-color-accent);
    outline-offset: 2px;
  }

  .workspace-preview__item:not(.preview-selected):has(.workspace-preview__button:not(:disabled):hover) {
    background: color-mix(in srgb, var(--kira-color-surface) 65%, var(--kira-color-background));
    color: var(--kira-color-text);
  }

  .workspace-preview__label {
    display: block;
    max-width: 72px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .workspace-preview__item.preview-active:not(.preview-selected) {
    background: color-mix(in srgb, var(--kira-color-surface) 70%, var(--kira-color-background));
    color: var(--kira-color-text);
  }

  /* One strong state. No decorative outline, nested frame or offset shadow. */
  .workspace-preview__item.preview-selected {
    background: var(--kira-color-accent);
    color: var(--kira-color-on-accent);
  }

  .workspace-preview__item.preview-selected .workspace-preview__button:focus-visible {
    outline-color: var(--kira-color-text);
  }

  .workspace-preview__urgent-mark {
    position: absolute;
    bottom: 2px;
    left: calc(50% - 4px);
    width: 8px;
    height: 2px;
    background: var(--kira-color-accent);
    pointer-events: none;
  }

  .workspace-preview__item.preview-selected .workspace-preview__urgent-mark {
    background: var(--kira-color-on-accent);
  }
</style>
