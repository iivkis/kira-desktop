<script lang="ts">
  import { onMount } from 'svelte';
  import SystemHealth from './lib/components/SystemHealth.svelte';
  import WorkspacePreview from './lib/components/WorkspacePreview.svelte';
  import { formatClock } from './lib/time';
  import { openLauncher, toggleCalendar, toggleSystemPanel, toggleSettings, request, focusWorkspace, onWorkspaceUpdate, onConfigUpdate, onSystemMetrics, defaultUiConfig, type WorkspaceState, type ConfigState, type SystemMetrics } from './lib/sdk/core';

  let now = $state(new Date());
  let coreStatus = $state<'connecting' | 'connected' | 'unavailable'>('connecting');
  let coreVersion = $state('');
  let workspaceState = $state<WorkspaceState | null>(null);
  let workspacePending = $state(false);
  let workspaceActionError = $state<string | null>(null);
  let launcherUnavailable = $state(false);
  let calendarUnavailable = $state(false);
  let systemUnavailable = $state(false);
  let settingsUnavailable = $state(false);
  let settingsShellOutdated = $state(false);
  let systemMetrics = $state<SystemMetrics | null>(null);
  let metricsLastReceivedAt = $state<number | null>(null);

  function activateSettings() {
    settingsUnavailable = !toggleSettings();
    // A packaged frontend can be newer than the already-running GTK
    // application. Older shells know Calendar but have no Settings handler.
    // A second kira-shell invocation only activates that old primary process;
    // the user must restart the actual Shell, not merely relaunch its binary.
    settingsShellOutdated = settingsUnavailable &&
      Boolean(window.webkit?.messageHandlers?.kiraCalendar) &&
      !window.webkit?.messageHandlers?.kiraSettings;
  }

  function activateSystemPanel() {
    systemUnavailable = !toggleSystemPanel();
  }

  function activateCalendar() {
    calendarUnavailable = !toggleCalendar();
  }

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

    const stopMetrics = onSystemMetrics((value) => {
      if (!disposed) {
        systemMetrics = value;
        metricsLastReceivedAt = Date.now();
      }
    });

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
      stopMetrics();
    };
  });
</script>

<main
  class="relative flex h-[42px] min-h-[42px] w-full items-stretch overflow-hidden border-b border-kira-muted/15 bg-kira-bg text-kira-text"
  aria-label="Kira Desktop top bar"
  data-kira-ui-root
  data-compact={configState.config.compact}
