<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { tenetHoverLens } from '../tenet-hover-lens';
  import {
    launchDesktopApp, onLauncherCatalog, onLauncherError, onLauncherIcons,
    requestLauncherCatalog, requestLauncherIcons, type LauncherApp,
  } from '../sdk/core';
  import { filterLauncherApps, nextLauncherSelection } from '../launcher-model';

  let { onClose, generation }: { onClose: () => void; generation: number } = $props();

  let apps = $state<LauncherApp[]>([]);
  let icons = $state<Record<number, string>>({});
  // Cache requests for this one visible widget only. Native icon bytes
  // are never requested for hidden entries or retained after unmount.
  const requestedIcons = new Set<number>();
  let loaded = $state(false);
  let error = $state('');
  let query = $state('');
  let selected = $state(0);
  let search: HTMLInputElement;
  let results: HTMLDivElement;

  const matches = $derived(filterLauncherApps(apps, query));
  // Render a bounded viewport instead of hundreds of application DOM nodes.
  const displayed = $derived(matches.slice(0, 80));
  const activeIndex = $derived(Math.min(selected, displayed.length - 1));

  onMount(() => {
    const stopCatalog = onLauncherCatalog(catalog => {
      apps = catalog.apps;
      requestedIcons.clear();
      icons = {};
      loaded = true;
      error = '';
      selected = 0;
    });
    const stopError = onLauncherError(message => { error = message; });
    const stopIcons = onLauncherIcons(batch => {
      for (const icon of batch.icons) {
        if (icon.data_url) icons[icon.id] = icon.data_url;
      }
    });
    if (!requestLauncherCatalog(generation)) {
      error = 'Application service unavailable';
    }
    void tick().then(() => search?.focus());
    return () => {
      stopCatalog();
      stopError();
      stopIcons();
    };
  });

  // Only request the at-most-80 rows Svelte is actually rendering.
  // Keep batches small so icon resolution cannot flood the WebKit bridge.
  $effect(() => {
    if (!loaded) return;
    const needed = displayed.map(app => app.id).filter(id => !requestedIcons.has(id));
    for (let i = 0; i < needed.length; i += 16) {
      const batch = needed.slice(i, i + 16);
      if (requestLauncherIcons(generation, batch)) {
        for (const id of batch) requestedIcons.add(id);
      }
    }
  });

  function choose(index: number) {
    const app = displayed[index];
    if (!app) return;
    selected = index;
    error = '';
    if (!launchDesktopApp(generation, app.id)) {
      error = 'Cannot contact application service';
    }
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      selected = nextLauncherSelection(selected, displayed.length, event.key === 'ArrowDown' ? 1 : -1);
      void tick().then(() => {
        const item = results?.querySelector<HTMLElement>('[aria-selected="true"]');
        item?.scrollIntoView({ block: 'nearest' });
      });
      return;
    }
    if (event.key === 'Enter' && !event.isComposing) {
      event.preventDefault();
      choose(activeIndex);
    }
  }
</script>

<svelte:window onkeydown={keydown} />

