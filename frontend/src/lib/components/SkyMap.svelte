<script lang="ts">
  import { onMount } from 'svelte';
  import { gsap } from 'gsap';
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
  let canvas: HTMLCanvasElement;
  let dimensions = $state({ width: 0, height: 0 });
  const observer = $derived(observerCoordinates(latitude, longitude));
  const when = $derived(localObservationDate(date, time));

  const scene = { latitude: 51.4779, sidereal: 0 };
  let painted = false;
  let tween: gsap.core.Tween | null = null;

  function paint() {
    if (!canvas || !dimensions.width || !dimensions.height) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const { width, height } = dimensions;
    const pixelRatio = Math.min(window.devicePixelRatio || 1, 2);
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
    ctx.fillStyle = '#000000';
    ctx.fillRect(-radius, -radius, radius * 2, radius * 2);
    ctx.strokeStyle = 'rgba(229,229,229,0.15)';
    ctx.lineWidth = 1;
    for (const alt of [30, 60]) {
      ctx.beginPath();
      ctx.arc(0, 0, radius * (90 - alt) / 90, 0, Math.PI * 2);
      ctx.stroke();
    }
    ctx.strokeStyle = 'rgba(229,229,229,0.09)';
    for (const angle of [0, Math.PI / 2]) {
      ctx.beginPath();
      ctx.moveTo(-radius * Math.cos(angle), -radius * Math.sin(angle));
      ctx.lineTo(radius * Math.cos(angle), radius * Math.sin(angle));
      ctx.stroke();
    }

    // Illustrated connect-the-dots patterns. Sample along each RA/Dec
    // segment to avoid connecting unrelated visible pieces over the horizon.
    ctx.lineWidth = 0.85;
    ctx.strokeStyle = 'rgba(252,163,17,0.48)';
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
      ctx.fillStyle = magnitude < 2 ? '#ffffff' : 'rgba(229,229,229,0.78)';
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
      ctx.fillStyle = 'rgba(252,163,17,0.80)';
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
      ctx.fillStyle = 'rgba(229,229,229,0.90)';
      ctx.fillText(name, px + 5, py - 6);
    }
    ctx.restore();

    ctx.strokeStyle = 'rgba(229,229,229,0.35)';
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, 2 * Math.PI);
    ctx.stroke();
    ctx.font = 'bold 11px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillStyle = '#fca311';
    for (const [label, dx, dy] of [
      ['N', 0, -1], ['E', 1, 0], ['S', 0, 1], ['W', -1, 0],
    ] as const) {
      ctx.fillText(label, x + dx * (radius + 16), y + dy * (radius + 16));
    }
  }

  onMount(() => {
    const resize = new ResizeObserver(entries => {
      const box = entries[0]?.contentRect;
      if (box) dimensions = { width: Math.round(box.width), height: Math.round(box.height) };
    });
    resize.observe(viewport);
    return () => {
      resize.disconnect();
      tween?.kill();
    };
  });

  $effect(() => {
    const targetObserver = observer;
    const dateTime = when;
    const size = dimensions;
    if (!targetObserver || !dateTime || !size.width || !size.height || !canvas) return;
    const targetSidereal = siderealDegrees(dateTime, targetObserver.longitude);
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

<section class="flex min-h-0 min-w-0 flex-1 flex-col bg-kira-bg p-5" aria-label="Карта звёздного неба" data-kira-sky-map>
  <header class="shrink-0 border-l-2 border-kira-accent pl-3">
    <p class="text-[9px] font-extrabold tracking-[0.17em] text-kira-accent">04 / CELESTIAL ATLAS</p>
    <h2 class="mt-1 text-[20px] font-extrabold tracking-tight">Звёздное небо</h2>
    <p class="mt-1 text-[11px] text-kira-muted">Над горизонтом · {isoDate(date)} · время системы</p>
  </header>

  <div class="mt-4 flex flex-wrap items-end gap-3 border-y border-kira-muted/15 py-3">
    <label class="flex flex-col gap-1 text-[10px] font-semibold text-kira-muted">
      Широта, °
      <input class="w-[95px] border border-kira-muted/25 bg-kira-surface px-2 py-2 text-[12px] text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
        aria-label="Широта наблюдателя" type="text" inputmode="decimal" bind:value={latitude} />
    </label>
    <label class="flex flex-col gap-1 text-[10px] font-semibold text-kira-muted">
      Долгота, ° E
      <input class="w-[95px] border border-kira-muted/25 bg-kira-surface px-2 py-2 text-[12px] text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
        aria-label="Долгота наблюдателя" type="text" inputmode="decimal" bind:value={longitude} />
    </label>
    <label class="flex flex-col gap-1 text-[10px] font-semibold text-kira-muted">
      Время системы
      <input class="w-[104px] border border-kira-muted/25 bg-kira-surface px-2 py-2 text-[12px] text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
        aria-label="Время наблюдения" type="time" bind:value={time} />
    </label>
    <button type="button" onclick={() => { latitude = '51.4779'; longitude = '0'; }}
      class="border border-kira-muted/25 px-2 py-2 text-[10px] font-bold text-kira-accent hover:border-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">
      ГРИНВИЧ
    </button>
  </div>

  <p class="mt-2 text-[10px] text-kira-muted">
    Пример: Гринвич, не геолокация. Время соответствует часовому поясу компьютера. Широта: север +; долгота: восток +.
  </p>

  <div bind:this={viewport} class="relative mt-3 min-h-[270px] flex-1 overflow-hidden border border-kira-muted/15 bg-kira-bg">
    {#if observer && when}
      <canvas bind:this={canvas} class="block h-full w-full" role="img"
        aria-label={'Карта созвездий над горизонтом ' + isoDate(date) + ' в ' + time + ', широта ' + observer.latitude + ', долгота ' + observer.longitude}></canvas>
    {:else}
      <div class="flex h-full items-center justify-center px-8 text-center text-[12px] text-kira-muted" role="status">
        Проверьте координаты (широта ±90°, долгота ±180°), время и год (1900–2100).
      </div>
    {/if}
  </div>

  <footer class="mt-3 shrink-0 space-y-1 text-[10px] leading-relaxed text-kira-muted">
    <p>Звёзды до 5,35 mag · созвездия · север вверху · восток справа.</p>
    <p>Каталог XHIP/Hipparcos; линии d3-celestial. Проекция и вращение приближённые (J2000). Не для точной навигации.</p>
  </footer>
</section>
