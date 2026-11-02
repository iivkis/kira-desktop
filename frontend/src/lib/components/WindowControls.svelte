<script lang="ts">
  import { onMount } from 'svelte';

  let { onClose, onPopout, generation, label }: {
    onClose: () => void;
    onPopout: () => void;
    generation: number;
    label: string;
  } = $props();

  let detached = $state(false);
  onMount(() => {
    const onDetached = (event: Event) => {
      const current = (event as CustomEvent<{ generation: number }>).detail?.generation;
      if (current === generation) detached = true;
    };
    window.addEventListener('kira:overlay:detached', onDetached);
    return () => window.removeEventListener('kira:overlay:detached', onDetached);
  });
</script>

<div class="flex shrink-0 items-center gap-1" role="group" aria-label={label + ' window actions'}>
  {#if !detached}
    <button type="button" onclick={onPopout}
      title={'Open ' + label + ' as a Niri window'}
      aria-label={'Open ' + label + ' as a separate window'}
      class="rounded-sm p-2 text-kira-muted transition-colors hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">
      <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24"
        fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="8" width="13" height="13" rx="1"></rect>
        <path d="M12 3h9v9M21 3l-11 11"></path>
      </svg>
    </button>
  {/if}
  <button type="button" onclick={onClose} aria-label={'Close ' + label} title="Close (Esc)"
    class="rounded-sm px-2 py-1 text-xl leading-none text-kira-muted transition-colors
      hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">×</button>
</div>
