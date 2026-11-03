<script lang="ts">
  import { systemHealth, type CoreConnectionStatus } from '../system-health';
  import type { SystemMetrics } from '../sdk/core';

  let {
    coreStatus,
    coreVersion,
    niriConnected,
    metrics,
    onOpen,
  }: {
    coreStatus: CoreConnectionStatus;
    coreVersion: string;
    niriConnected: boolean | null;
    metrics: SystemMetrics | null;
    onOpen: () => void;
  } = $props();

  const health = $derived(systemHealth(coreStatus, coreVersion, niriConnected, metrics));
</script>

<button
  type="button"
  class="inline-flex h-8 shrink-0 cursor-pointer items-center justify-center gap-[7px] rounded-[2px] border-0 bg-transparent px-2 text-kira-muted transition-colors duration-150 hover:bg-kira-surface focus-visible:outline-2 focus-visible:outline-kira-accent motion-reduce:transition-none"
  aria-haspopup="dialog"
  aria-label={'Open system monitor: ' + health.label}
  title={health.details + '\nClick for detailed system monitor'}
  data-phase={health.phase}
  onclick={onOpen}
>
  <span
    class={['h-[7px] w-[7px] shrink-0 border',
      health.phase === 'ready'
        ? 'rounded-full border-kira-muted bg-kira-muted'
        : health.phase === 'error' || health.phase === 'warning'
          ? 'rounded-[2px] border-kira-accent bg-kira-accent'
          : 'rounded-full border-kira-muted bg-transparent'].join(' ')}
    aria-hidden="true"
  ></span>
  <span role="status" aria-live="polite" aria-atomic="true"
    class={['text-[10px] leading-none font-bold tracking-[0.04em]',
      health.phase === 'error' || health.phase === 'warning'
        ? 'text-kira-accent' : 'text-kira-text',
      health.phase === 'ready' ? 'sr-only' : ''].join(' ')}
  >{health.label}</span>
</button>
