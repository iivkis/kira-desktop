<script lang="ts">
  import { systemHealth, type CoreConnectionStatus } from '../system-health';
  import { healthLabel } from '../ui-locale';
  import type { SystemMetrics } from '../sdk/core';

  let {
    coreStatus,
    coreVersion,
    niriConnected,
    metrics,
    metricsLastReceivedAt,
    now,
    onOpen,
  }: {
    coreStatus: CoreConnectionStatus;
    coreVersion: string;
    niriConnected: boolean | null;
    metrics: SystemMetrics | null;
    metricsLastReceivedAt: number | null;
    now: Date;
    onOpen: () => void;
  } = $props();

  const age = $derived(metricsLastReceivedAt === null ? null : now.getTime() - metricsLastReceivedAt);
  // The native sampler runs every 2 seconds; match the monitor's 6-second expiry.
  const freshness = $derived(age !== null && (age < 0 || age > 6_000) ? 'stale' : 'fresh');
  const health = $derived(systemHealth(coreStatus, coreVersion, niriConnected, metrics, freshness));
  const label = $derived(healthLabel(health.label));
</script>

<button
  type="button"
  class="inline-flex h-8 shrink-0 cursor-pointer items-center justify-center gap-[7px] rounded-[2px] border-0 bg-transparent px-2 text-kira-muted transition-colors duration-150 hover:bg-kira-surface focus-visible:outline-2 focus-visible:outline-kira-accent motion-reduce:transition-none"
  aria-haspopup="dialog"
  aria-label={'Открыть монитор системы: ' + label}
  title={health.details + '\nНажмите для подробных сведений'}
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
    class={['text-[10px] leading-none font-bold tracking-[0.04em] max-[540px]:sr-only',
      health.phase === 'error' || health.phase === 'warning'
        ? 'text-kira-accent' : 'text-kira-text',
      health.phase === 'ready' ? 'sr-only' : ''].join(' ')}
  >{label}</span>
</button>
