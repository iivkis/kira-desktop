<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { calendarGridMotion, clockJuggle, revealStagger, rollingClockDigit } from '../motion';
  import {
    clockGlyphs, dayOfYear, isoDate, isoWeek, localClockText, localDateLabel, monthGrid,
    shiftDays, shiftMonth, todayCivilDate, type CivilDate,
  } from '../calendar-model';

  // The GTK/WebKit host manages window lifecycle and native clipboard.
  let {
    onClose,
    onCopy,
    locale,
  }: {
    onClose: () => void;
    onCopy: (isoDate: string) => void;
    locale?: string;
  } = $props();

  let now = $state(new Date());
  let today = $state<CivilDate>(todayCivilDate(now));
  let selected = $state<CivilDate>(today);
  let visible = $state({ year: today.year, month: today.month });

  const clockText = $derived(localClockText(now, locale));
  const clockParts = $derived(clockGlyphs(clockText));
  const weeks = $derived(monthGrid(visible.year, visible.month));
  const selectedIso = $derived(isoDate(selected));
  const todayIso = $derived(isoDate(today));
  const week = $derived(isoWeek(selected));
  const title = $derived(localDateLabel({ year: visible.year, month: visible.month, day: 1 }, locale, { month: 'long' }));
  const weekdayLabels = $derived(Array.from({ length: 7 }, (_, index) =>
    localDateLabel({ year: 2024, month: 1, day: index + 1 }, locale, { weekday: 'short' })));

  onMount(() => {
    const interval = window.setInterval(() => {
      now = new Date();
      today = todayCivilDate(now);
    }, 1_000);
    return () => window.clearInterval(interval);
  });

  function navigate(months: number) {
    const next = shiftMonth(selected, months);
    if (next) {
      selected = next;
      visible = { year: next.year, month: next.month };
    }
  }

  function choose(date: CivilDate) {
    selected = date;
    visible = { year: date.year, month: date.month };
  }

  function goToday() {
    selected = todayCivilDate();
    visible = { year: selected.year, month: selected.month };
  }

  function focusSelectedDay() {
    void tick().then(() => {
      document.querySelector<HTMLButtonElement>('button[data-calendar-selected="true"]')?.focus();
    });
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
      return;
    }
    // Arrow navigation belongs to the day grid, not the close/toolbar buttons.
    if (!(event.target instanceof HTMLElement) || !event.target.hasAttribute('data-calendar-day')) return;
    const step = event.key === 'ArrowLeft' ? -1
      : event.key === 'ArrowRight' ? 1
        : event.key === 'ArrowUp' ? -7
          : event.key === 'ArrowDown' ? 7
            : null;
    if (step !== null) {
      event.preventDefault();
      const next = shiftDays(selected, step);
      if (next) { choose(next); focusSelectedDay(); }
    } else if (event.key === 'Home') {
      event.preventDefault();
      goToday();
      focusSelectedDay();
    } else if (event.key === 'PageUp' || event.key === 'PageDown') {
      event.preventDefault();
      navigate((event.key === 'PageUp' ? -1 : 1) * (event.ctrlKey ? 12 : 1));
      focusSelectedDay();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<main
  data-kira-ui-root
  use:revealStagger
  class="flex h-auto min-h-0 w-full flex-col overflow-y-auto bg-kira-bg text-kira-text"
  aria-label="Kira Calendar"
>
  <section data-kira-enter class="flex items-start justify-between gap-4 bg-kira-bg px-5 pb-4 pt-5">
    <div class="min-w-0">
      <span class="text-[10px] font-bold tracking-[0.12em] text-kira-accent">KIRA / CALENDAR</span>
      <button type="button" use:clockJuggle data-calendar-clock-play
        aria-label="Make the clock digits fly and return"
        title="Click for a little clock chaos"
        class="mt-1 block cursor-pointer rounded-sm text-left focus-visible:outline-2 focus-visible:outline-kira-accent">
        <time class="flex items-center text-4xl font-extrabold tabular-nums tracking-tight"
          datetime={now.toISOString()} aria-label={clockText} data-calendar-clock>
        {#each clockParts as part, index (index)}
          {#if part.digit}
            <span class="relative inline-flex h-[1.15em] w-[0.64em] shrink-0 items-center justify-center overflow-hidden"
              aria-hidden="true" data-clock-cell>
              <span class="flex h-full w-full items-center justify-center"
                data-clock-digit use:rollingClockDigit={part.char}>{part.char}</span>
            </span>
          {:else}
            <span aria-hidden="true" class="inline-block shrink-0 px-[0.025em] text-kira-muted">{part.char}</span>
          {/if}
        {/each}
        </time>
      </button>
      <p class="mt-1 text-xs text-kira-muted">
        {new Intl.DateTimeFormat(locale, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }).format(now)}
      </p>
    </div>
    <button type="button" onclick={onClose}
      class="rounded px-2 py-1 text-xl text-kira-muted hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent"
      title="Close calendar" aria-label="Close calendar">×</button>
  </section>

  <section data-kira-enter class="min-h-0 flex-1 bg-kira-surface px-4 py-4" aria-label="Date picker">
    <div class="mb-4 flex items-center justify-between gap-2">
      <div class="min-w-0">
        <div class="text-[10px] font-semibold tracking-wider text-kira-muted">SELECT DATE</div>
        <h2 class="truncate text-base font-extrabold">{title} <span class="text-kira-muted">{visible.year}</span></h2>
      </div>
      <div class="flex shrink-0 items-center gap-1">
        {#each [-12, -1, 1, 12] as step}
          <button type="button"
            class="h-8 min-w-8 rounded text-kira-muted hover:bg-kira-bg/30 hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent disabled:opacity-30"
            disabled={!shiftMonth(selected, step)}
            onclick={() => navigate(step)}
            title={step === -12 ? 'Previous year' : step === -1 ? 'Previous month' : step === 1 ? 'Next month' : 'Next year'}
            aria-label={step === -12 ? 'Previous year' : step === -1 ? 'Previous month' : step === 1 ? 'Next month' : 'Next year'}
          >{step === -12 ? '«' : step === -1 ? '‹' : step === 1 ? '›' : '»'}</button>
        {/each}
      </div>
    </div>

    <div class="grid grid-cols-8 gap-y-1 text-center" aria-label="Calendar dates"
      use:calendarGridMotion={{ month: visible.year * 12 + visible.month, selected: selectedIso }}>
      <span class="py-2 text-[10px] font-bold text-kira-muted" aria-label="Week number">WK</span>
      {#each weekdayLabels as label}
        <span class="py-2 text-[10px] font-bold text-kira-muted">{label}</span>
      {/each}
      {#each weeks as days}
        <span class="flex items-center justify-center text-[10px] text-kira-muted/70">
          {days[0] ? String(isoWeek(days[0]).week).padStart(2, '0') : ''}
        </span>
        {#each days as date}
          {#if date}
            <button type="button"
              class={'min-h-9 rounded text-xs font-semibold focus-visible:outline-2 focus-visible:outline-kira-accent ' +
                (date.year !== visible.year || date.month !== visible.month ? 'text-kira-muted/70 ' : 'text-kira-text ') +
                (isoDate(date) === selectedIso ? 'bg-kira-accent text-kira-on-accent focus-visible:-outline-offset-2 focus-visible:outline-kira-on-accent ' : 'hover:bg-kira-bg/40 hover:text-kira-accent ') +
                (isoDate(date) === todayIso && isoDate(date) !== selectedIso ? 'underline decoration-kira-accent underline-offset-4' : '')}
              aria-label={localDateLabel(date, locale)}
              aria-pressed={isoDate(date) === selectedIso}
              data-calendar-day
              data-calendar-selected={isoDate(date) === selectedIso}
              onclick={() => choose(date)}
            >{date.day}</button>
          {:else}
            <span aria-hidden="true"></span>
          {/if}
        {/each}
      {/each}
    </div>
  </section>

  <section data-kira-enter class="shrink-0 bg-kira-bg px-5 pb-5 pt-4">
    <p class="text-sm font-bold" aria-live="polite">{localDateLabel(selected, locale)}</p>
    <p class="mt-1 text-[10px] font-semibold tracking-wider text-kira-muted">
      ISO WEEK {String(week.week).padStart(2, '0')} / {week.year}
      · DAY {String(dayOfYear(selected)).padStart(3, '0')} / {selected.year}
    </p>
    <div class="mt-3 flex items-center justify-between">
      <button type="button" onclick={goToday}
        class="px-1 py-2 text-xs font-bold text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">TODAY</button>
      <button type="button" onclick={() => onCopy(selectedIso)}
        class="px-1 py-2 text-xs font-bold text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">COPY DATE</button>
    </div>
  </section>
</main>
