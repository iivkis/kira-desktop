<script lang="ts">
  import { onMount } from 'svelte';
  import SystemHealth from './lib/components/SystemHealth.svelte';
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

<main
  class="relative flex h-[42px] min-h-[42px] w-full items-stretch overflow-hidden bg-kira-bg text-kira-text after:pointer-events-none after:absolute after:inset-x-0 after:bottom-0 after:h-[2px] after:bg-kira-surface"
  aria-label="Kira Desktop top bar"
  data-kira-ui-root
  data-compact={configState.config.compact}
>
  <section
    class={`relative z-10 flex shrink-0 items-stretch bg-kira-surface after:pointer-events-none after:absolute after:inset-x-0 after:bottom-0 after:h-[2px] after:bg-kira-accent ${configState.config.compact ? 'w-[84px]' : 'w-[102px] max-[540px]:w-[78px]'}`}
    aria-label="Kira Launcher"
  >
    <button
      type="button"
      class={`flex h-full w-full cursor-pointer items-center border-0 bg-transparent text-left text-kira-text transition-colors duration-150 hover:bg-kira-accent/10 active:bg-kira-bg/20 focus-visible:-outline-offset-4 focus-visible:outline-2 focus-visible:outline-kira-accent motion-reduce:transition-none ${configState.config.compact ? 'gap-[7px] px-[10px]' : 'gap-[10px] px-[14px] max-[540px]:gap-[6px] max-[540px]:px-[9px]'}`}
      aria-label="Open applications launcher"
      title="Open applications"
      onclick={activateLauncher}
    >
      <span class="h-[19px] w-[3px] shrink-0 bg-kira-accent" aria-hidden="true"></span>
      <span class="text-[13px] font-extrabold tracking-[0.11em] max-[540px]:text-xs">KIRA</span>
    </button>
  </section>

  <section
    class={`flex min-w-0 flex-1 items-center pb-[2px] ${configState.config.compact ? 'gap-2 px-2' : 'gap-[13px] px-4 max-[540px]:gap-[7px] max-[540px]:px-[7px]'}`}
    aria-label="Niri workspaces"
  >
    <WorkspacePreview
      state={workspaceState}
      showNames={configState.config.show_names}
      limit={configState.config.max_workspaces}
      pending={workspacePending}
      onFocus={(id) => { void activateWorkspace(id); }}
    />
    {#if !configState.config.compact}
      <div class="flex min-w-0 max-w-[176px] flex-col justify-center gap-[2px] overflow-hidden max-[820px]:hidden" title={configState.config.title}>
        <span class="whitespace-nowrap text-[8px] leading-none font-semibold tracking-[0.11em] text-kira-muted/80">WORKSPACE</span>
        <span class="max-w-40 truncate text-[10px] leading-tight font-bold tracking-[0.06em] text-kira-text">
          {configState.config.title === 'KIRA DESKTOP' ? 'DESKTOP' : configState.config.title}
        </span>
      </div>
    {/if}
    {#if launcherUnavailable}
      <span class="shrink-0 cursor-help self-center whitespace-nowrap text-[10px] font-bold text-kira-accent" role="status" title="Launcher requires the native Shell">LAUNCHER OFFLINE</span>
    {/if}
    {#if workspaceActionError}
      <span class="shrink-0 cursor-help self-center whitespace-nowrap text-[10px] font-bold text-kira-accent" role="alert" title={workspaceActionError}>WS !</span>
    {/if}
  </section>

  <section
    class={`flex shrink-0 items-center justify-end gap-3 bg-kira-surface-soft px-3 pb-[2px] ${configState.config.compact ? 'gap-2 px-2' : 'max-[540px]:gap-2 max-[540px]:px-2'}`}
    aria-label="System status and local time"
  >
    {#if configState.error}
      <span class="shrink-0 cursor-help whitespace-nowrap text-[10px] font-bold text-kira-accent" title={configState.error} aria-label={'Configuration error: ' + configState.error}>CONFIG !</span>
    {/if}
    <SystemHealth
      coreStatus={coreStatus}
      coreVersion={coreVersion}
      niriConnected={workspaceState?.connected ?? null}
    />
    <span class="h-3 w-px shrink-0 bg-kira-muted/25" aria-hidden="true"></span>
    <time
      class="shrink-0 whitespace-nowrap text-[13px] leading-none font-bold tracking-[0.03em] text-kira-text tabular-nums max-[540px]:text-xs"
      datetime={now.toISOString()}
      title={new Intl.DateTimeFormat(undefined, { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' }).format(now)}
      aria-label={'Local time ' + formatClock(now, undefined, configState.config.show_seconds)}
    >
      {formatClock(now, undefined, configState.config.show_seconds)}
    </time>
  </section>
</main>
