/** GSAP is Kira Desktop's only JavaScript motion engine.
 * Shared entrances are triggered when the native GTK surface becomes
 * VISIBLE, never when Svelte mounts into an opacity-zero WebKit window.
 * A real FORWARD content assembly is primary; subtle INVERTED traces
 * replay saved paths to the same physical editorial contact point.
 */
import { gsap } from 'gsap';
import { dualTime, tenetFragmentsFor } from './calendar-tenet-model';

const reducedMotion = () => typeof window !== 'undefined' &&
  window.matchMedia('(prefers-reduced-motion: reduce)').matches;

export interface TenetEntrance {
  contact: string;
  direction?: 1 | -1;
}

export function tenetWidgetReveal(node: HTMLElement, options: TenetEntrance) {
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  let forward: gsap.core.Timeline | null = null;
  let inverted: gsap.core.Timeline | null = null;
  let traces: HTMLElement[] = [];
  let restore: number | null = null;
  let started = false;

  const targets = () => Array.from(node.querySelectorAll<HTMLElement>('[data-kira-enter]'));

  function settle() {
    if (restore !== null) {
      window.clearTimeout(restore);
      restore = null;
    }
    forward?.kill();
    inverted?.kill();
    forward = null;
    inverted = null;
    for (const trace of traces) trace.remove();
    traces = [];
    const content = targets();
    gsap.killTweensOf(content);
    if (content.length) gsap.set(content, { clearProps: 'transform,opacity,visibility' });
    node.dataset.tenetReveal = 'settled';
  }

  function begin() {
    if (started || preference.matches) {
      node.dataset.tenetReveal = 'settled';
      return;
    }
    started = true;
    const contact = node.querySelector<HTMLElement>(options.contact);
    const content = targets();
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
    // One immutable trajectory memory for the entire entrance event.
    const flights = tenetFragmentsFor(direction);
    traces = flights.map((_, index) => {
      const trace = node.ownerDocument.createElement('span');
      trace.dataset.tenetFragment = 'inverted';
      trace.setAttribute('aria-hidden', 'true');
      // Hairline fragments support the composition; they are no longer
      // four conspicuous orange/white squares dominating the animation.
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

    inverted = gsap.timeline({ paused: true });
    inverted.to(traces, {
      x: (i: number) => flights[i].x,
      y: (i: number) => flights[i].y,
      rotation: (i: number) => flights[i].rotation,
      opacity: 0.32,
      duration: 0.88,
      stagger: 0.025,
      ease: 'power3.out',
    }, 0);
    inverted.progress(1);

    const stagger = Math.min(0.11, 0.25 / Math.max(1, content.length - 1));
    forward = gsap.timeline({
      paused: true,
      onUpdate: () => {
        // The inverse replays the ORIGINAL easing and rotations,
        // synchronized with the progression of the visible content.
        inverted?.progress(dualTime(forward?.progress() ?? 1).inverted);
      },
      onComplete: settle,
    });
    forward.fromTo(content,
      { opacity: 0.18, x: 16 * direction, y: 28, scale: 0.975 },
      {
        opacity: 1, x: 0, y: 0, scale: 1,
        duration: 0.84, stagger, ease: 'power3.out',
      }, 0);

    node.dataset.tenetReveal = 'started';
    forward.play(0);
    // Recovery for throttled WebKit frames; never leave content faded out.
    restore = window.setTimeout(settle, 1550);
  }

  function isCurrentGeneration(event: Event): boolean {
    const generation = (event as CustomEvent<{ generation?: unknown }>).detail?.generation;
    return Number.isSafeInteger(generation) && generation === Number(node.dataset.kiraGeneration);
  }

  function onVisible(event: Event) {
    if (!isCurrentGeneration(event) || started || preference.matches) return;
    begin();
  }

  function onHide(event: Event) {
    if (isCurrentGeneration(event)) settle();
  }

  function onPreference() {
    if (preference.matches) settle();
  }

  // Never hide controls before the window is mapped. On legacy hosts
  // without the visibility event, content remains fully usable and static.
  node.dataset.tenetReveal = preference.matches ? 'settled' : 'ready';
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
