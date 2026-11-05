<script lang="ts">
  import { systemHealth, type CoreConnectionStatus } from '../system-health';
  import { healthLabel } from '../ui-locale';
  import { isSampleFresh } from '../system-model';
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

  // The native sampler runs every 2 seconds. A previously received sample
  // without a valid timestamp is not eligible to become a READY state.
  const freshness = $derived(metrics !== null &&
    !isSampleFresh(metricsLastReceivedAt, now.getTime()) ? 'stale' : 'fresh');
  const health = $derived(systemHealth(coreStatus, coreVersion, niriConnected, metrics, freshness));
  const label = $derived(healthLabel(health.label));
  // A delayed telemetry tick is not a user-facing incident. Keep the
  // small neutral indicator, but do not flash a new status message.
  const quietRefresh = $derived(freshness === 'stale' &&
    coreStatus === 'connected' && niriConnected === true);
</script>

<button
  type="button"
  class="inline-flex h-8 shrink-0 cursor-pointer items-center justify-center gap-[7px] rounded-[2px] border-0 bg-transparent px-2 text-kira-muted transition-colors duration-150 hover:bg-kira-surface focus-visible:outline-2 focus-visible:outline-kira-accent motion-reduce:transition-none"
  aria-haspopup="dialog"
  aria-label={'Открыть монитор системы' + (quietRefresh ? '' : ': ' + label)}
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
  {#if !quietRefresh}
    <span role="status" aria-live="polite" aria-atomic="true"
      class={['text-[10px] leading-none font-bold tracking-[0.04em] max-[540px]:sr-only',
        health.phase === 'error' || health.phase === 'warning'
          ? 'text-kira-accent' : 'text-kira-text',
        health.phase === 'ready' ? 'sr-only' : ''].join(' ')}
    >{label}</span>
  {/if}
</button>