<main data-kira-ui-root aria-label="Kira Launcher" use:tenetHoverLens
  class="flex h-auto min-h-0 w-full flex-col overflow-hidden bg-kira-bg text-kira-text">
  <header class="shrink-0 px-5 pb-4 pt-5">
    <div class="flex items-start justify-between gap-2">
      <div class="min-w-0">
        <span class="text-[10px] font-extrabold tracking-[0.16em] text-kira-accent">KIRA / APPLICATION INDEX</span>
        <h1 class="mt-1 text-[30px] font-extrabold leading-tight">Applications</h1>
        <p class="mt-1 text-xs text-kira-muted">Search your installed tools.</p>
      </div>
      <button type="button" onclick={onClose} title="Close (Esc)" aria-label="Close launcher"
        class="rounded px-2 py-1 text-xl text-kira-muted hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">×</button>
    </div>
  </header>

  <section class="flex min-h-0 flex-1 flex-col gap-3 bg-kira-surface px-4 py-4">
    <label for="kira-launcher-query" class="text-[10px] font-bold tracking-[0.1em] text-kira-muted">FIND / OPEN</label>
    <input id="kira-launcher-query" type="search" bind:this={search} bind:value={query}
      oninput={() => { selected = 0; error = ''; }}
      autocomplete="off" spellcheck="false"
      aria-label="Search applications" aria-controls="kira-launcher-results"
      placeholder="Search applications…"
      class="h-11 w-full shrink-0 rounded-md border border-kira-muted/40 bg-kira-bg px-3 text-sm text-kira-text outline-none placeholder:text-kira-muted focus-visible:border-kira-accent"/>
    <div class="flex items-center justify-between gap-2">
      <span class="text-[10px] font-bold tracking-wider text-kira-muted">APPLICATION CATALOG</span>
      <span class="text-[10px] font-bold tabular-nums text-kira-muted">{matches.length} / {apps.length} APPS</span>
    </div>
    <div id="kira-launcher-results" bind:this={results}
      class="kira-launcher-scroll min-h-0 flex-1 overflow-y-auto pr-1" role="listbox" aria-label="Matching applications">
      {#each displayed as app, index (app.id)}
        <button type="button" role="option" aria-selected={index === activeIndex}
          onclick={() => choose(index)}
          title={app.name + ' / ' + (app.description || 'Installed application')}
          class={'my-0.5 flex min-h-[54px] w-full items-center gap-3 py-2 pr-2 text-left focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-kira-accent ' +
            (index === activeIndex ? 'bg-kira-bg/80' : 'hover:bg-kira-bg/30')}>
          <span class={'h-10 w-[3px] shrink-0 ' + (index === activeIndex ? 'bg-kira-accent' : 'bg-transparent')}></span>
          <span class={'flex size-9 shrink-0 items-center justify-center overflow-hidden ' +
              (icons[app.id] ? 'bg-transparent' : 'bg-kira-surface-soft')}
            aria-hidden="true" data-kira-icon-slot>
            {#if icons[app.id]}
              <img class="size-8 object-contain" src={icons[app.id]} alt="" loading="lazy"
                onerror={() => { delete icons[app.id]; }} />
            {:else}
              <svg data-kira-icon-placeholder class="size-5 text-kira-muted" viewBox="0 0 24 24"
                fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"
                stroke-linejoin="round" aria-hidden="true">
                <rect x="3" y="3" width="7" height="7" rx="1" />
                <rect x="14" y="3" width="7" height="7" rx="1" />
                <rect x="3" y="14" width="7" height="7" rx="1" />
                <rect x="14" y="14" width="7" height="7" rx="1" />
              </svg>
            {/if}
          </span>
          <span class="min-w-0 flex-1">
            <span class={'block truncate text-sm font-semibold ' + (index === activeIndex ? 'text-kira-accent' : 'text-kira-text')}>{app.name}</span>
            <span class="line-clamp-2 text-[11px] leading-4 text-kira-muted">{app.description.trim() || 'Installed application'}</span>
          </span>
        </button>
      {:else}
        <p class="py-8 text-center text-xs text-kira-muted" role="status">
          {#if error}{error}{:else if !loaded}LOADING APPLICATIONS…{:else}No matching applications{/if}
        </p>
      {/each}
    </div>
    {#if error && displayed.length}
      <p class="shrink-0 text-xs font-medium text-kira-accent" role="alert">{error}</p>
    {/if}
    {#if matches.length > displayed.length}
      <p class="shrink-0 text-[10px] text-kira-muted">Showing first {displayed.length} matches. Refine search for more.</p>
    {/if}
  </section>
  <footer class="flex shrink-0 flex-wrap items-center gap-x-2 gap-y-1 px-5 py-3 text-[10px] text-kira-muted">
    <kbd class="rounded border border-kira-muted/40 px-1.5 py-1 text-kira-text">↑ ↓</kbd><span>Navigate</span>
    <kbd class="ml-2 rounded border border-kira-muted/40 px-1.5 py-1 text-kira-text">ENTER</kbd><span>Open</span>
    <kbd class="ml-2 rounded border border-kira-muted/40 px-1.5 py-1 text-kira-text">ESC</kbd><span>Close</span>
  </footer>
</main>
