<script lang="ts">
  import { onMount, tick } from 'svelte';
  import SystemHealth from './lib/components/SystemHealth.svelte';
  import WorkspacePreview from './lib/components/WorkspacePreview.svelte';
  import { formatClock } from './lib/time';
  import { nextWarningId, visibleWarningIndex, type UiWarning } from './lib/warning-model';
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

  // An in-bar inspector reveals long errors without a new GTK window.
  // The native exclusive zone and normal three-zone layout remain 42px.
  let activeWarningId = $state<string | null>(null);
  let warningDetailsOpen = $state(false);
  let warningButton = $state<HTMLButtonElement | undefined>(undefined);
  let warningMessage = $state<HTMLDivElement | undefined>(undefined);
  let settingsButton = $state<HTMLButtonElement | undefined>(undefined);
  const warnings = $derived([
    launcherUnavailable ? { id: 'launcher', text: 'ЛАУНЧЕР НЕДОСТУПЕН', short: 'ЛАУНЧЕР' } : null,
    workspaceActionError ? { id: 'workspace', text: 'РАБОЧИЙ СТОЛ: ' + workspaceActionError, short: 'РАБОЧИЙ СТОЛ' } : null,
    configState.error ? { id: 'config', text: 'КОНФИГ: ' + configState.error, short: 'КОНФИГ' } : null,
    systemUnavailable ? { id: 'system', text: 'МОНИТОР НЕДОСТУПЕН', short: 'МОНИТОР' } : null,
    settingsUnavailable ? { id: 'settings', text: settingsShellOutdated ? 'ПЕРЕЗАПУСТИТЬ KIRA SHELL' : 'НАСТРОЙКИ НЕДОСТУПНЫ', short: 'НАСТРОЙКИ' } : null,
    calendarUnavailable ? { id: 'calendar', text: 'КАЛЕНДАРЬ НЕДОСТУПЕН', short: 'КАЛЕНДАРЬ' } : null,
  ].filter((value): value is UiWarning => value !== null));
  const warningPosition = $derived(visibleWarningIndex(warnings, activeWarningId));
  const shownWarning = $derived(warningPosition === -1 ? null : warnings[warningPosition]);
  const inspectingWarnings = $derived(warningDetailsOpen && shownWarning !== null);

  async function showWarningDetails() {
    if (!shownWarning) return;
    warningDetailsOpen = true;
    await tick();
    warningMessage?.focus();
  }

  function inspectWarnings() {
    activeWarningId = nextWarningId(warnings, activeWarningId);
    if (warningMessage) warningMessage.scrollTop = 0;
  }

  async function closeWarningDetails() {
    warningDetailsOpen = false;
    await tick();
    (warningButton ?? settingsButton)?.focus();
  }

  function onWarningKeydown(event: KeyboardEvent) {
    if (inspectingWarnings && event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      void closeWarningDetails();
    }
  }

  // A cleared error must not reopen the details view on the next incident.
  $effect(() => {
    if (warnings.length === 0) warningDetailsOpen = false;
  });

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

<svelte:window onkeydown={onWarningKeydown} />

<main
  class="relative flex h-[42px] min-h-[42px] w-full items-stretch overflow-hidden border-b border-kira-muted/15 bg-kira-bg text-kira-text"
  aria-label="Верхняя панель Kira Desktop"
  data-kira-ui-root
  data-compact={configState.config.compact}
