<script lang="ts">
  import { systemHealth, type CoreConnectionStatus } from '../system-health';

  let {
    coreStatus,
    coreVersion,
    niriConnected,
  }: {
    coreStatus: CoreConnectionStatus;
    coreVersion: string;
    niriConnected: boolean | null;
  } = $props();

  const health = $derived(systemHealth(coreStatus, coreVersion, niriConnected));
</script>

<div
  class="inline-flex h-6 shrink-0 items-center justify-center gap-[6px] whitespace-nowrap"
  role="status"
  aria-live="polite"
  aria-atomic="true"
  aria-label={health.details.replace('\n', '; ')}
  title={health.details}
  data-phase={health.phase}
>
  <span
    class={`h-[7px] w-[7px] shrink-0 border ${health.phase === 'ready'
      ? 'rounded-full border-kira-muted bg-kira-muted'
      : health.phase === 'error'
        ? 'rounded-[2px] border-kira-accent bg-kira-accent'
        : 'rounded-full border-kira-muted bg-transparent'}`}
    aria-hidden="true"
  ></span>
  <span class={`text-[10px] leading-none font-semibold tracking-[0.02em] ${health.phase === 'error'
    ? 'text-kira-accent'
    : 'text-kira-text'} ${health.phase === 'ready' ? 'max-[540px]:hidden' : ''}`}>{health.label}</span>
</div>
