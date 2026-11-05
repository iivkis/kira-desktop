<script lang="ts">
  import { onMount } from 'svelte';
  import { splitTray, type TrayItem } from '../tray-model';
  let { items, onToggle, onActivate, onSecondary }: {
    items: TrayItem[];
    onToggle: () => void;
    onActivate: (id: number) => void;
    onSecondary: (id: number) => void;
  } = $props();
  let maxVisible = $state(4);
  onMount(() => {
    const resize = () => {
      maxVisible = window.innerWidth < 700 ? 2 : window.innerWidth < 1100 ? 4 : 6;
    };
    resize();
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  });
  const groups = $derived(splitTray(items));
  const visiblePinned = $derived(groups.pinned.slice(0, maxVisible));
  const overflowCount = $derived(groups.hidden.length + groups.pinned.length - visiblePinned.length);
  // A pinned icon outside the width cap still belongs to the overflow.
  const overflow = $derived([...groups.hidden, ...groups.pinned.slice(maxVisible)]);
  const needsAttention = $derived(overflow.some(item => item.status === 'NeedsAttention'));
</script>

<div class="flex shrink-0 items-center gap-0.5" role="group" aria-label="Системный трей">
  {#each visiblePinned as item (item.id)}
    <button type="button" title={item.title + ' · Правый клик: меню приложения'}
      aria-label={'Открыть ' + item.title}
      onclick={() => onActivate(item.id)}
      oncontextmenu={(event) => { event.preventDefault(); onSecondary(item.id); }}
      class="flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-[2px] text-kira-muted hover:bg-kira-bg/20 hover:text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent">
      {#if item.icon}
        <img src={item.icon} alt="" class="size-5 object-contain" draggable="false" />
      {:else}
        <span aria-hidden="true" class="text-[11px] font-bold uppercase">{item.title.slice(0, 1)}</span>
      {/if}
    </button>
  {/each}
  <button type="button" onclick={onToggle}
    aria-label={'Открыть скрытые значки трея, ' + overflowCount}
    title="Скрытые значки"
    aria-haspopup="dialog"
    class="relative flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-[2px] text-kira-muted hover:bg-kira-bg/20 hover:text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent">
    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor"
      stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="m6 9 6 6 6-6"/>
    </svg>
    {#if needsAttention}
      <span class="absolute bottom-1 right-1 size-1 bg-kira-accent" aria-hidden="true"></span>
    {/if}
  </button>
</div>