>
  <!-- KIRA opens the application index; it never claims to switch windows. -->
  <section
    class={['relative z-10 flex shrink-0 items-center border-r border-kira-muted/15 bg-kira-surface',
      configState.config.compact ? 'w-[78px]' : 'w-[96px] max-[540px]:w-[76px]'].join(' ')}
    aria-label="Kira Launcher"
  >
    <button type="button"
      class={['flex h-full w-full cursor-pointer items-center border-0 bg-transparent text-left text-kira-text transition-colors duration-150 hover:bg-kira-bg/15 focus-visible:-outline-offset-4 focus-visible:outline-2 focus-visible:outline-kira-accent motion-reduce:transition-none',
        configState.config.compact ? 'gap-2 px-3' : 'gap-[9px] px-[13px] max-[540px]:gap-[6px] max-[540px]:px-[9px]'].join(' ')}
      aria-label="Open applications launcher"
      title="Open applications" onclick={activateLauncher}
    >
      <span class="h-4 w-[3px] shrink-0 bg-kira-accent" aria-hidden="true"></span>
      <span class="text-[12px] font-extrabold tracking-[0.16em] max-[540px]:text-[11px]">KIRA</span>
    </button>
  </section>

  <!-- Focus and navigation are exclusively owned by Niri's EventStream. -->
  <section
    class={['flex min-w-0 flex-1 items-center bg-kira-bg',
      configState.config.compact ? 'gap-2 px-2' : 'gap-3 px-3 max-[540px]:gap-1.5 max-[540px]:px-2'].join(' ')}
    aria-label="Niri workspaces"
  >
    <WorkspacePreview
      state={workspaceState}
      showNames={configState.config.show_names}
      limit={configState.config.max_workspaces}
      pending={workspacePending}
      onFocus={(id) => { void activateWorkspace(id); }}
    />
    {#if !configState.config.compact && configState.config.title !== 'KIRA DESKTOP'}
      <span class="max-w-36 min-w-0 truncate border-l border-kira-muted/20 pl-3 text-[11px] font-semibold tracking-wide text-kira-muted max-[900px]:hidden"
        title={configState.config.title}>{configState.config.title}</span>
    {/if}
    {#if launcherUnavailable}
      <span class="shrink-0 cursor-help whitespace-nowrap text-[11px] font-bold text-kira-accent"
        role="status" title="Launcher requires the native Shell">LAUNCHER OFFLINE</span>
    {/if}
    {#if workspaceActionError}
      <span class="shrink-0 cursor-help whitespace-nowrap text-[11px] font-bold text-kira-accent"
        role="alert" title={workspaceActionError}>WS !</span>
    {/if}
  </section>

  <!-- Native measurements and local time: no fake taskbar activity. -->
  <section
    class={['flex shrink-0 items-center justify-end gap-1 border-l border-kira-muted/15 bg-kira-surface-soft px-3',
      configState.config.compact ? 'px-2' : 'max-[540px]:px-2'].join(' ')}
    aria-label="System status and local time"
  >
    {#if configState.error}
      <span class="shrink-0 cursor-help whitespace-nowrap px-1 text-[11px] font-bold text-kira-accent"
        title={configState.error}
        aria-label={'Configuration error: ' + configState.error}>CONFIG !</span>
    {/if}
    <SystemHealth
      coreStatus={coreStatus}
      coreVersion={coreVersion}
      niriConnected={workspaceState?.connected ?? null}
      metrics={systemMetrics}
      {metricsLastReceivedAt}
      {now}
      onOpen={activateSystemPanel}
    />
    {#if systemUnavailable}
      <span role="status" class="shrink-0 px-1 text-[11px] font-bold text-kira-accent">MONITOR OFFLINE</span>
    {/if}
    <button type="button" onclick={activateSettings}
      aria-label="Open system settings" title="System settings" aria-haspopup="dialog"
      class="flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-[2px] text-kira-text transition-colors duration-150 hover:bg-kira-surface hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent motion-reduce:transition-none">
      <svg aria-hidden="true" viewBox="0 0 24 24" width="17" height="17" fill="none"
        stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 3.5v2m0 13v2M3.5 12h2m13 0h2M6 6l1.5 1.5m9 9L18 18M18 6l-1.5 1.5m-9 9L6 18"/>
        <circle cx="12" cy="12" r="5.5"/><circle cx="12" cy="12" r="2"/>
      </svg>
    </button>
    {#if settingsUnavailable}
      <span role="status" class="shrink-0 text-[11px] font-bold text-kira-accent"
        title={settingsShellOutdated
          ? 'Frontend новее запущенного Kira Shell. Обновите и полностью перезапустите процесс kira-shell.'
          : 'Нативный канал настроек недоступен.'}>
        {settingsShellOutdated ? 'RESTART KIRA SHELL' : 'SETTINGS OFFLINE'}
      </span>
    {/if}
    {#if calendarUnavailable}
      <span class="shrink-0 px-1 text-[11px] font-bold text-kira-accent"
        role="status">CALENDAR OFFLINE</span>
    {/if}
    <button
      type="button"
      class="shrink-0 cursor-pointer rounded-[2px] border-0 bg-transparent px-2 py-2 leading-none text-kira-text transition-colors duration-150 hover:bg-kira-surface hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent motion-reduce:transition-none"
      title="Open calendar and date details"
      aria-label={'Open calendar, local time ' + formatClock(now, undefined, configState.config.show_seconds)}
      aria-haspopup="dialog"
      onclick={activateCalendar}
    >
      <time class="whitespace-nowrap text-[13px] font-extrabold tracking-[0.05em] tabular-nums max-[540px]:text-xs"
        datetime={now.toISOString()}>
        {formatClock(now, undefined, configState.config.show_seconds)}
      </time>
    </button>
  </section>
</main>
