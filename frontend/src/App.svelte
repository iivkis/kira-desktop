<script lang="ts">
  import { onMount } from 'svelte';
  import { formatClock } from './lib/time';
  import { request } from './lib/sdk/core';

  let now = $state(new Date());
  let coreStatus = $state<'connecting' | 'connected' | 'unavailable'>('connecting');
  let coreVersion = $state('');

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

    return () => {
      disposed = true;
      window.clearInterval(clock);
      window.clearInterval(statusTimer);
    };
  });
</script>

<main class="bar" aria-label="Kira Desktop top bar">
  <section class="start" aria-label="Workspaces (preview)">
    <span class="brand" title="Kira Desktop">K</span>
    <div class="workspaces">
      <span class="workspace selected">1</span>
      <span class="workspace">2</span>
      <span class="workspace">3</span>
    </div>
  </section>

  <section class="center" aria-label="Desktop identity">Kira Desktop</section>

  <section class="end" aria-label="Core and system status">
    <span class:online={coreStatus === 'connected'} class="status" title={coreStatus === 'connected' ? `Kira Core v${coreVersion}` : 'Kira Core is unavailable'}>
      Core: {coreStatus}
    </span>
    <time>{formatClock(now)}</time>
  </section>
</main>
