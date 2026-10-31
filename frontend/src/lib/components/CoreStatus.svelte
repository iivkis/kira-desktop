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
  class="core-status"
  data-status={status}
  role="status"
  aria-live="polite"
  aria-atomic="true"
  aria-label={'Kira Core: ' + labels[status]}
  title={status === 'connected' ? 'Kira Core v' + version : 'Kira Core: ' + labels[status]}
>
  <span class="core-status__signal" aria-hidden="true"></span>
  <span class="core-status__name">Core</span>
  {#if status !== 'connected'}
    <span class="core-status__value">{labels[status]}</span>
  {/if}
</div>

<style>
  .core-status {
    display: inline-flex;
    align-items: center;
    flex: none;
    gap: 7px;
    min-height: 28px;
    color: var(--kira-color-muted);
    white-space: nowrap;
  }

  .core-status__signal {
    flex: none;
    width: 7px;
    height: 7px;
    border: 1px solid var(--kira-color-muted);
    border-radius: 50%;
    background: transparent;
  }

  .core-status[data-status='connected'] .core-status__signal {
    background: var(--kira-color-accent);
    border-color: var(--kira-color-accent);
  }

  .core-status[data-status='unavailable'] .core-status__signal {
    border-color: var(--kira-color-accent);
    border-radius: 2px;
  }

  .core-status__name {
    font-size: 11px;
    font-weight: 650;
  }

  .core-status__value {
    color: var(--kira-color-text);
    font-size: 10px;
    font-weight: 650;
  }
</style>
