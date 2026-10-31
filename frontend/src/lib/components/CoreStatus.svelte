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
    display: flex;
    align-items: center;
    gap: 7px;
    height: 29px;
    max-width: 156px;
    padding: 0 11px;
    border: 1px solid color-mix(in srgb, var(--kira-color-muted) 25%, transparent);
    border-radius: 999px;
    background: color-mix(in srgb, var(--kira-color-surface) 33%, var(--kira-color-background));
    color: var(--kira-color-text);
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
    border-color: var(--kira-color-accent);
    background: var(--kira-color-accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--kira-color-accent) 20%, transparent);
  }

  .core-status[data-status='connecting'] .core-status__signal {
    border-color: var(--kira-color-muted);
    background: var(--kira-color-muted);
  }

  .core-status__name {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    opacity: 0.86;
  }

  .core-status__value {
    overflow: hidden;
    font-size: 10px;
    font-weight: 600;
    text-overflow: ellipsis;
  }

  @media (max-width: 490px) {
    .core-status {
      padding-inline: 8px;
    }

    .core-status__name {
      display: none;
    }
  }
</style>
