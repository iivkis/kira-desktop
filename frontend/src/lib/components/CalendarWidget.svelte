<script lang="ts">
  import { onMount, tick } from 'svelte';
  import WindowControls from './WindowControls.svelte';
  import { onCalendarCopyResult } from '../sdk/core';
  import { UI_LOCALE } from '../ui-locale';
  import SkyMap from './SkyMap.svelte';
  import { tenetCalendarGrid, tenetCalendarReveal, tenetClockDigit, tenetClockJuggle } from '../calendar-tenet-motion';
  import { tenetFlowIndicator } from '../motion';
  import {
    calendarDayState, clockGlyphs, isoDate, isoWeek, localClockText, localDateLabel, monthGrid,
    shiftDays, shiftMonth, todayCivilDate, type CivilDate,
  } from '../calendar-model';

  // The GTK/WebKit host manages window lifecycle and native clipboard.
  let {
    onClose,
    onPopout,
    onCopy,
    generation,
    locale,
  }: {
    onClose: () => void;
    onPopout: () => void;
    onCopy: (isoDate: string) => boolean;
    generation: number;
    locale?: string;
  } = $props();

  let detached = $state(false);
  let now = $state(new Date());
  let today = $state<CivilDate>(todayCivilDate(now));
  let selected = $state<CivilDate>(today);
  let visible = $state({ year: today.year, month: today.month });
  let copyState = $state<'idle' | 'pending' | 'copied' | 'failed'>('idle');
  let copyTimeout: number | null = null;

  const clockText = $derived(localClockText(now, locale ?? UI_LOCALE));
  const clockParts = $derived(clockGlyphs(clockText));
  const weeks = $derived(monthGrid(visible.year, visible.month));
  const selectedIso = $derived(isoDate(selected));
  const title = $derived(localDateLabel({ year: visible.year, month: visible.month, day: 1 }, locale ?? UI_LOCALE, { month: 'long' }));
  const selectedLabel = $derived(localDateLabel(selected, locale ?? UI_LOCALE, {
    weekday: 'long', day: 'numeric', month: 'long',
  }));
  const currentDateLabel = $derived(localDateLabel(today, locale ?? UI_LOCALE, {
    weekday: 'long', day: 'numeric', month: 'long', year: 'numeric',
  }));
  const weekdayLabels = $derived(Array.from({ length: 7 }, (_, index) =>
    localDateLabel({ year: 2024, month: 1, day: index + 1 }, locale ?? UI_LOCALE, { weekday: 'short' })));

  onMount(() => {
    const interval = window.setInterval(() => {
      now = new Date();
      today = todayCivilDate(now);
    }, 1_000);
    const onDetached = (event: Event) => {
      const current = (event as CustomEvent<{ generation: number }>).detail?.generation;
      if (current === generation) detached = true;
    };
    window.addEventListener('kira:overlay:detached', onDetached);
    const stopCopy = onCalendarCopyResult(generation, result => {
      if (result.date !== selectedIso || copyState !== 'pending') return;
      copyState = result.ok ? 'copied' : 'failed';
      scheduleCopyReset(1800);
    });
    return () => {
      stopCopy();
      if (copyTimeout !== null) window.clearTimeout(copyTimeout);
      window.clearInterval(interval);
      window.removeEventListener('kira:overlay:detached', onDetached);
    };
  });

  function scheduleCopyReset(ms: number) {
    if (copyTimeout !== null) window.clearTimeout(copyTimeout);
    copyTimeout = window.setTimeout(() => {
      copyTimeout = null;
      copyState = copyState === 'pending' ? 'failed' : 'idle';
      if (copyState === 'failed') scheduleCopyReset(1800);
    }, ms);
  }

  function copySelectedDate() {
    copyState = 'pending';
    if (!onCopy(selectedIso)) {
      copyState = 'failed';
      scheduleCopyReset(1800);
    } else {
      // The event may be dropped by an outdated Shell: never imply success.
      scheduleCopyReset(2500);
    }
  }

  function navigate(months: number) {
    const next = shiftMonth(selected, months);
    if (next) {
      selected = next;
      copyState = 'idle';
      visible = { year: next.year, month: next.month };
    }
  }

  function choose(date: CivilDate) {
    selected = date;
    copyState = 'idle';
    visible = { year: date.year, month: date.month };
  }

  function goToday() {
    selected = todayCivilDate();
    copyState = 'idle';
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
  class={'kira-calendar-scroll relative flex min-h-0 w-full bg-kira-bg text-kira-text ' +
    (detached
      ? 'h-full flex-row overflow-hidden max-[760px]:flex-col max-[760px]:overflow-y-auto max-[760px]:overflow-x-hidden'
      : 'h-auto flex-col overflow-y-auto')}
  data-calendar-mode={detached ? 'window' : 'widget'}
  aria-label="Kira Calendar"
>
  <div data-calendar-rail class={detached
    ? 'flex h-full w-[370px] shrink-0 flex-col overflow-y-auto border-r border-kira-muted/20 max-[760px]:h-auto max-[760px]:w-full max-[760px]:overflow-visible max-[760px]:border-r-0 max-[760px]:border-b'
    : 'flex w-full min-h-0 flex-col'}>
  <!-- The live clock and date are enough context; no editorial chapter labels. -->
  <header data-kira-enter class="shrink-0 bg-kira-bg px-5 pb-3 pt-4">
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <button type="button" use:tenetClockJuggle data-calendar-clock-play
          aria-label="Анимировать цифры часов"
          title="Анимировать цифры часов"
          class="block cursor-pointer rounded-sm text-left focus-visible:outline-2 focus-visible:outline-kira-accent">
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
        <p class="mt-1 truncate text-[11px] font-semibold text-kira-muted">{currentDateLabel}</p>
      </div>
      <WindowControls {onClose} {onPopout} {generation} label="calendar" />
    </div>
  </header>

  <!-- Each date has three independent semantics: current, selected, adjacent. -->
  <section data-kira-enter class="min-h-0 flex-1 bg-kira-surface px-4 pb-3 pt-3" aria-label="Выбор даты">
    <div class="mb-3 flex items-center justify-between gap-2">
      <div class="min-w-0">
        <h2 class="truncate text-[21px] font-extrabold leading-tight tracking-tight capitalize">
          {title}<span class="ml-2 text-sm font-semibold text-kira-muted">{visible.year}</span>
        </h2>
      </div>
      <div class="flex shrink-0 items-center gap-1" role="group" aria-label="Навигация по календарю">
        {#each [-12, -1, 1, 12] as step}
          <button type="button"
            class="flex h-8 w-8 items-center justify-center border border-kira-muted/15 bg-kira-bg/20
              text-[17px] font-semibold text-kira-text transition-colors
              hover:border-kira-accent hover:text-kira-accent
              focus-visible:outline-2 focus-visible:outline-kira-accent disabled:cursor-default disabled:opacity-30"
            disabled={!shiftMonth(selected, step)}
            onclick={() => navigate(step)}
            title={step === -12 ? 'Предыдущий год' : step === -1 ? 'Предыдущий месяц' : step === 1 ? 'Следующий месяц' : 'Следующий год'}
            aria-label={step === -12 ? 'Предыдущий год' : step === -1 ? 'Предыдущий месяц' : step === 1 ? 'Следующий месяц' : 'Следующий год'}
          >{step === -12 ? '«' : step === -1 ? '‹' : step === 1 ? '›' : '»'}</button>
        {/each}
      </div>
    </div>

    <div class="relative grid grid-cols-8 gap-y-0.5 text-center" role="group" aria-label="Даты календаря"
      use:tenetCalendarGrid={{ month: visible.year * 12 + visible.month, selected: selectedIso }}
      use:tenetFlowIndicator={{ active: selectedIso, axis: 'both' }}>
      <span data-kira-flow-indicator aria-hidden="true"
        class="pointer-events-none absolute left-0 top-0 z-0 bg-kira-accent"></span>
      <span class="flex min-h-7 items-center justify-center border-b border-r border-kira-muted/15 bg-kira-bg/20
        text-[11px] font-bold tracking-wide text-kira-muted" aria-label="Номер недели">НЕД</span>
      {#each weekdayLabels as label}
        <span class="flex min-h-7 items-center justify-center border-b border-kira-muted/15
          text-[11px] font-bold uppercase text-kira-muted">{label}</span>
      {/each}
      {#each weeks as days}
        <span class="flex min-h-8 items-center justify-center border-r border-kira-muted/15
          bg-kira-bg/15 text-[11px] font-semibold tabular-nums text-kira-muted"
          aria-label={'ISO-неделя ' + (days[0] ? isoWeek(days[0]).week : '')}>
          {days[0] ? String(isoWeek(days[0]).week).padStart(2, '0') : ''}
        </span>
        {#each days as date}
          {#if date}
            {@const marker = calendarDayState(date, selected, today, visible)}
            <button type="button"
              class={'relative z-10 flex min-h-8 items-center justify-center text-xs font-semibold tabular-nums focus-visible:z-20 focus-visible:outline-2 focus-visible:outline-kira-accent ' +
                (marker.outside ? 'text-kira-muted/60 ' : 'text-kira-text ') +
                (marker.selected ? 'font-extrabold text-kira-on-accent focus-visible:-outline-offset-2 focus-visible:outline-kira-on-accent ' :
                  'hover:bg-kira-bg/35 hover:text-kira-accent ') +
                (marker.today && !marker.selected ? 'underline decoration-2 decoration-kira-accent underline-offset-4' : '')}
              aria-label={localDateLabel(date, locale ?? UI_LOCALE)}
              aria-pressed={marker.selected}
              aria-current={marker.today ? 'date' : undefined}
              tabindex={marker.selected ? 0 : -1}
              data-calendar-day
              data-kira-tab={isoDate(date)}
              data-calendar-selected={marker.selected}
              onclick={() => choose(date)}
            >{date.day}</button>
          {:else}
            <span aria-hidden="true"></span>
          {/if}
        {/each}
      {/each}
    </div>
  </section>

  <!-- Selected date is a factual summary, not a fictitious event agenda. -->
  <footer data-kira-enter class="shrink-0 border-t border-kira-muted/15 bg-kira-bg px-5 pb-3 pt-3">
    <div class="flex items-center justify-between gap-3" aria-live="polite">
      <p class="min-w-0 truncate text-[15px] font-extrabold capitalize" title={localDateLabel(selected, locale ?? UI_LOCALE)}>
        {selectedLabel}
      </p>
      <p class="shrink-0 text-[11px] font-semibold tabular-nums text-kira-muted">{selectedIso}</p>
    </div>
    <div class="mt-2 flex items-center justify-between border-t border-kira-muted/15 pt-2">
      <button type="button" onclick={goToday}
        class="px-1 py-1.5 text-[11px] font-bold tracking-wide text-kira-accent
          hover:text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent">СЕГОДНЯ</button>
      <button type="button" onclick={copySelectedDate}
        aria-label={copyState === 'copied' ? 'Дата ' + selectedIso + ' скопирована' :
          copyState === 'pending' ? 'Копирование даты ' + selectedIso : 'Скопировать дату ' + selectedIso}
        class="px-1 py-1.5 text-[11px] font-bold tracking-wide text-kira-accent
          hover:text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent">{copyState === 'copied' ? 'СКОПИРОВАНО' : copyState === 'pending' ? 'КОПИРОВАНИЕ…' : 'КОПИРОВАТЬ'}</button>
    </div>
    <span role="status" aria-live="polite" aria-atomic="true"
        class={copyState === 'failed' ? 'mt-1 block text-[11px] font-semibold text-kira-accent' : 'sr-only'}>
        {copyState === 'copied' ? 'Дата ' + selectedIso + ' скопирована' :
          copyState === 'failed' ? 'НЕ УДАЛОСЬ СКОПИРОВАТЬ' :
            copyState === 'pending' ? 'Копирование даты' : ''}
    </span>
  </footer>
  </div>
  {#if detached}
    <SkyMap date={selected} />
  {/if}
</main>
