<script lang="ts">
  import { onMount, tick } from 'svelte';
  import NotificationCard from './NotificationCard.svelte';
  import type { Notice } from '../notification-model';

  type Snapshot = { revision: number; height: number; cardHeight: number; visible: Notice[] };
  let visible = $state<Notice[]>([]);
  let cardHeight = $state(90);

  function validNotice(value: unknown): value is Notice {
    if (!value || typeof value !== 'object') return false;
    const item = value as Record<string, unknown>;
    return Number.isSafeInteger(item.id) && Number(item.id) > 0 &&
      typeof item.app === 'string' && item.app.length <= 256 &&
      typeof item.summary === 'string' && item.summary.length <= 1024 &&
      typeof item.body === 'string' && item.body.length <= 4096 &&
      typeof item.critical === 'boolean' && Array.isArray(item.actions) &&
      item.actions.length <= 8 && item.actions.every(action => {
        if (!action || typeof action !== 'object') return false;
        const pair = action as Record<string, unknown>;
        return typeof pair.key === 'string' && pair.key.length <= 256 &&
          typeof pair.label === 'string' && pair.label.length <= 256;
      });
  }

  function send(message: string) {
    try { window.webkit?.messageHandlers?.kiraNotificationUi?.postMessage(message); }
    catch { /* A dead renderer must not invent a successful close/action. */ }
  }

  onMount(() => {
    let lastRevision = 0;
    let disposed = false;
    const update = (event: Event) => {
      const next = (event as CustomEvent<unknown>).detail as Snapshot | undefined;
      if (!next || !Number.isSafeInteger(next.revision) || next.revision < 1 ||
          next.revision <= lastRevision ||
          !Array.isArray(next.visible) || next.visible.length > 3 ||
          !next.visible.every(validNotice) ||
          !Number.isSafeInteger(next.height) || next.height < 1 || next.height > 20000 ||
          !Number.isSafeInteger(next.cardHeight) || next.cardHeight < 1 ||
          next.cardHeight > Math.floor(next.height / 3)) return;
      lastRevision = next.revision;
      cardHeight = next.cardHeight;
      visible = next.visible;
      if (next.visible.length > 0) {
        // A native admission is not a painted frame. Confirm after Svelte's
        // DOM commit and compositor-bound rAFs so dwell >= 3s is real.
        const revision = next.revision;
        void tick().then(() => {
          if (disposed) return;
          requestAnimationFrame(() => requestAnimationFrame(() => {
            if (!disposed && lastRevision === revision) send('painted:' + revision);
          }));
        });
      }
    };
    window.addEventListener('kira:notifications:update', update);
    send('ready');
    return () => {
      disposed = true;
      window.removeEventListener('kira:notifications:update', update);
    };
  });
</script>

<main class="flex h-full w-full flex-col-reverse justify-start gap-2 bg-transparent p-2 text-kira-text"
  data-kira-ui-root aria-label="Уведомления Kira Desktop">
  {#each visible as notice (notice.id)}
    <NotificationCard {notice} height={cardHeight}
      onClose={(id) => send('close:' + id)}
      onAction={(id, key) => send('action:' + id + ':' + key)} />
  {/each}
</main>
