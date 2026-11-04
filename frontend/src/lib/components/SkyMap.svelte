<script lang="ts">
  import { onMount } from 'svelte';
  import { gsap } from 'gsap';
  import { surfaceColors, withOpacity } from '../surface-colors';
  import type { CivilDate } from '../calendar-model';
  import { isoDate } from '../calendar-model';
  import { CONSTELLATION_LABELS, CONSTELLATION_LINES, NAMED_STARS, STARS } from '../astronomy/sky-catalog';
  import { horizonPoint, localObservationDate, observerCoordinates, shortestSiderealDelta, siderealDegrees } from '../astronomy/sky-model';

  let { date }: { date: CivilDate } = $props();

  // An explicit, clearly marked example location; never pretend to know
  // the user's position or request WebKit geolocation permissions.
  let latitude = $state('51.4779');
  let longitude = $state('0');
  let time = $state('22:00');
  let viewport: HTMLDivElement;
  let canvas = $state<HTMLCanvasElement | null>(null);
  let dimensions = $state({ width: 0, height: 0 });
  const observer = $derived(observerCoordinates(latitude, longitude));
  const exampleLocation = $derived(observer?.latitude === 51.4779 && observer?.longitude === 0);
  const when = $derived(localObservationDate(date, time));

  const scene = { latitude: 51.4779, sidereal: 0 };
  let painted = false;
  let tween: gsap.core.Tween | null = null;
  let latestTarget: { latitude: number; sidereal: number } | null = null;
  let palette = surfaceColors();
  const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'локальный';

  function paint() {
    if (!canvas || !dimensions.width || !dimensions.height) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const { width, height } = dimensions;
    // Keep the backing bitmap bounded on HiDPI and ultrawide monitors.
    const pixelRatio = Math.min(window.devicePixelRatio || 1, 2, 1536 / Math.max(width, height));
    const bufferWidth = Math.round(width * pixelRatio);
    const bufferHeight = Math.round(height * pixelRatio);
    if (canvas.width !== bufferWidth || canvas.height !== bufferHeight) {
      canvas.width = bufferWidth;
      canvas.height = bufferHeight;
    }
    ctx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const x = width / 2;
    const y = height / 2;
    const radius = Math.max(1, Math.min(width - 64, height - 58) / 2);
    const position = (ra: number, dec: number) => horizonPoint(ra, dec, scene.latitude, scene.sidereal);

    // Horizon is the true clipping boundary of the 2D azimuthal projection.
    ctx.save();
    ctx.translate(x, y);
    ctx.beginPath();
    ctx.arc(0, 0, radius, 0, Math.PI * 2);
    ctx.clip();
    ctx.fillStyle = palette.background;
    ctx.fillRect(-radius, -radius, radius * 2, radius * 2);
    ctx.strokeStyle = withOpacity(palette.muted, 0.15);
    ctx.lineWidth = 1;
    for (const alt of [30, 60]) {
      ctx.beginPath();
      ctx.arc(0, 0, radius * (90 - alt) / 90, 0, Math.PI * 2);
      ctx.stroke();
    }
    ctx.strokeStyle = withOpacity(palette.muted, 0.09);
    for (const angle of [0, Math.PI / 2]) {
      ctx.beginPath();
      ctx.moveTo(-radius * Math.cos(angle), -radius * Math.sin(angle));
      ctx.lineTo(radius * Math.cos(angle), radius * Math.sin(angle));
      ctx.stroke();
    }

    // Illustrated connect-the-dots patterns. Sample along each RA/Dec
    // segment to avoid connecting unrelated visible pieces over the horizon.
    ctx.lineWidth = 0.85;
    ctx.strokeStyle = withOpacity(palette.accent, 0.48);
    for (const [, chains] of CONSTELLATION_LINES) {
      for (const chain of chains) {
        ctx.beginPath();
        let active = false;
        for (let i = 1; i < chain.length; i++) {
          const from = chain[i - 1];
          const to = chain[i];
          let deltaRa = to[0] - from[0];
          if (deltaRa > 180) deltaRa -= 360;
          if (deltaRa < -180) deltaRa += 360;
          for (let step = i === 1 ? 0 : 1; step <= 8; step++) {
            const fraction = step / 8;
            const pt = position(from[0] + deltaRa * fraction, from[1] + (to[1] - from[1]) * fraction);
            if (pt.altitude < 0) {
              active = false;
            } else if (!active) {
              ctx.moveTo(pt.x * radius, pt.y * radius);
              active = true;
            } else {
              ctx.lineTo(pt.x * radius, pt.y * radius);
            }
          }
        }
        ctx.stroke();
      }
    }

    for (const [ra, dec, magnitude] of STARS) {
      const star = position(ra, dec);
      if (star.altitude < 0) continue;
      const r = magnitude < 1.4 ? 2.6 : magnitude < 2.6 ? 1.8 : magnitude < 4 ? 1.2 : 0.72;
      ctx.beginPath();
      ctx.arc(star.x * radius, star.y * radius, r, 0, 2 * Math.PI);
      ctx.fillStyle = magnitude < 2 ? palette.text : withOpacity(palette.muted, 0.78);
      ctx.fill();
    }

    // Prevent dense star/constellation labels from obscuring the actual sky.
    const occupied: { x: number; y: number }[] = [];
    ctx.font = '10px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (const [name, ra, dec] of CONSTELLATION_LABELS) {
      const pt = position(ra, dec);
      if (pt.altitude < 12) continue;
      const px = pt.x * radius;
      const py = pt.y * radius;
      if (occupied.some(other => Math.hypot(other.x - px, other.y - py) < 65)) continue;
      occupied.push({ x: px, y: py });
      ctx.fillStyle = withOpacity(palette.accent, 0.80);
      ctx.fillText(name, px, py);
    }
    ctx.font = '10px system-ui, sans-serif';
    ctx.textAlign = 'left';
    for (const [ra, dec, name] of NAMED_STARS) {
      const pt = position(ra, dec);
      if (pt.altitude < 7) continue;
      const px = pt.x * radius;
      const py = pt.y * radius;
      if (occupied.some(other => Math.hypot(other.x - px, other.y - py) < 40)) continue;
      occupied.push({ x: px, y: py });
      ctx.fillStyle = withOpacity(palette.muted, 0.90);
      ctx.fillText(name, px + 5, py - 6);
    }
    ctx.restore();

    ctx.strokeStyle = withOpacity(palette.muted, 0.35);
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, 2 * Math.PI);
    ctx.stroke();
    ctx.font = 'bold 11px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillStyle = palette.accent;
    for (const [label, dx, dy] of [
      ['N', 0, -1], ['E', 1, 0], ['S', 0, 1], ['W', -1, 0],
    ] as const) {
      ctx.fillText(label, x + dx * (radius + 16), y + dy * (radius + 16));
    }
  }

  onMount(() => {
    // Resolve CSS roles only at mount/theme changes, not on every GSAP paint frame.
    palette = surfaceColors(viewport);
    const theme = new MutationObserver(() => {
      palette = surfaceColors(viewport);
      paint();
    });
    theme.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'style'] });
    const resize = new ResizeObserver(entries => {
      const box = entries[0]?.contentRect;
      if (box) dimensions = { width: Math.round(box.width), height: Math.round(box.height) };
    });
    resize.observe(viewport);
    const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
    const settleForReducedMotion = () => {
      if (!motion.matches || !latestTarget) return;
      tween?.kill();
      tween = null;
      scene.latitude = latestTarget.latitude;
      scene.sidereal = latestTarget.sidereal;
      paint();
    };
    motion.addEventListener('change', settleForReducedMotion);
    return () => {
      theme.disconnect();
      resize.disconnect();
      motion.removeEventListener('change', settleForReducedMotion);
      tween?.kill();
    };
  });

  $effect(() => {
    const targetObserver = observer;
    const dateTime = when;
    const size = dimensions;
    if (!targetObserver || !dateTime || !size.width || !size.height || !canvas) return;
    const targetSidereal = siderealDegrees(dateTime, targetObserver.longitude);
    latestTarget = { latitude: targetObserver.latitude, sidereal: targetSidereal };
    tween?.kill();
    // Same star field follows a continuous forward trajectory when time
    // changes. Interrupted transitions retarget from the actual last frame.
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (!painted || reduced) {
      scene.latitude = targetObserver.latitude;
      scene.sidereal = targetSidereal;
      painted = true;
      paint();
    } else {
      tween = gsap.to(scene, {
        latitude: targetObserver.latitude,
        sidereal: scene.sidereal + shortestSiderealDelta(scene.sidereal, targetSidereal),
        duration: 0.42,
        ease: 'power3.out',
        overwrite: 'auto',
        onUpdate: paint,
        onComplete: paint,
      });
    }
    return () => tween?.kill();
  });
