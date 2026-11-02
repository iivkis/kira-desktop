<script lang="ts">
  import { onMount } from 'svelte';
  import {
    launchQuickApp, onQuickLaunchApps, onQuickLaunchError,
    onQuickLaunchIcons, requestQuickLaunchCatalog, requestQuickLaunchIcons,
    type LauncherApp,
  } from '../sdk/core';

  let apps = $state<LauncherApp[]>([]);
  let icons = $state<Record<number, string>>({});
  let error = $state('');
  const requested = new Set<number>();

  onMount(() => {
    // The native worker can finish before the bar mounts. The catalog
    // request replays the last validated snapshot, with no frontend polling.
    const stopApps = onQuickLaunchApps(entries => {
      apps = entries;
      icons = {};
      requested.clear();
      error = '';
    });
    const stopIcons = onQuickLaunchIcons(batch => {
      for (const icon of batch.icons) {
        if (icon.data_url !== null) icons[icon.id] = icon.data_url;
      }
    });
    const stopError = onQuickLaunchError(message => { error = message; });
    requestQuickLaunchCatalog();
    return () => {
      stopApps();
      stopIcons();
      stopError();
      requested.clear();
    };
  });

  $effect(() => {
    const missing = apps.map(app => app.id).filter(id => !requested.has(id));
    if (missing.length && requestQuickLaunchIcons(missing)) {
      for (const id of missing) requested.add(id);
    }
  });

  function launch(id: number) {
    error = '';
    if (!launchQuickApp(id)) error = 'Application service unavailable';
  }
</script>

{#if apps.length > 0}
  <nav aria-label="Quick launch" class="hidden shrink-0 items-center gap-0.5
    border-l border-kira-muted/15 px-2 pb-[2px] min-[740px]:flex"
    data-quick-launch>
    {#each apps as app (app.id)}
      <button type="button" onclick={() => launch(app.id)}
        aria-label={'Launch ' + app.name} title={'Launch ' + app.name}
        class="flex size-8 shrink-0 items-center justify-center rounded-sm
          text-kira-muted transition-colors duration-150
          hover:bg-kira-surface hover:text-kira-text
          focus-visible:bg-kira-surface focus-visible:outline-2
          focus-visible:outline-kira-accent motion-reduce:transition-none">
        {#if icons[app.id]}
          <img class="size-[22px] object-contain" src={icons[app.id]}
            alt="" aria-hidden="true" loading="lazy"
            onerror={() => { delete icons[app.id]; }} />
        {:else}
          <svg class="size-5" viewBox="0 0 24 24" fill="none"
            stroke="currentColor" stroke-width="1.5"
            stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <rect x="3" y="3" width="7" height="7" rx="1" />
            <rect x="14" y="3" width="7" height="7" rx="1" />
            <rect x="3" y="14" width="7" height="7" rx="1" />
            <rect x="14" y="14" width="7" height="7" rx="1" />
          </svg>
        {/if}
      </button>
    {/each}
    {#if error}
      <span role="status" class="max-w-20 truncate text-[9px] font-bold text-kira-accent"
        title={error}>LAUNCH !</span>
    {/if}
  </nav>
{/if}
