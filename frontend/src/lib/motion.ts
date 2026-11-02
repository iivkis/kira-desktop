/** GSAP is Kira Desktop's only JavaScript motion engine.
 * Shared entrances are triggered when the native GTK surface becomes
 * VISIBLE, never when Svelte mounts into an opacity-zero WebKit window.
 * A real FORWARD content assembly is primary; subtle INVERTED traces
 * replay saved paths to the same physical editorial contact point.
 */
import { gsap } from 'gsap';
import { tenetFragmentsFor } from './calendar-tenet-model';

const reducedMotion = () => typeof window !== 'undefined' &&
  window.matchMedia('(prefers-reduced-motion: reduce)').matches;

export interface TenetEntrance {
  contact: string;
  direction?: 1 | -1;
  sections?: number;
}

export function tenetWidgetReveal(node: HTMLElement, options: TenetEntrance) {
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  // Cap moving surfaces: the system monitor's many nested charts/rows
  // must not compete for compositor layers and trigger choppy frames.
  const content = Array.from(node.querySelectorAll<HTMLElement>('[data-kira-enter]'))
    .slice(0, options.sections ?? 3);
  let master: gsap.core.Timeline | null = null;
  let traces: HTMLElement[] = [];
  let restore: number | null = null;
  let awaitVisible: number | null = null;
  let started = false;

  function settle() {
    if (restore !== null) window.clearTimeout(restore);
    if (awaitVisible !== null) window.clearTimeout(awaitVisible);
    restore = null;
    awaitVisible = null;
    master?.kill();
    master = null;
    for (const trace of traces) trace.remove();
    traces = [];
    gsap.killTweensOf(content);
    if (content.length) {
      gsap.set(content, { clearProps: 'transform,opacity,willChange' });
    }
    node.dataset.tenetReveal = 'settled';
  }

  function begin() {
    if (started || preference.matches) return;
    started = true;
    if (awaitVisible !== null) window.clearTimeout(awaitVisible);
    awaitVisible = null;
    const contact = node.querySelector<HTMLElement>(options.contact);
    if (!contact || !content.length) {
      settle();
      return;
    }
    const direction = options.direction ?? 1;
    const rootRect = node.getBoundingClientRect();
    const contactRect = contact.getBoundingClientRect();
    const origin = {
      x: contactRect.left - rootRect.left + contactRect.width / 2,
      y: contactRect.top - rootRect.top + contactRect.height / 2,
    };

    // Memory is fixed for this one event. Reversing a power3.out path
    // is power3.in on the SAME coordinates, with the order reversed.
    // One master GSAP timeline lets WebKit render both tracks together
    // without an imperative second seek on every animation frame.
    const flights = tenetFragmentsFor(direction);
    traces = flights.map((_, index) => {
      const trace = node.ownerDocument.createElement('span');
      trace.dataset.tenetFragment = 'inverted';
      trace.setAttribute('aria-hidden', 'true');
      trace.style.cssText = [
        'position:absolute', 'pointer-events:none', 'z-index:2',
        'width:12px', 'height:1px', 'opacity:0',
        'transform-origin:center center',
        'left:' + origin.x + 'px', 'top:' + origin.y + 'px',
        'background:' + (index % 2 ? '#e5e5e5' : '#fca311'),
      ].join(';');
      node.appendChild(trace);
      return trace;
    });

    master = gsap.timeline({ paused: true, onComplete: settle });
    master.to(content, {
      y: 0,
      scale: 1,
      duration: 0.44,
      stagger: 0.035,
      ease: 'back.out(1.22)',
      force3D: true,
      overwrite: 'auto',
    }, 0);
    // Fade is kept separate from the elastic transform: no opacity overshoot.
    master.to(content, { opacity: 1, duration: 0.22, stagger: 0.035, ease: 'power2.out' }, 0);
    master.fromTo(traces,
      {
        x: (i: number) => flights[i].x,
        y: (i: number) => flights[i].y,
        rotation: (i: number) => flights[i].rotation,
        opacity: 0.13,
      },
      {
        x: 0, y: 0, rotation: 0, opacity: 0,
        duration: 0.44,
        stagger: { each: 0.012, from: 'end' },
        ease: 'power3.in',
        force3D: true,
        immediateRender: true,
      }, 0);
    node.dataset.tenetReveal = 'started';
    master.play(0);
    // WebKitGTK may throttle compositor frames while Niri maps a window.
    // Never leave foreground controls transformed after a stalled timer.
    restore = window.setTimeout(settle, 1200);
  }

  function isCurrentGeneration(event: Event): boolean {
    const generation = (event as CustomEvent<{ generation?: unknown }>).detail?.generation;
    return Number.isSafeInteger(generation) && generation === Number(node.dataset.kiraGeneration);
  }
  function onVisible(event: Event) {
    if (isCurrentGeneration(event)) begin();
  }
  function onHide(event: Event) {
    if (isCurrentGeneration(event)) {
      started = true;
      settle();
    }
  }
  function onPreference() {
    if (preference.matches) {
      started = true;
      settle();
    }
  }

  // Prepare the visible geometry while the native window is still
  // transparent. The GTK visible handshake starts the tween from these
  // exact values, eliminating the old full-opacity -> 0.18 snap.
  // Only two or three small sections move. The live chart layers remain still.
  if (preference.matches || !content.length) {
    node.dataset.tenetReveal = 'settled';
  } else {
    gsap.set(content, {
      y: 9,
      scale: 0.988,
      opacity: 0.94,
      force3D: true,
      willChange: 'transform, opacity',
    });
    node.dataset.tenetReveal = 'ready';
    // Older hosts never send the event. Restore the true DOM promptly;
    // never strand an inactive panel in a dimmed/transformed state.
    awaitVisible = window.setTimeout(() => {
      started = true;
      settle();
    }, 550);
  }

  window.addEventListener('kira:overlay:visible', onVisible);
  window.addEventListener('kira:overlay:hide', onHide);
  preference.addEventListener('change', onPreference);
  return {
    destroy() {
      window.removeEventListener('kira:overlay:visible', onVisible);
      window.removeEventListener('kira:overlay:hide', onHide);
      preference.removeEventListener('change', onPreference);
      settle();
    },
  };
}