</script>

<section class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto bg-kira-bg max-[760px]:min-h-[640px] max-[760px]:w-full max-[760px]:flex-none max-[760px]:overflow-visible"
  aria-label="Карта звёздного неба" data-kira-sky-map>
  <!-- Editorial Black hero, Prussian Blue workbench, Black factual footer. -->
  <header class="shrink-0 bg-kira-bg px-6 pb-4 pt-5">
    <div class="border-l-2 border-kira-accent pl-3">
      <h2 class="text-[22px] font-extrabold leading-tight tracking-tight">Звёздное небо</h2>
    </div>
  </header>

  <div class="flex min-h-[395px] flex-1 flex-col bg-kira-surface px-5 pb-4 pt-4">
    <div class="shrink-0 border-b border-kira-muted/20 pb-4">
      <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-1">
        <p class="text-[11px] font-bold tracking-wide text-kira-muted">ТОЧКА НАБЛЮДЕНИЯ</p>
        {#if exampleLocation}
          <p class="text-[11px] font-semibold text-kira-accent">ДЕМО · ГРИНВИЧ</p>
        {/if}
      </div>
      <div class="mt-3 flex flex-wrap items-end gap-x-3 gap-y-3">
        <label class="flex min-w-[100px] flex-1 flex-col gap-1.5 text-[11px] font-semibold text-kira-muted">
          Широта, °
          <input class="w-full min-w-0 border border-kira-muted/30 bg-kira-bg px-3 py-2 text-[13px] tabular-nums text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
            aria-label="Широта наблюдателя" type="text" inputmode="decimal" bind:value={latitude} />
        </label>
        <label class="flex min-w-[100px] flex-1 flex-col gap-1.5 text-[11px] font-semibold text-kira-muted">
          Долгота, ° E
          <input class="w-full min-w-0 border border-kira-muted/30 bg-kira-bg px-3 py-2 text-[13px] tabular-nums text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
            aria-label="Долгота наблюдателя" type="text" inputmode="decimal" bind:value={longitude} />
        </label>
        <label class="flex min-w-[105px] flex-1 flex-col gap-1.5 text-[11px] font-semibold text-kira-muted">
          Время
          <input class="w-full min-w-0 border border-kira-muted/30 bg-kira-bg px-3 py-2 text-[13px] tabular-nums text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
            aria-label="Время наблюдения" type="time" bind:value={time} />
        </label>
        <button type="button" onclick={() => { latitude = '51.4779'; longitude = '0'; }}
          class="min-h-9 shrink-0 border border-kira-muted/30 px-3 py-2 text-[11px] font-bold tracking-wide text-kira-accent hover:border-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent"
          aria-label="Вернуть демонстрационные координаты Гринвича">
          СБРОС
        </button>
      </div>
      <p class="mt-3 text-[11px] leading-relaxed text-kira-muted">
        Часовой пояс: <span class="font-semibold text-kira-text">{timeZone}</span> · Север и восток: +.
      </p>
    </div>

    <div bind:this={viewport} class="relative mt-4 min-h-[270px] flex-1 overflow-hidden bg-kira-bg">
      {#if observer && when}
        <div class="h-full w-full" role="img"
          aria-label={'Карта созвездий над горизонтом ' + isoDate(date) + ' в ' + time + ', широта ' + observer.latitude + ', долгота ' + observer.longitude}>
          <canvas bind:this={canvas} class="block h-full w-full" aria-hidden="true"></canvas>
        </div>
      {:else}
        <div class="flex h-full items-center justify-center px-8 text-center text-[12px] text-kira-muted" role="status">
          Проверьте координаты (широта ±90°, долгота ±180°), время и год (1900–2100).
        </div>
      {/if}
    </div>
    <p class="mt-3 text-[11px] text-kira-muted">Звёзды до 5,35 mag</p>
  </div>

  <footer class="shrink-0 bg-kira-bg px-6 pb-4 pt-3 text-[11px] leading-relaxed text-kira-muted">
    XHIP / Hipparcos · схемы созвездий d3-celestial · J2000. Приближённая карта, не для навигации.
  </footer>
</section>
