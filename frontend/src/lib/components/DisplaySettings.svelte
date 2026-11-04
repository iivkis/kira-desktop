<script lang="ts">
  import { onMount, tick } from 'svelte';
  import {
    onDisplayStatus, requestDisplays, applyDisplays, confirmDisplays, revertDisplays,
    displayModeId, saveDisplays, type DisplayOutput, type DisplayDraft, type DisplayStatus,
  } from '../sdk/core';

  let { generation }: { generation: number } = $props();

  let outputs = $state<DisplayOutput[]>([]);
  let draft = $state<DisplayDraft[]>([]);
  let selected = $state('');
  let busy = $state(false);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let statusText = $state('');
  let canSave = $state(false);
  let persistence = $state<DisplayStatus['persistence']>('manual');
  let pendingId = $state<number | null>(null);
  let deadlineMs = $state<number | null>(null);
  let clockNow = $state(Date.now());
  const remaining = $derived(deadlineMs === null ? null :
    Math.max(0, Math.ceil((deadlineMs - clockNow) / 1_000)));
  let drag = $state<{ connector: string; px: number; py: number; x: number; y: number; zoom: number } | null>(null);

  let mapViewport = $state<HTMLDivElement | null>(null);
  let mapWidth = $state(500);
  let confirmButton = $state<HTMLButtonElement | null>(null);

  const selectedOutput = $derived(outputs.find(output => output.connector === selected));
  const selectedDraft = $derived(draft.find(output => output.connector === selected));
  const activeCount = $derived(draft.filter(out => out.enabled).length);
  const isPending = $derived(pendingId !== null);

  function logicalSize(output: DisplayDraft): { width: number; height: number } {
    const source = outputs.find(item => item.connector === output.connector);
    const mode = source?.modes.find(mode => displayModeId(mode) === output.mode);
    if (!mode) return { width: 800, height: 450 };
    const swapped = output.transform.includes('90') || output.transform.includes('270');
    return {
      width: Math.round((swapped ? mode.height : mode.width) / output.scale),
      height: Math.round((swapped ? mode.width : mode.height) / output.scale),
    };
  }

  const rectangles = $derived(draft.filter(out => out.enabled).map(out => ({
    ...out, ...logicalSize(out),
  })));
  const minX = $derived(rectangles.length ? Math.min(...rectangles.map(r => r.x)) : 0);
  const minY = $derived(rectangles.length ? Math.min(...rectangles.map(r => r.y)) : 0);
  const spanX = $derived(rectangles.length
    ? Math.max(...rectangles.map(r => r.x + r.width)) - minX : 1920);
  const spanY = $derived(rectangles.length
    ? Math.max(...rectangles.map(r => r.y + r.height)) - minY : 1080);
  const zoom = $derived(Math.min(0.22, Math.max(1, mapWidth - 28) / Math.max(spanX, 1), 184 / Math.max(spanY, 1)));

  function draftFromOutputs(data: DisplayOutput[]): DisplayDraft[] {
    let nextX = 0;
    return data.map(output => {
      const current = output.current_mode === null ? null : output.modes[output.current_mode];
      const mode = current ?? output.modes.find(m => m.is_preferred) ?? output.modes[0];
      const logical = output.logical;
      const result: DisplayDraft = {
        connector: output.connector,
        enabled: logical !== null,
        mode: mode ? displayModeId(mode) : '',
        scale: logical?.scale ?? 1,
        transform: logical?.transform === 'Normal' ? 'normal'
          : logical?.transform?.replace('Flipped', 'flipped-').replace('-90', '90') ?? 'normal',
        x: logical?.x ?? nextX,
        y: logical?.y ?? 0,
        vrr: output.vrr_enabled,
      };
      const size = mode ? Math.round(mode.width / result.scale) : 800;
      nextX = Math.max(nextX, result.x + size);
      return result;
    });
  }

  function receive(message: DisplayStatus) {
    loading = false;
    busy = false;
    persistence = message.persistence;
    if (message.state === 'pending' && message.pending_id !== null) {
      pendingId = message.pending_id;
      deadlineMs = message.deadline_unix_ms ?? null;
      clockNow = Date.now();
      error = null;
      statusText = 'Новая конфигурация активна. Подтвердите изменения.';
      return;
    }
    if (message.state === 'error') {
      error = message.error ?? 'Niri не смог применить изменения';
      statusText = '';
      return;
    }
    pendingId = null;
    deadlineMs = null;
    if (message.state === 'confirmed') canSave = true;
    if (message.state === 'saved') canSave = false;
    if (message.state === 'reverted') canSave = false;
    if (message.outputs !== null) {
      outputs = message.outputs;
      draft = draftFromOutputs(outputs);
      if (!outputs.some(out => out.connector === selected)) selected = outputs[0]?.connector ?? '';
    }
    if (message.error) error = message.error;
    else error = null;
    statusText = message.state === 'reverted' ? 'Предыдущая конфигурация восстановлена.'
      : message.state === 'confirmed' ? 'Изменения действуют в сеансе. Можно сохранить декларативно.'
      : message.state === 'saved' ? 'Конфигурация сохранена в KDL и будет загружена при следующем запуске Niri.'
        : '';
  }

  function change(patch: Partial<DisplayDraft>) {
    if (busy || isPending) return;
    canSave = false;
    draft = draft.map(out => out.connector === selected ? { ...out, ...patch } : out);
  }

  function reset() {
    if (busy || isPending) return;
    canSave = false;
    draft = draftFromOutputs(outputs);
    error = null;
    statusText = '';
  }

  function refresh() {
    if (busy || isPending) return;
    canSave = false;
    loading = true;
    error = null;
    if (!requestDisplays(generation)) {
      loading = false;
      error = 'Нет соединения с нативным Shell';
    }
  }

  function apply() {
    if (busy || isPending) return;
    canSave = false;
    busy = true;
    error = null;
    statusText = '';
    if (!applyDisplays(generation, draft)) {
      busy = false;
      error = 'Не удалось отправить конфигурацию в Shell';
    }
  }

  function confirm() {
    if (pendingId !== null) {
      busy = confirmDisplays(generation, pendingId);
      if (!busy) error = 'Нет соединения с Shell';
    }
  }
  function revert() {
    if (pendingId !== null) {
      busy = revertDisplays(generation, pendingId);
      if (!busy) error = 'Нет соединения с Shell';
    }
  }

  function save() {
    if (busy || !canSave || isPending) return;
    busy = saveDisplays(generation);
    if (!busy) error = 'Не удалось отправить команду сохранения';
  }

  function snapPosition(connector: string, rawX: number, rawY: number) {
    const moving = draft.find(out => out.connector === connector);
    if (!moving) return { x: rawX, y: rawY };
    const self = logicalSize(moving);
    let x = rawX;
    let y = rawY;
    for (const item of draft.filter(d => d.enabled && d.connector !== connector)) {
      const size = logicalSize(item);
      for (const edge of [item.x + size.width, item.x - self.width]) {
        if (Math.abs(rawX - edge) < 60) x = edge;
      }
      for (const edge of [item.y + size.height, item.y - self.height]) {
        if (Math.abs(rawY - edge) < 60) y = edge;
      }
      if (Math.abs(rawY - item.y) < 60) y = item.y;
      if (Math.abs(rawX - item.x) < 60) x = item.x;
    }
    return { x: Math.max(-32000, Math.min(32000, x)), y: Math.max(-32000, Math.min(32000, y)) };
  }

  function onPointerDown(event: PointerEvent, connector: string) {
    if (busy || isPending) return;
    const found = draft.find(out => out.connector === connector);
    if (!found) return;
    selected = connector;
    drag = { connector, px: event.clientX, py: event.clientY, x: found.x, y: found.y, zoom };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }
  function onPointerMove(event: PointerEvent) {
    if (!drag) return;
    const { connector, px, py, x, y, zoom: factor } = drag;
    const next = snapPosition(connector, Math.round(x + (event.clientX - px) / factor),
      Math.round(y + (event.clientY - py) / factor));
    canSave = false;
    draft = draft.map(out => out.connector === connector ? { ...out, ...next } : out);
  }
  function onPointerUp() { drag = null; }

  // The display map exists only after Niri supplies outputs; observe the
  // actual width then. It may disappear when all outputs are disconnected.
  $effect(() => {
    if (!mapViewport) return;
    const observer = new ResizeObserver(entries => {
      const width = entries[0]?.contentRect.width;
      if (width && width > 0) mapWidth = width;
    });
    observer.observe(mapViewport);
    return () => observer.disconnect();
  });

  // Preview is a timed transaction. Move keyboard focus to confirmation
  // when its 15-second native rollback watchdog starts.
  $effect(() => {
    const id = pendingId;
    if (id === null) return;
    void tick().then(() => {
      if (pendingId === id && !busy) confirmButton?.focus();
    });
  });

  onMount(() => {
    const stop = onDisplayStatus(generation, receive);
    refresh();
    const clock = window.setInterval(() => {
      if (pendingId !== null) clockNow = Date.now();
    }, 250);
    return () => {
      stop();
      window.clearInterval(clock);
    };
  });
