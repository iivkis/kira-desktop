<script lang="ts">
  import { onMount } from 'svelte';
  import CoreStatus from './lib/components/CoreStatus.svelte';
  import WorkspacePreview from './lib/components/WorkspacePreview.svelte';
  import { formatClock } from './lib/time';
  import { request, type WorkspaceState } from './lib/sdk/core';

  let now = $state(new Date());
  let coreStatus = $state<'connecting' | 'connected' | 'unavailable'>('connecting');
  let coreVersion = $state('');
  let workspaceState = $state<WorkspaceState | null>(null);

  onMount(() => {
    const clock = window.setInterval(() => {
      now = new Date();
    }, 1_000);

    let disposed = false;
    const refresh = async () => {
      try {
        const state = await request('get_state');
        if (!disposed) {
          coreVersion = state.version;
          coreStatus = 'connected';
        }
      } catch {
        if (!disposed) coreStatus = 'unavailable';
      }
    };
    void refresh();
    const statusTimer = window.setInterval(() => void refresh(), 5_000);

    // Core is subscribed to Niri's event stream. The Shell exposes read-only
    // snapshots; this lightweight UI poll will be replaced by typed pushes.
    let workspaceRequestPending = false;
    const refreshWorkspaces = async () => {
      if (workspaceRequestPending) return;
      workspaceRequestPending = true;
      try {
        const state = await request('get_workspaces');
        if (!disposed) workspaceState = state;
      } catch {
        if (!disposed) workspaceState = { connected: false, workspaces: [] };
      } finally {
        workspaceRequestPending = false;
      }
    };
    void refreshWorkspaces();
    const workspaceTimer = window.setInterval(() => void refreshWorkspaces(), 1_500);

    return () => {
      disposed = true;
      window.clearInterval(clock);
      window.clearInterval(statusTimer);
      window.clearInterval(workspaceTimer);
    };
  });
</script>

<main class="bar" aria-label="Kira Desktop top bar">
  <section class="bar__start" aria-label="Kira and workspaces">
    <div class="brand" aria-label="Kira Desktop">
      <span class="brand__symbol" aria-hidden="true">K</span>
      <span class="brand__wordmark" aria-hidden="true">KIRA</span>
    </div>
    <span class="bar__separator" aria-hidden="true"></span>
    <WorkspacePreview state={workspaceState} />
  </section>

  <div class="bar__center" aria-label="Kira Desktop environment">
    <span class="bar__center-line" aria-hidden="true"></span>
    <span class="bar__center-title">KIRA <span>DESKTOP</span></span>
    <span class="bar__center-line" aria-hidden="true"></span>
  </div>

  <section class="bar__end" aria-label="Core and clock">
    <CoreStatus status={coreStatus} version={coreVersion} />
    <span class="bar__separator" aria-hidden="true"></span>
    <div class="clock">
      <svg class="clock__icon" aria-hidden="true" viewBox="0 0 16 16" fill="none">
        <circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3"></circle>
        <path d="M8 4.6V8.1L10.2 9.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"></path>
      </svg>
      <time datetime={now.toISOString()} aria-label={'Local time ' + formatClock(now)}>
        {formatClock(now)}
      </time>
    </div>
  </section>
</main>
