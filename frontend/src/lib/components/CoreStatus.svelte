<script lang="ts">
  type Status = 'connecting' | 'connected' | 'unavailable';

  let {
    status,
    version,
  }: {
    status: Status;
    version: string;
  } = $props();

  const labels: Record<Status, string> = {
    connecting: 'Connecting',
    connected: 'Online',
    unavailable: 'Offline',
  };
</script>

<div
  class="inline-flex min-h-7 shrink-0 items-center gap-[7px] whitespace-nowrap text-kira-muted"
  data-status={status}
  role="status"
  aria-live="polite"
  aria-atomic="true"
  aria-label={'Kira Core: ' + labels[status]}
  title={status === 'connected' ? 'Kira Core v' + version : 'Kira Core: ' + labels[status]}
>
  <span
    class={`h-[7px] w-[7px] shrink-0 border ${status === 'connected'
      ? 'rounded-full border-kira-accent bg-kira-accent'
      : status === 'unavailable'
        ? 'rounded-[2px] border-kira-accent bg-transparent'
        : 'rounded-full border-kira-muted bg-transparent'}`}
    aria-hidden="true"
  ></span>
  <span class="text-[11px] font-semibold">Core</span>
  {#if status !== 'connected'}
    <span class="text-[10px] font-semibold text-kira-text">{labels[status]}</span>
  {/if}
</div>