/** One real indicator moves between the measured tab geometries, including
 * interrupted selections. The truthful active state updates in Svelte first.
 */
export function tenetFlowIndicator(node: HTMLElement, initial: { active: string }) {
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  const indicator = node.querySelector<HTMLElement>('[data-kira-flow-indicator]');
  let active = initial.active;
  if (!indicator) return { update(_next: { active: string }) {}, destroy() {} };

  function relocate(next: string, animate: boolean) {
    const button = Array.from(node.querySelectorAll<HTMLElement>('[data-kira-tab]'))
      .find(item => item.dataset.kiraTab === next);
    if (!button) return;
    const x = button.offsetLeft;
    const width = button.offsetWidth;
    if (!Number.isFinite(x) || width <= 0) return;
    if (!animate || preference.matches) {
      gsap.killTweensOf(indicator);
      gsap.set(indicator, { x, width });
      return;
    }
    gsap.to(indicator, {
      x, width, duration: 0.42, ease: 'back.out(1.18)',
      overwrite: 'auto', force3D: true,
    });
  }

  relocate(active, false);
  const onPreference = () => { if (preference.matches) relocate(active, false); };
  preference.addEventListener('change', onPreference);
  const observer = typeof ResizeObserver === 'undefined' ? null :
    new ResizeObserver(() => relocate(active, false));
  observer?.observe(node);

  return {
    update(next: { active: string }) {
      if (next.active === active) return;
      active = next.active;
      relocate(active, true);
    },
    destroy() {
      observer?.disconnect();
      preference.removeEventListener('change', onPreference);
      gsap.killTweensOf(indicator);
      gsap.set(indicator, { clearProps: 'transform,width,willChange' });
    },
  };
}

/** Soft panel arrival on tab changes; the initial native mount is excluded. */
export function tenetPanelArrival(node: HTMLElement) {
  const root = node.closest<HTMLElement>('[data-kira-ui-root]');
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  if (!root?.dataset.kiraGeneration || preference.matches) return { destroy() {} };
  let tween: gsap.core.Tween | null = null;
  const settle = () => {
    tween?.kill();
    tween = null;
    gsap.set(node, { clearProps: 'transform,opacity,visibility,willChange' });
  };
  gsap.set(node, { willChange: 'transform, opacity' });
  tween = gsap.fromTo(node, { y: 6, autoAlpha: 0.94 },
    { y: 0, autoAlpha: 1, duration: 0.30, ease: 'power3.out', onComplete: settle });
  const onPreference = () => { if (preference.matches) settle(); };
  preference.addEventListener('change', onPreference);
  return {
    destroy() {
      preference.removeEventListener('change', onPreference);
      settle();
    },
  };
}

/** Flash a new numeric reading once without competing with ECharts updates. */
export function pulseReading(node: HTMLElement, initial: string) {
  let previous = initial;
  return {
    update(next: string) {
      if (next === previous) return;
      previous = next;
      if (reducedMotion()) return;
      gsap.killTweensOf(node);
      gsap.fromTo(node,
        { color: '#fca311' },
        { color: '#ffffff', duration: 0.6, ease: 'power2.out', clearProps: 'color' },
      );
    },
    destroy() {
      gsap.killTweensOf(node);
      gsap.set(node, { clearProps: 'color' });
    },
  };
}
