<script lang="ts">
  // Visual-only until Kira Core receives real Niri workspace state.
  const mockWorkspaces = [1, 2, 3] as const;
</script>

<ol class="workspace-preview" aria-label="Workspace layout preview, not connected to Niri">
  {#each mockWorkspaces as number}
    <li
      class="workspace-preview__item"
      class:preview-selected={number === 1}
      title={'Workspace ' + number + ' (layout preview only)'}
    >
      <span>{number}</span>
      {#if number === 1}
        <span class="workspace-preview__selected-mark" aria-hidden="true"></span>
      {/if}
    </li>
  {/each}
</ol>

<style>
  .workspace-preview {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .workspace-preview__item {
    position: relative;
    display: grid;
    place-items: center;
    width: 27px;
    height: 27px;
    border-radius: var(--kira-radius-sm);
    color: var(--kira-color-text-secondary);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    letter-spacing: 0.02em;
    cursor: default;
    transition:
      background-color var(--kira-motion-fast) var(--kira-motion-easing),
      color var(--kira-motion-fast) var(--kira-motion-easing);
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
</style>
