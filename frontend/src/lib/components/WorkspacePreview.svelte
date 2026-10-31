<script lang="ts">
  import type { WorkspaceState } from '../sdk/core';

  let { state, showNames, limit }: { state: WorkspaceState | null; showNames: boolean; limit: number } = $props();

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
  <ol class="workspace-preview" aria-label="Niri workspaces (read-only)">
    {#each visible as workspace (workspace.id)}
      <li
        class="workspace-preview__item"
        class:preview-selected={workspace.is_focused}
        class:preview-active={workspace.is_active}
        class:preview-urgent={workspace.is_urgent}
        aria-label={'Workspace ' + (workspace.name || workspace.idx)
          + (workspace.output ? ' on ' + workspace.output : '')
          + (workspace.is_focused ? ', focused' : '')
          + (workspace.is_active ? ', active' : '')
          + (workspace.is_urgent ? ', urgent' : '')}
        title={'Workspace ' + (workspace.name || workspace.idx)
          + (workspace.output ? ' on ' + workspace.output : '')
          + (workspace.is_focused ? ' (focused)' : workspace.is_active ? ' (active)' : '')
          + (workspace.is_urgent ? ' (urgent)' : '')}
      >
        <span class="workspace-preview__label">{showNames ? (workspace.name || workspace.idx) : workspace.idx}</span>
        {#if workspace.is_focused}
          <span class="workspace-preview__selected-mark" aria-hidden="true"></span>
        {:else if workspace.is_urgent}
          <span class="workspace-preview__urgent-mark" aria-hidden="true"></span>
        {/if}
      </li>
    {/each}
  </ol>
{/if}

<style>
  .workspace-status {
    display: flex;
    align-items: center;
    height: 27px;
    color: var(--kira-color-muted);
    font-size: 9px;
    font-weight: 650;
    letter-spacing: 0.08em;
    white-space: nowrap;
  }

  .workspace-preview {
    display: flex;
    align-items: center;
    gap: 5px;
    max-width: min(33vw, 330px);
    margin: 0;
    padding: 0;
    overflow-x: auto;
    scrollbar-width: none;
    list-style: none;
  }

  .workspace-preview::-webkit-scrollbar { display: none; }

  .workspace-preview__item {
    position: relative;
    display: grid;
    flex: none;
    place-items: center;
    min-width: 27px;
    max-width: 76px;
    height: 27px;
    padding-inline: 5px;
    box-sizing: border-box;
    border-radius: var(--kira-radius-sm);
    color: color-mix(in srgb, var(--kira-color-text) 80%, var(--kira-color-background));
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    letter-spacing: 0.02em;
    cursor: default;
    transition:
      background-color var(--kira-motion-fast) var(--kira-motion-easing),
      color var(--kira-motion-fast) var(--kira-motion-easing);
  }

  .workspace-preview__label {
    display: block;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    max-width: 66px;
  }

  .workspace-preview__item.preview-active:not(.preview-selected) {
    background: var(--kira-color-surface-subtle);
    box-shadow: inset 0 0 0 1px var(--kira-color-border);
  }

  .workspace-preview__item.preview-selected {
    color: var(--kira-color-on-accent);
    background: var(--kira-color-accent);
    box-shadow: 0 1px 3px color-mix(in srgb, var(--kira-color-background) 35%, transparent);
  }

  .workspace-preview__selected-mark {
    position: absolute;
    bottom: 2px;
    width: 8px;
    height: 2px;
    border-radius: 2px;
    background: var(--kira-color-background);
  }

  .workspace-preview__item.preview-urgent:not(.preview-selected) {
    box-shadow: inset 0 0 0 1px var(--kira-color-accent);
  }

  .workspace-preview__urgent-mark {
    position: absolute;
    bottom: 2px;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--kira-color-accent);
  }
</style>
