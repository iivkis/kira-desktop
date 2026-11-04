<script lang="ts">
  import gsap from 'gsap';

  export type Notice = {
    id: number;
    app: string;
    summary: string;
    body: string;
    critical: boolean;
    actions: { key: string; label: string }[];
  };

  let { notice, height, onClose, onAction }: {
    notice: Notice;
    height: number;
    onClose: (id: number) => void;
    onAction: (id: number, key: string) => void;
  } = $props();

  function enter(node: HTMLElement) {
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)');
    if (!reduce.matches) {
      // A real incoming event is the contact point. Never animate a fake
      // message or defer its content/action availability.
      gsap.fromTo(node, { y: 12, opacity: 0 }, {
        y: 0, opacity: 1, duration: 0.18, ease: 'power2.out',
        overwrite: 'auto',
      });
    }
    return { destroy: () => {
      gsap.killTweensOf(node);
      gsap.set(node, { clearProps: 'transform,opacity' });
    } };
  }
</script>

<article
  use:enter
  data-kira-notification-card={notice.id}
  class="relative flex w-full shrink-0 flex-col overflow-hidden border-l-[3px] border-kira-accent bg-kira-surface px-3 py-2 text-kira-text"
  style:height={height + 'px'}
  aria-label={'Уведомление от ' + notice.app}
>
  <div class="flex min-w-0 items-center gap-2">
    <span class="min-w-0 flex-1 truncate text-[12px] font-bold">{notice.app}</span>
    {#if notice.critical}<span class="shrink-0 text-[10px] font-bold text-kira-accent">ВАЖНО</span>{/if}
    <button type="button" class="shrink-0 px-1 text-kira-muted hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent"
      aria-label={'Закрыть уведомление от ' + notice.app}
      onclick={() => onClose(notice.id)}>×</button>
  </div>
  <div class="mt-0.5 line-clamp-1 text-[12px] font-semibold">{notice.summary}</div>
  {#if notice.body}
    <p class="mt-0.5 line-clamp-2 min-h-0 overflow-hidden text-[11px] text-kira-muted">{notice.body}</p>
  {/if}
  {#if notice.actions.length}
    <div class="mt-auto flex gap-3 overflow-hidden pt-1">
      {#each notice.actions.slice(0, 2) as action (action.key)}
        <button type="button" class="truncate text-[11px] font-semibold text-kira-accent hover:text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
          onclick={() => onAction(notice.id, action.key)}>{action.label}</button>
      {/each}
    </div>
  {/if}
</article>
