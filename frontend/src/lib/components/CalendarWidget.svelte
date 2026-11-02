<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { tenetCalendarGrid, tenetCalendarReveal, tenetClockDigit, tenetClockJuggle } from '../calendar-tenet-motion';
  import {
    calendarDayState, clockGlyphs, dayOfYear, isoDate, isoWeek, localClockText, localDateLabel, monthGrid,
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
  const selectedLabel = $derived(localDateLabel(selected, locale, {
    weekday: 'long', day: 'numeric', month: 'long',
  }));
  const currentDateLabel = $derived(localDateLabel(today, locale, {
    weekday: 'long', day: 'numeric', month: 'long', year: 'numeric',
  }));
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
  use:tenetCalendarReveal
  class="kira-calendar-scroll relative flex h-auto min-h-0 w-full flex-col overflow-y-auto bg-kira-bg text-kira-text"
  aria-label="Kira Calendar"
>
  <!-- Clock is the identity anchor; native host remains the only window owner. -->
  <header data-kira-enter class="shrink-0 bg-kira-bg px-5 pb-3 pt-4">
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0 border-l-2 border-kira-accent pl-3">
        <p class="text-[10px] font-extrabold tracking-[0.14em] text-kira-accent">KIRA / CHRONOLOGY</p>
        <p class="mt-1 text-[9px] font-semibold tracking-[0.12em] text-kira-muted">01 / LOCAL TIME</p>
      </div>
      <button type="button" onclick={onClose}
        class="rounded-sm px-2 py-1 text-xl leading-none text-kira-muted transition-colors
          hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent"
        title="Close calendar" aria-label="Close calendar">×</button>
    </div>
    <button type="button" use:tenetClockJuggle data-calendar-clock-play
      aria-label="Make the clock digits fly and return"
      title="Animate clock digits"
      class="mt-1 block cursor-pointer rounded-sm text-left focus-visible:outline-2 focus-visible:outline-kira-accent">
      <time class="flex items-center text-[36px] font-extrabold leading-none tabular-nums tracking-[-0.055em]"
        datetime={now.toISOString()} aria-label={clockText} data-calendar-clock>
      {#each clockParts as part, index (index)}
        {#if part.digit}
          <span class="relative inline-flex h-[1.15em] w-[0.64em] shrink-0 items-center justify-center overflow-hidden"
            aria-hidden="true" data-clock-cell>
            <span class="flex h-full w-full items-center justify-center"
              data-clock-digit use:tenetClockDigit={part.char}>{part.char}</span>
          </span>
        {:else}
          <span aria-hidden="true" class="inline-block shrink-0 px-[0.025em] text-kira-muted">{part.char}</span>
        {/if}
      {/each}
      </time>
    </button>
    <div class="mt-2 flex items-center justify-between gap-2 border-t border-kira-muted/15 pt-2">
      <p class="min-w-0 truncate text-[11px] font-semibold text-kira-text">{currentDateLabel}</p>
      <span class="shrink-0 text-[9px] font-bold tracking-[0.12em] text-kira-muted">LIVE / LOCAL</span>
    </div>
  </header>

  <!-- Each date has three independent semantics: current, selected, adjacent. -->
  <section data-kira-enter class="min-h-0 flex-1 bg-kira-surface px-4 pb-3 pt-3" aria-label="Date picker">
    <div class="mb-3 flex items-center justify-between gap-2">
      <div class="min-w-0">
        <p class="text-[9px] font-bold tracking-[0.14em] text-kira-muted">02 / DATE INDEX</p>
        <h2 class="mt-1 truncate text-[21px] font-extrabold leading-tight tracking-tight capitalize">
          {title}<span class="ml-2 text-sm font-semibold text-kira-muted">{visible.year}</span>
        </h2>
      </div>
      <div class="flex shrink-0 items-center gap-1" role="group" aria-label="Navigate calendar">
        {#each [-12, -1, 1, 12] as step}
          <button type="button"
            class="flex h-8 w-8 items-center justify-center border border-kira-muted/15 bg-kira-bg/20
              text-[17px] font-semibold text-kira-text transition-colors
              hover:border-kira-accent hover:text-kira-accent
              focus-visible:outline-2 focus-visible:outline-kira-accent disabled:cursor-default disabled:opacity-30"
            disabled={!shiftMonth(selected, step)}
            onclick={() => navigate(step)}
            title={step === -12 ? 'Previous year' : step === -1 ? 'Previous month' : step === 1 ? 'Next month' : 'Next year'}
            aria-label={step === -12 ? 'Previous year' : step === -1 ? 'Previous month' : step === 1 ? 'Next month' : 'Next year'}
          >{step === -12 ? '«' : step === -1 ? '‹' : step === 1 ? '›' : '»'}</button>
        {/each}
      </div>
    </div>

    <div class="relative grid grid-cols-8 gap-y-0.5 text-center" role="group" aria-label="Calendar dates"
      use:tenetCalendarGrid={{ month: visible.year * 12 + visible.month, selected: selectedIso }}>
      <span class="flex min-h-7 items-center justify-center border-b border-r border-kira-muted/15 bg-kira-bg/20
        text-[9px] font-bold tracking-wide text-kira-muted" aria-label="Week number">WK</span>
      {#each weekdayLabels as label}
        <span class="flex min-h-7 items-center justify-center border-b border-kira-muted/15
          text-[10px] font-bold uppercase text-kira-muted">{label}</span>
      {/each}
      {#each weeks as days}
        <span class="flex min-h-8 items-center justify-center border-r border-kira-muted/15
          bg-kira-bg/15 text-[9px] font-semibold tabular-nums text-kira-muted"
          aria-label={'ISO week ' + (days[0] ? isoWeek(days[0]).week : '')}>
          {days[0] ? String(isoWeek(days[0]).week).padStart(2, '0') : ''}
        </span>
        {#each days as date}
          {#if date}
            {@const marker = calendarDayState(date, selected, today, visible)}
            <button type="button"
              class={'relative flex min-h-8 items-center justify-center text-xs font-semibold tabular-nums
                focus-visible:z-10 focus-visible:outline-2 focus-visible:outline-kira-accent ' +
                (marker.outside ? 'text-kira-muted/60 ' : 'text-kira-text ') +
                (marker.selected ? 'bg-kira-accent font-extrabold text-kira-on-accent focus-visible:-outline-offset-2 focus-visible:outline-kira-on-accent ' :
                  'hover:bg-kira-bg/35 hover:text-kira-accent ') +
                (marker.today && !marker.selected ? 'underline decoration-2 decoration-kira-accent underline-offset-4' : '')}
              aria-label={localDateLabel(date, locale)}
              aria-pressed={marker.selected}
              aria-current={marker.today ? 'date' : undefined}
              tabindex={marker.selected ? 0 : -1}
              data-calendar-day
              data-calendar-selected={marker.selected}
              onclick={() => choose(date)}
            >{date.day}</button>
          {:else}
            <span aria-hidden="true"></span>
          {/if}
        {/each}
      {/each}
    </div>
    <div class="mt-2 flex items-center justify-between text-[9px] font-semibold tracking-wide text-kira-muted">
      <span>MON / SUN · ISO WEEKS</span>
      <span><span class="text-kira-accent">_</span> TODAY</span>
    </div>
  </section>

  <!-- Selected date is a factual summary, not a fictitious event agenda. -->
  <footer data-kira-enter class="shrink-0 border-t border-kira-muted/15 bg-kira-bg px-5 pb-3 pt-3">
    <div class="flex items-start justify-between gap-3" aria-live="polite">
      <div class="min-w-0 border-l-2 border-kira-accent pl-3">
        <p class="text-[9px] font-bold tracking-[0.13em] text-kira-muted">
          03 / {selectedIso === todayIso ? 'TODAY' : 'SELECTED DATE'}
        </p>
        <p class="mt-1 truncate text-[15px] font-extrabold capitalize" title={localDateLabel(selected, locale)}>
          {selectedLabel}
        </p>
      </div>
      <div class="shrink-0 text-right">
        <p class="text-[10px] font-bold tabular-nums text-kira-accent">{selectedIso}</p>
        <p class="mt-1 text-[9px] font-semibold tracking-wide text-kira-muted">
          WK {String(week.week).padStart(2, '0')} · D{String(dayOfYear(selected)).padStart(3, '0')}
        </p>
      </div>
    </div>
    <div class="mt-2 flex items-center justify-between border-t border-kira-muted/15 pt-2">
      <button type="button" onclick={goToday}
        class="px-1 py-1.5 text-[11px] font-bold tracking-wide text-kira-accent
          hover:text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent">TODAY</button>
      <button type="button" onclick={() => onCopy(selectedIso)}
        aria-label={'Copy date ' + selectedIso}
        class="px-1 py-1.5 text-[11px] font-bold tracking-wide text-kira-accent
          hover:text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent">COPY DATE</button>
    </div>
  </footer>
</main>
