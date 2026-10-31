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
  <span class="core-status__name">CORE</span>
  <span class="core-status__value">{labels[status]}</span>
</div>

<style>
  .core-status {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    max-width: 156px;
    padding: 0 10px;
    border: var(--kira-border-strong) solid var(--kira-color-border);
    border-radius: var(--kira-radius-md);
    background: var(--kira-color-surface-subtle);
    color: var(--kira-color-text);
    box-shadow: var(--kira-shadow-brutal);
    white-space: nowrap;
  }

  .core-status__signal {
    flex: none;
    width: 7px;
    height: 7px;
    border: 1px solid var(--kira-color-muted);
    background: transparent;
  }

  .core-status[data-status='connected'] .core-status__signal {
    border-color: var(--kira-color-accent);
    background: var(--kira-color-accent);
  }

  .core-status[data-status='connecting'] .core-status__signal {
    border-color: var(--kira-color-muted);
    background: var(--kira-color-muted);
  }

  .core-status__name {
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .core-status__value {
    overflow: hidden;
    font-size: 10px;
    font-weight: 650;
    text-overflow: ellipsis;
  }

  @media (max-width: 490px) {
    .core-status { padding-inline: 7px; }
    .core-status__name { display: none; }
  }
</style>
