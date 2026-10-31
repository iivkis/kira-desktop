<script lang="ts">
  import { onMount } from 'svelte';
  import CoreStatus from './lib/components/CoreStatus.svelte';
  import WorkspacePreview from './lib/components/WorkspacePreview.svelte';
  import { formatClock } from './lib/time';
  import { request, onWorkspaceUpdate, onConfigUpdate, defaultUiConfig, type WorkspaceState, type ConfigState } from './lib/sdk/core';

  let now = $state(new Date());
  let coreStatus = $state<'connecting' | 'connected' | 'unavailable'>('connecting');
  let coreVersion = $state('');
  let workspaceState = $state<WorkspaceState | null>(null);
  let configState = $state<ConfigState>({ config: defaultUiConfig, error: null });

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

    // Core listens to Niri EventStream. Shell forwards each changed snapshot
    // as kira:workspaces, so focus updates arrive without polling.
    let pushReceived = false;
    const unsubscribe = onWorkspaceUpdate((state) => {
      if (disposed) return;
      pushReceived = true;
      workspaceState = state;
    });

    // Initial request covers the small gap between WebKit page load and
    // event listener registration. An older RPC reply must never overwrite
    // a newer pushed event.
    void request('get_workspaces').then((state) => {
      if (!disposed && !pushReceived) workspaceState = state;
    }).catch(() => {
      if (!disposed && !pushReceived) {
        workspaceState = { connected: false, workspaces: [] };
      }
    });

    // Validate/parse only in Core. Shell forwards the last-good config as
    // immediate push events. A one-shot request covers the load race.
    let pushedConfig = false;
    const stopConfig = onConfigUpdate((state) => {
      if (disposed) return;
      pushedConfig = true;
      configState = state;
    });
    void request('get_config').then((state) => {
      if (!disposed && !pushedConfig) configState = state;
    }).catch(() => {
      // Preserve the embedded defaults if Core is not available.
    });

    return () => {
      disposed = true;
      window.clearInterval(clock);
      window.clearInterval(statusTimer);
      unsubscribe();
      stopConfig();
    };
  });
</script>

<main class="bar" class:compact={configState.config.compact} aria-label="Kira Desktop top bar">
  <section class="bar__start" aria-label="Kira and workspaces">
    <div class="brand" aria-label="Kira Desktop">
      <span class="brand__symbol" aria-hidden="true">K</span>
      <span class="brand__wordmark" aria-hidden="true">KIRA</span>
    </div>
    <span class="bar__separator" aria-hidden="true"></span>
    <WorkspacePreview state={workspaceState} showNames={configState.config.show_names} limit={configState.config.max_workspaces} />
  </section>

  <div class="bar__center" aria-label="Kira Desktop environment">
    <span class="bar__center-line" aria-hidden="true"></span>
    <span class="bar__center-title">
      {#if configState.config.title === 'KIRA DESKTOP'}KIRA <span>DESKTOP</span>{:else}{configState.config.title}{/if}
    </span>
    <span class="bar__center-line" aria-hidden="true"></span>
  </div>

  <section class="bar__end" aria-label="Core and clock">
    {#if configState.error}
      <span class="config-warning" title={configState.error} aria-label={'Configuration error: ' + configState.error}>CONFIG !</span>
    {/if}
    <CoreStatus status={coreStatus} version={coreVersion} />
    <span class="bar__separator" aria-hidden="true"></span>
    <div class="clock">
      <svg class="clock__icon" aria-hidden="true" viewBox="0 0 16 16" fill="none">
        <circle cx="8" cy="8" r="5.6" stroke="currentColor" stroke-width="1.3"></circle>
        <path d="M8 4.6V8.1L10.2 9.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"></path>
      </svg>
      <time datetime={now.toISOString()} aria-label={'Local time ' + formatClock(now, undefined, configState.config.show_seconds)}>
        {formatClock(now, undefined, configState.config.show_seconds)}
      </time>
    </div>
  </section>
</main>
