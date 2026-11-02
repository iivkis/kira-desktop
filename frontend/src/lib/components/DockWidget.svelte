<script lang="ts">
  import { onMount } from 'svelte';
  import {
    launchDockApp, onDockCatalog, onDockError, onDockIcons,
    openLauncherFromDock, requestDockCatalog, requestDockIcons,
    type LauncherApp,
  } from '../sdk/core';
  import { dockApplications } from '../dock-model';

  let apps = $state<LauncherApp[]>([]);
  let favorites = $state<string[]>([]);
  let icons = $state<Record<number, string>>({});
  let loaded = $state(false);
  let error = $state('');
  const requested = new Set<number>();
  const visible = $derived(dockApplications(apps, favorites));

  onMount(() => {
    const stopCatalog = onDockCatalog(data => {
      apps = data.apps;
      favorites = data.favorites;
      icons = {};
      requested.clear();
      loaded = true;
      error = '';
    });
    const stopIcons = onDockIcons(batch => {
      for (const icon of batch.icons) {
        if (icon.data_url) icons[icon.id] = icon.data_url;
      }
    });
    const stopError = onDockError(message => { error = message; });
    if (!requestDockCatalog()) error = 'Dock service unavailable';
    return () => {
      stopCatalog();
      stopIcons();
      stopError();
      requested.clear();
    };
  });

  $effect(() => {
    if (!loaded) return;
    const needed = visible.map(app => app.id).filter(id => !requested.has(id));
    if (needed.length && requestDockIcons(needed)) {
      for (const id of needed) requested.add(id);
    }
  });

  function launch(id: number) {
    error = '';
    if (!launchDockApp(id)) error = 'Dock service unavailable';
  }
</script>

<main data-kira-ui-root aria-label="Kira Dock"
  class="flex h-[104px] min-h-[104px] w-full items-center justify-center overflow-hidden bg-transparent text-kira-text">
  <!-- One floating platform rather than a full-width toolbar. No permanent
       orange "active" bar: running-window state is not implemented yet. -->
  <nav data-dock-platform aria-label="Dock applications"
    class="kira-dock-platform flex h-[80px] max-w-[calc(100%-16px)] items-center justify-center
      gap-2 px-3.5">
    <button type="button" onclick={openLauncherFromDock}
      data-dock-launcher
      aria-label="Open application launcher" title="Open application launcher"
      class="group relative flex h-14 w-[76px] shrink-0 items-center justify-center gap-2
        rounded-md bg-kira-bg/65 text-kira-text
        transition-[background-color,transform] duration-150
        hover:bg-kira-bg focus-visible:outline-2 focus-visible:outline-kira-accent
        active:translate-y-px">
      <span aria-hidden="true" class="h-6 w-[3px] shrink-0 bg-kira-accent"></span>
      <span class="text-[12px] font-extrabold tracking-[0.15em]">KIRA</span>
    </button>

    {#if visible.length > 0}
      <span aria-hidden="true" class="mx-0.5 h-8 w-px shrink-0 bg-kira-muted/20"></span>
    {/if}

    {#each visible as app (app.id)}
      <button type="button" data-dock-app
        onclick={() => launch(app.id)}
        title={app.name} aria-label={'Launch ' + app.name}
        class="group relative flex size-14 shrink-0 items-center justify-center rounded-md
          bg-transparent transition-[background-color,transform] duration-150
          hover:bg-kira-bg/55 focus-visible:bg-kira-bg/55
          focus-visible:outline-2 focus-visible:outline-kira-accent
          active:translate-y-px">
        <span class="flex size-11 items-center justify-center" aria-hidden="true">
          {#if icons[app.id]}
            <img class="size-8 object-contain" src={icons[app.id]} alt=""
              loading="lazy" onerror={() => { delete icons[app.id]; }} />
          {:else}
            <!-- A single neutral placeholder; don't invent an app's identity
                 from its first character while the trusted icon is loading. -->
            <svg data-dock-icon-placeholder class="size-7 text-kira-muted/75"
              viewBox="0 0 32 32" fill="none" stroke="currentColor"
              stroke-width="1.6" stroke-linejoin="round">
              <rect x="5" y="5" width="9" height="9" rx="1.5" />
              <rect x="18" y="5" width="9" height="9" rx="1.5" />
              <rect x="5" y="18" width="9" height="9" rx="1.5" />
              <rect x="18" y="18" width="9" height="9" rx="1.5" />
            </svg>
          {/if}
        </span>
        <!-- A transient interaction cue, never a fabricated running indicator. -->
        <span aria-hidden="true"
          class="absolute bottom-1 h-[2px] w-3 bg-transparent
            group-hover:bg-kira-accent group-focus-visible:bg-kira-accent"></span>
      </button>
    {/each}

    {#if !loaded && !error}
      <span class="px-2 text-[10px] font-semibold tracking-wider text-kira-muted"
        role="status">INDEXING</span>
    {:else if error}
      <span class="max-w-36 truncate px-2 text-[10px] text-kira-muted"
        role="status" title={error}>UNAVAILABLE</span>
    {/if}
  </nav>
</main>
