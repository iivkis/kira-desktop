<script lang="ts">
  import { onMount } from 'svelte';
  import { formatClock } from './lib/time';

  let now = $state(new Date());

  onMount(() => {
    const timer = window.setInterval(() => {
      now = new Date();
    }, 1_000);

    return () => window.clearInterval(timer);
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

  <section class="end" aria-label="System status">
    <span class="status" title="Not connected to system services yet">Preview</span>
    <time>{formatClock(now)}</time>
  </section>
</main>