>
  <!-- KIRA opens the application index; it never claims to switch windows. -->
  <section inert={inspectingWarnings}
    class={['relative z-10 flex shrink-0 items-center border-r border-kira-muted/15 bg-kira-surface',
      configState.config.compact ? 'w-[78px]' : 'w-[96px] max-[540px]:w-[76px]'].join(' ')}
    aria-label="Запуск приложений Kira"
  >
    <button type="button"
      class={['flex h-full w-full cursor-pointer items-center border-0 bg-transparent text-left text-kira-text transition-colors duration-150 hover:bg-kira-bg/15 focus-visible:-outline-offset-4 focus-visible:outline-2 focus-visible:outline-kira-accent motion-reduce:transition-none',
        configState.config.compact ? 'gap-2 px-3' : 'gap-[9px] px-[13px] max-[540px]:gap-[6px] max-[540px]:px-[9px]'].join(' ')}
      aria-label="Открыть меню приложений"
      title="Открыть приложения" onclick={activateLauncher}
    >
      <span class="h-4 w-[3px] shrink-0 bg-kira-accent" aria-hidden="true"></span>
      <span class="text-[12px] font-extrabold tracking-[0.16em] max-[540px]:text-[11px]">KIRA</span>
    </button>
  </section>

  <!-- Focus and navigation are exclusively owned by Niri's EventStream. -->
  <section
    class={['flex min-w-0 flex-1 items-center bg-kira-bg',
      configState.config.compact ? 'gap-2 px-2' : 'gap-3 px-3 max-[540px]:gap-1.5 max-[540px]:px-2'].join(' ')}
    aria-label="Рабочие пространства Niri"
    inert={inspectingWarnings}
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
  </section>

  <!-- Native measurements and local time: no fake taskbar activity. -->
  <section
    class={['flex shrink-0 items-center justify-end gap-1 border-l border-kira-muted/15 bg-kira-surface-soft px-3',
      configState.config.compact ? 'px-2' : 'max-[540px]:px-2'].join(' ')}
    aria-label="Система и местное время"
    inert={inspectingWarnings}
  >
    <SystemHealth
      coreStatus={coreStatus}
      coreVersion={coreVersion}
      niriConnected={workspaceState?.connected ?? null}
      metrics={systemMetrics}
      {metricsLastReceivedAt}
      {now}
      onOpen={activateSystemPanel}
    />
    {#if warnings.length}
      <span class="sr-only" role="status" aria-live="polite">{warnings.map(warning => warning.text).join('; ')}</span>
    {/if}
    {#if shownWarning}
      <button bind:this={warningButton} type="button" onclick={showWarningDetails}
        aria-label={'Открыть полный текст предупреждения ' + (warningPosition + 1) + ' из ' + warnings.length + ': ' + shownWarning.text}
        aria-haspopup="dialog" aria-expanded={inspectingWarnings}
        title={shownWarning.text + '\nНажмите для просмотра полного текста'}
        class="min-w-0 max-w-[105px] shrink-0 cursor-pointer truncate px-1 text-center text-[11px] font-extrabold text-kira-accent transition-colors hover:text-kira-text focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-kira-accent motion-reduce:transition-none min-[900px]:max-w-[180px]">
        <span class="min-[900px]:hidden">{shownWarning.short}</span>
        <span class="hidden min-[900px]:inline">{shownWarning.text}</span>
        {#if warnings.length > 1}
          <span class="ml-1 whitespace-nowrap" aria-hidden="true">{warningPosition + 1}/{warnings.length}</span>
        {/if}
      </button>
    {/if}
    <button bind:this={settingsButton} type="button" onclick={activateSettings}
      aria-label="Открыть настройки системы" title="Настройки системы" aria-haspopup="dialog"
      class="flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-[2px] text-kira-text transition-colors duration-150 hover:bg-kira-surface hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent motion-reduce:transition-none">
      <svg aria-hidden="true" viewBox="0 0 24 24" width="17" height="17" fill="none"
        stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 3.5v2m0 13v2M3.5 12h2m13 0h2M6 6l1.5 1.5m9 9L18 18M18 6l-1.5 1.5m-9 9L6 18"/>
        <circle cx="12" cy="12" r="5.5"/><circle cx="12" cy="12" r="2"/>
      </svg>
    </button>
    <button
      type="button"
      class="shrink-0 cursor-pointer rounded-[2px] border-0 bg-transparent px-2 py-2 leading-none text-kira-text transition-colors duration-150 hover:bg-kira-surface hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent motion-reduce:transition-none"
      title="Открыть календарь"
      aria-label={'Открыть календарь, местное время ' + formatClock(now, undefined, configState.config.show_seconds)}
      aria-haspopup="dialog"
      onclick={activateCalendar}
    >
      <time class="whitespace-nowrap text-[13px] font-extrabold tracking-[0.05em] tabular-nums max-[540px]:text-xs"
        datetime={now.toISOString()}>
        {formatClock(now, undefined, configState.config.show_seconds)}
      </time>
    </button>
  </section>
  {#if inspectingWarnings && shownWarning}
    <!-- Temporary inspection mode; the underlying Top Bar zones are inert. -->
    <section role="dialog" aria-modal="true" aria-label="Подробности предупреждения"
      class="absolute inset-0 z-30 flex h-[42px] items-center gap-2 border-b border-kira-muted/15 bg-kira-bg px-2 text-kira-text">
      <span class="shrink-0 text-[11px] font-bold tabular-nums text-kira-accent" aria-hidden="true">
        !{warningPosition + 1}/{warnings.length}
      </span>
      <div bind:this={warningMessage} tabindex="0" role="document"
        aria-label={'Полный текст предупреждения: ' + shownWarning.text}
        class="min-w-0 h-[34px] flex-1 overflow-y-auto break-words py-0.5 text-[12px] font-semibold leading-[15px] text-kira-text focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-kira-accent">
        {shownWarning.text}
      </div>
      {#if warnings.length > 1}
        <button type="button" onclick={inspectWarnings} aria-label="Следующее предупреждение" title="Следующее предупреждение"
          class="flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-[2px] border border-kira-muted/20 text-[18px] font-semibold text-kira-accent hover:bg-kira-surface focus-visible:outline-2 focus-visible:outline-kira-accent">
          <span aria-hidden="true">›</span>
        </button>
      {/if}
      <button type="button" onclick={() => { void closeWarningDetails(); }} aria-label="Закрыть подробности" title="Закрыть подробности"
        class="flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-[2px] border border-kira-muted/20 text-[18px] font-semibold text-kira-text hover:bg-kira-surface focus-visible:outline-2 focus-visible:outline-kira-accent">
        <span aria-hidden="true">×</span>
      </button>
    </section>
  {/if}
</main>
