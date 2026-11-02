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
  class="flex h-[72px] min-h-[72px] w-full items-center justify-center overflow-hidden bg-kira-bg text-kira-text">
  <nav aria-label="Dock applications"
    class="flex h-full max-w-full items-center justify-center gap-1.5 border-t-2 border-kira-accent bg-kira-surface px-3">
    <button type="button" onclick={openLauncherFromDock}
      aria-label="Open application launcher" title="Application index"
      class="flex size-11 shrink-0 items-center justify-center border border-kira-muted/25 bg-kira-bg
        text-[11px] font-extrabold tracking-widest text-kira-accent
        hover:border-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">
      KIRA
    </button>
    {#if visible.length}
      <span class="mx-1 h-8 w-px shrink-0 bg-kira-muted/25" aria-hidden="true"></span>
    {/if}
    {#each visible as app (app.id)}
      <button type="button" onclick={() => launch(app.id)}
        title={app.name} aria-label={'Launch ' + app.name}
        class="group relative flex size-11 shrink-0 items-center justify-center border border-transparent
          bg-kira-bg/60 hover:border-kira-muted/50
          focus-visible:border-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">
        {#if icons[app.id]}
          <img class="size-8 object-contain" src={icons[app.id]} alt="" aria-hidden="true"
            onerror={() => { delete icons[app.id]; }} />
        {:else}
          <span aria-hidden="true" class="text-[17px] font-extrabold text-kira-muted">
            {app.name.trim().slice(0, 1).toLocaleUpperCase()}
          </span>
        {/if}
        <span class="absolute -bottom-1 left-3 right-3 h-[2px] bg-transparent
          group-hover:bg-kira-accent group-focus-visible:bg-kira-accent" aria-hidden="true"></span>
      </button>
    {/each}
    {#if !loaded && !error}
      <span class="px-2 text-[10px] font-semibold tracking-widest text-kira-muted" role="status">INDEXING</span>
    {:else if error}
      <span class="max-w-40 truncate px-2 text-[10px] text-kira-muted" role="status"
        title={error}>UNAVAILABLE</span>
    {/if}
  </nav>
</main>
