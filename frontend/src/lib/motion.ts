/** GSAP is the project's sole JavaScript motion engine.
 * Actions are scoped to the short-lived Svelte widget DOM. They never animate
 * GTK/Wayland windows, and reduced-motion is a functional no-animation path.
 */
import { gsap } from 'gsap';
import { dualTime, tenetFragmentsFor } from './calendar-tenet-model';

const reducedMotion = () => typeof window !== 'undefined' &&
  window.matchMedia('(prefers-reduced-motion: reduce)').matches;

/** A widget mount is one recorded TENET event. FORWARD sections arrive
 * while INVERTED traces replay the same outbound physical path backward
 * to the real header/clock contact, from one synchronized master cursor.
 */
export interface TenetEntrance {
  contact: string;
  direction?: 1 | -1;
}

export function tenetWidgetReveal(node: HTMLElement, options: TenetEntrance) {
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  const targets = Array.from(node.querySelectorAll<HTMLElement>('[data-kira-enter]'));
  const contact = node.querySelector<HTMLElement>(options.contact);
  const direction = options.direction ?? 1;
  let forward: gsap.core.Timeline | null = null;
  let inverted: gsap.core.Timeline | null = null;
  let traces: HTMLElement[] = [];
  let restore: number | null = null;

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
    gsap.killTweensOf(targets);
    if (targets.length) gsap.set(targets, { clearProps: 'transform,opacity,visibility' });
    node.dataset.tenetReveal = 'settled';
  }

  const onPreference = () => { if (preference.matches) settle(); };
  preference.addEventListener('change', onPreference);
  if (!preference.matches && contact && targets.length) {
    const rootRect = node.getBoundingClientRect();
    const contactRect = contact.getBoundingClientRect();
    const origin = {
      x: contactRect.left - rootRect.left + contactRect.width / 2,
      y: contactRect.top - rootRect.top + contactRect.height / 2,
    };
    const flights = tenetFragmentsFor(direction);
    traces = flights.map((_, index) => {
      const trace = node.ownerDocument.createElement('span');
      trace.dataset.tenetFragment = 'inverted';
      trace.setAttribute('aria-hidden', 'true');
      trace.style.cssText = [
        'position:absolute', 'pointer-events:none', 'z-index:2',
        'width:3px', 'height:3px', 'opacity:0',
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
      opacity: 0.76, scale: 1, duration: 0.68,
      stagger: 0.024, ease: 'power3.out',
    }, 0);
    inverted.progress(1); // Consequence is visible before contact.
    forward = gsap.timeline({
      paused: true,
      onUpdate: () => {
        // Reverse recorded chronology, not the coordinates or CSS direction.
        inverted?.progress(dualTime(forward?.progress() ?? 1).inverted);
      },
      onComplete: settle,
    });
    forward.fromTo(targets,
      { autoAlpha: 0, x: 8 * direction, y: 7 },
      {
        autoAlpha: 1, x: 0, y: 0, duration: 0.54,
        stagger: 0.045, ease: 'power3.out',
      }, 0);
    node.dataset.tenetReveal = 'started';
    forward.play(0);
    // GTK may map WebKit while animation frames are throttled.
    restore = window.setTimeout(settle, 1100);
  } else {
    node.dataset.tenetReveal = 'settled';
  }
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

