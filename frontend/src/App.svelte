<script lang="ts">
  import { onMount } from 'svelte';
  import CoreStatus from './lib/components/CoreStatus.svelte';
  import WorkspacePreview from './lib/components/WorkspacePreview.svelte';
  import { formatClock } from './lib/time';
  import { openLauncher, request, focusWorkspace, onWorkspaceUpdate, onConfigUpdate, defaultUiConfig, type WorkspaceState, type ConfigState } from './lib/sdk/core';

  let now = $state(new Date());
  let coreStatus = $state<'connecting' | 'connected' | 'unavailable'>('connecting');
  let coreVersion = $state('');
  let workspaceState = $state<WorkspaceState | null>(null);
  let workspacePending = $state(false);
  let workspaceActionError = $state<string | null>(null);
  let launcherUnavailable = $state(false);

  function activateLauncher() {
    launcherUnavailable = !openLauncher();
  }

  async function activateWorkspace(id: string) {
    if (workspacePending || !workspaceState?.connected ||
        !workspaceState.workspaces.some((ws) => ws.id === id && !ws.is_focused)) return;
    workspacePending = true;
    workspaceActionError = null;
    try {
      await focusWorkspace(id);
      // Do not optimistically change focus. Niri's EventStream is authoritative.
    } catch (error) {
      workspaceActionError = error instanceof Error ? error.message : 'Workspace focus failed';
    } finally {
      workspacePending = false;
    }
  }

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
  <section class="bar__identity" aria-label="Kira Launcher">
    <button type="button" class="brand" aria-label="Open applications launcher"
      title="Open applications" onclick={activateLauncher}>
      <span class="brand__accent" aria-hidden="true"></span>
      <span class="brand__label">KIRA</span>
    </button>
  </section>

  <section class="bar__navigation" aria-label="Niri workspaces">
    <WorkspacePreview state={workspaceState} showNames={configState.config.show_names}
      limit={configState.config.max_workspaces} pending={workspacePending}
      onFocus={(id) => { void activateWorkspace(id); }} />
    <div class="bar__workspace-meta" title={configState.config.title}>
      <span class="bar__eyebrow">WORKSPACE</span>
      <span class="bar__context">
        {configState.config.title === 'KIRA DESKTOP' ? 'DESKTOP' : configState.config.title}
      </span>
    </div>
    {#if launcherUnavailable}
      <span class="launcher-warning" role="status" title="Launcher requires the native Shell">LAUNCHER OFFLINE</span>
    {/if}
    {#if workspaceActionError}
      <span class="workspace-action-error" role="alert" title={workspaceActionError}>WS !</span>
    {/if}
  </section>

  <section class="bar__system" aria-label="System status and local time">
    {#if configState.error}
      <span class="config-warning" title={configState.error} aria-label={'Configuration error: ' + configState.error}>CONFIG !</span>
    {/if}
    <div class="bar__system-item">
      <span class="bar__eyebrow" aria-hidden="true">SYSTEM</span>
      <CoreStatus status={coreStatus} version={coreVersion} />
    </div>
    <div class="bar__system-item bar__clock">
      <span class="bar__eyebrow" aria-hidden="true">LOCAL</span>
      <time datetime={now.toISOString()} aria-label={'Local time ' + formatClock(now, undefined, configState.config.show_seconds)}>
        {formatClock(now, undefined, configState.config.show_seconds)}
      </time>
    </div>
  </section>
</main>