</script>

<section class="space-y-5" aria-label="Настройки дисплеев">
  <h2 class="text-[26px] font-extrabold tracking-tight">Дисплеи</h2>

  {#if pendingId !== null}
    <div role="alertdialog" aria-label="Подтвердите параметры дисплеев"
      aria-describedby="kira-display-confirmation-warning"
      class="border-l-[3px] border-kira-accent bg-kira-surface p-4">
      <div class="flex justify-between gap-3">
        <div>
          <p class="text-[13px] font-extrabold">Оставить новые параметры экрана?</p>
          <p id="kira-display-confirmation-warning" class="mt-1 text-[12px] leading-relaxed text-kira-muted">
            Изменения действуют только в этом сеансе. Если изображение пропало, настройки откатятся автоматически. Сохранение в KDL выполняется отдельно.
          </p>
        </div>
        {#if remaining !== null}
           <strong class="text-[23px] font-black tabular-nums text-kira-accent" aria-label={'До автоматического отката ' + remaining + ' секунд'}>{remaining}</strong>
         {:else}
           <span class="text-[12px] font-bold text-kira-accent">Ожидание подтверждения</span>
         {/if}
      </div>
      <div class="mt-4 flex flex-wrap gap-2">
        <button type="button" bind:this={confirmButton} onclick={confirm} disabled={busy}
          class="bg-kira-accent px-4 py-2 text-[12px] font-extrabold text-kira-on-accent focus-visible:outline-2 focus-visible:outline-kira-text disabled:opacity-50">
          Оставить изменения
        </button>
        <button type="button" onclick={revert} disabled={busy}
          class="border border-kira-muted/40 px-4 py-2 text-[12px] font-bold text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent disabled:opacity-50">
          Вернуть как было
        </button>
      </div>
    </div>
  {/if}

  {#if error}
    <p role="alert" class="border-l-2 border-kira-accent bg-kira-surface px-3 py-3 text-[11px] leading-relaxed">{error}</p>
  {/if}
  {#if statusText}
    <p role="status" class="border-l-2 border-kira-accent px-3 py-2 text-[11px] text-kira-muted">{statusText}</p>
  {/if}

  <div class="flex flex-wrap items-center justify-between gap-3">
    <div class="text-[11px] font-semibold text-kira-muted">
      {#if loading}Поиск дисплеев…{:else}{outputs.length} подключено · {activeCount} включено{/if}
    </div>
    <button type="button" onclick={refresh} disabled={loading || busy || isPending}
      class="border border-kira-muted/40 px-3 py-2 text-[11px] font-bold text-kira-text hover:border-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent disabled:opacity-40">
      Обновить список
    </button>
  </div>

  {#if !loading && outputs.length === 0}
    <div class="border border-kira-muted/20 bg-kira-surface-soft p-5 text-[12px] leading-relaxed text-kira-muted">
      Niri не сообщил о подключённых дисплеях. Запустите Kira Desktop внутри работающей сессии Niri.
    </div>
  {:else if outputs.length > 0}
    <div class="border border-kira-muted/20 bg-kira-surface-soft p-4">
      <p class="mb-3 text-[12px] font-bold text-kira-text">Расположение <span class="font-normal text-kira-muted">· перетащите экран</span></p>
      <div bind:this={mapViewport} class="kira-displays-map relative h-[230px] overflow-hidden bg-kira-bg"
        aria-label="Схема расположения мониторов">
        {#each rectangles as rect (rect.connector)}
          <button type="button"
            aria-label={'Переместить или выбрать ' + rect.connector}
            aria-pressed={selected === rect.connector}
            onclick={() => selected = rect.connector}
            onpointerdown={event => onPointerDown(event, rect.connector)}
            onpointermove={onPointerMove}
            onpointerup={onPointerUp}
            onpointercancel={onPointerUp}
            class={['absolute flex flex-col items-center justify-center gap-1 overflow-hidden border-2 text-center touch-none select-none focus-visible:outline-2 focus-visible:outline-kira-accent',
              selected === rect.connector ? 'border-kira-accent bg-kira-surface text-kira-text' : 'border-kira-muted/60 bg-kira-surface-soft text-kira-muted'].join(' ')}
            style:left={14 + (rect.x - minX) * zoom + 'px'}
            style:top={18 + (rect.y - minY) * zoom + 'px'}
            style:width={Math.max(38, rect.width * zoom) + 'px'}
            style:height={Math.max(32, rect.height * zoom) + 'px'}>
            <strong class="text-[12px] font-black">{outputs.findIndex(o => o.connector === rect.connector) + 1}</strong>
            <span class="max-w-full truncate px-1 text-[9px] font-semibold">{rect.connector}</span>
          </button>
        {/each}
        {#if rectangles.length === 0}
          <div class="p-4 text-[11px] text-kira-muted">Все дисплеи отключены</div>
        {/if}
      </div>
      <p class="mt-3 text-[10px] leading-relaxed text-kira-muted">
        Niri может скорректировать пересекающиеся экраны.
      </p>
    </div>

    <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
      {#each outputs as output, i (output.connector)}
        <button type="button" onclick={() => selected = output.connector}
          aria-pressed={selected === output.connector}
          class={['flex min-w-0 items-start gap-3 border-l-[3px] p-3 text-left focus-visible:outline-2 focus-visible:outline-kira-accent',
            selected === output.connector ? 'border-kira-accent bg-kira-surface text-kira-text' : 'border-kira-muted/25 bg-kira-bg text-kira-muted hover:bg-kira-surface-soft'].join(' ')}>
          <span class="grid size-7 shrink-0 place-items-center border border-kira-muted/40 text-[12px] font-black">{i + 1}</span>
          <span class="min-w-0">
            <strong class="block truncate text-[11px]">{output.make} {output.model}</strong>
            <span class="mt-1 block truncate text-[10px] text-kira-muted">{output.connector} ·
              {draft.find(d => d.connector === output.connector)?.enabled ? 'Включён' : 'Выключен'}
            </span>
          </span>
        </button>
      {/each}
    </div>

    {#if selectedOutput && selectedDraft}
      <div class="space-y-4 border-t border-kira-muted/20 pt-5">
        <div class="flex items-center justify-between gap-2">
          <h3 class="text-[14px] font-extrabold">Экран {outputs.indexOf(selectedOutput) + 1} · {selectedOutput.connector}</h3>
          <label class="flex shrink-0 items-center gap-2 text-[11px] font-bold">
            <input type="checkbox" checked={selectedDraft.enabled}
              disabled={busy || isPending || (selectedDraft.enabled && activeCount === 1)}
              onchange={event => change({ enabled: event.currentTarget.checked })}
              class="accent-kira-accent" />
            Включён
          </label>
        </div>
        <div class="grid gap-4 sm:grid-cols-2">
          <label class="block min-w-0 text-[11px] font-semibold text-kira-muted">Разрешение и частота
            <select class="mt-2 w-full border border-kira-muted/35 bg-kira-surface px-2 py-3 text-[12px] text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
              value={selectedDraft.mode} disabled={busy || isPending}
              onchange={event => change({ mode: event.currentTarget.value })}>
              {#each selectedOutput.modes as mode (displayModeId(mode))}
                <option value={displayModeId(mode)}>{mode.width} × {mode.height} · {(mode.refresh_rate / 1000).toFixed(2)} Гц{mode.is_preferred ? ' · рекоменд.' : ''}</option>
              {/each}
            </select>
          </label>
          <label class="block min-w-0 text-[11px] font-semibold text-kira-muted">Масштаб
            <select class="mt-2 w-full border border-kira-muted/35 bg-kira-surface px-2 py-3 text-[12px] text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
              value={String(selectedDraft.scale)} disabled={busy || isPending}
              onchange={event => change({ scale: Number(event.currentTarget.value) })}>
              {#each [...new Set([0.75, 1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4, selectedDraft.scale])].sort((a, b) => a - b) as value}
                <option value={String(value)}>{Math.round(value * 100)}%</option>
              {/each}
            </select>
          </label>
          <label class="block min-w-0 text-[11px] font-semibold text-kira-muted">Ориентация
            <select class="mt-2 w-full border border-kira-muted/35 bg-kira-surface px-2 py-3 text-[12px] text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
              value={selectedDraft.transform} disabled={busy || isPending}
              onchange={event => change({ transform: event.currentTarget.value })}>
              <option value="normal">Альбомная · 0°</option>
              <option value="90">Портретная · 90°</option>
              <option value="180">Альбомная · 180°</option>
              <option value="270">Портретная · 270°</option>
              <option value="flipped">Отражение</option>
              <option value="flipped-90">Отражение · 90°</option>
              <option value="flipped-180">Отражение · 180°</option>
              <option value="flipped-270">Отражение · 270°</option>
            </select>
          </label>
          <div class="text-[11px] font-semibold text-kira-muted">
            Положение · логические пиксели
            <div class="mt-2 grid grid-cols-2 gap-2">
              <label class="flex items-center gap-1 border border-kira-muted/35 bg-kira-surface px-2">
                X <input type="number" min="-32000" max="32000" step="1" value={selectedDraft.x}
                  disabled={busy || isPending}
                  onchange={event => change({ x: Math.round(Number(event.currentTarget.value)) })}
                  class="min-w-0 w-full bg-transparent px-1 py-3 text-[12px] text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent" />
              </label>
              <label class="flex items-center gap-1 border border-kira-muted/35 bg-kira-surface px-2">
                Y <input type="number" min="-32000" max="32000" step="1" value={selectedDraft.y}
                  disabled={busy || isPending}
                  onchange={event => change({ y: Math.round(Number(event.currentTarget.value)) })}
                  class="min-w-0 w-full bg-transparent px-1 py-3 text-[12px] text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent" />
              </label>
            </div>
          </div>
        </div>
        {#if selectedOutput.vrr_supported}
          <label class="flex items-center justify-between gap-3 border-t border-kira-muted/15 py-3 text-[11px]">
            <span><strong class="block">Адаптивная частота (VRR)</strong></span>
            <input type="checkbox" checked={selectedDraft.vrr} disabled={busy || isPending}
              onchange={event => change({ vrr: event.currentTarget.checked })} class="size-4 accent-kira-accent" />
          </label>
        {/if}
      </div>
    {/if}

    <div class="flex flex-wrap gap-2 border-t border-kira-muted/20 pt-4">
      <button type="button" onclick={apply} disabled={loading || busy || isPending || activeCount < 1}
        class="bg-kira-accent px-5 py-3 text-[11px] font-extrabold text-kira-on-accent focus-visible:outline-2 focus-visible:outline-kira-text disabled:opacity-40">
        {busy ? 'Применение…' : 'Применить'}
      </button>
      <button type="button" onclick={save} disabled={loading || busy || isPending || !canSave}
        title="Сохранить подтверждённые параметры в управляемый KDL"
        class="border border-kira-accent px-5 py-3 text-[11px] font-bold text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent disabled:opacity-40">
        Сохранить в конфиг
      </button>
      <button type="button" onclick={reset} disabled={loading || busy || isPending}
        class="border border-kira-muted/40 px-5 py-3 text-[11px] font-bold text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent disabled:opacity-40">
        Сбросить изменения
      </button>
    </div>
  {/if}
  <div class="border-t border-kira-muted/15 pt-4 text-[11px] leading-relaxed text-kira-muted">
    <p>Источник: <strong class="text-kira-text">{persistence === 'managed' ? 'Kira KDL'
      : persistence === 'uninitialized' ? 'Пустой слой Kira'
      : persistence === 'conflict' ? 'Конфликт KDL'
      : 'Декларативная конфигурация'}</strong>.
      Сначала примените и подтвердите настройки, затем сохраните их в конфиг.</p>
    <details class="mt-2">
      <summary class="cursor-pointer py-1 font-semibold text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">Как сохраняются настройки</summary>
      <div class="mt-2 space-y-2">
        <p>Kira создаёт отдельный <code>kira-managed-outputs.kdl</code> и включает его в
          доступный для записи <code>config.kdl</code>. Nix-managed файлы,
          чужие блоки <code>output</code> и ручные правки не перезаписываются.</p>
        <p>Зеркалирование экранов Niri не поддерживает.</p>
      </div>
    </details>
  </div>
</section>
