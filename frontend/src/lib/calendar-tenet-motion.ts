/** TENET Motion System for the Calendar.
 *
 * A single interaction owns a remembered scene. FORWARD elements settle
 * normally while INVERTED fragments replay one pre-recorded outward flight
 * backward towards the SAME contact point. All scenes are cancellable and
 * leave no transforms, fragments, timers or RAF loops behind.
 */
import { gsap } from 'gsap';
import { nextClockFlights, type ClockFlight } from './clock-flight';
import { dualTime, tenetFragmentsFor, type FragmentFlight } from './calendar-tenet-model';

type Scene = { forward: gsap.core.Timeline; inverted: gsap.core.Timeline; fragments: HTMLElement[] };
const ACCENT = '#fca311';
const MUTED = '#e5e5e5';

function reduced(motion: MediaQueryList): boolean { return motion.matches; }

function createFragments(root: HTMLElement, contact: HTMLElement, positions: readonly FragmentFlight[]): HTMLElement[] {
  const base = root.getBoundingClientRect();
  const target = contact.getBoundingClientRect();
  const x = target.left - base.left + target.width / 2;
  const y = target.top - base.top + target.height / 2;
  return positions.map((_, i) => {
    const fragment = root.ownerDocument.createElement('span');
    fragment.dataset.tenetFragment = 'inverted';
    fragment.setAttribute('aria-hidden', 'true');
    fragment.style.cssText = [
      'position:absolute', 'pointer-events:none', 'z-index:1', 'width:4px', 'height:4px',
      `left:${x}px`, `top:${y}px`, `background:${i % 2 ? MUTED : ACCENT}`,
      'opacity:0', 'transform-origin:center center',
    ].join(';');
    root.appendChild(fragment);
    return fragment;
  });
}

/** Scene master timeline advances forward content and reverses the exact
 * same recorded fragment keyframes. Both streams meet at the contact point.
 */
function dualScene(
  root: HTMLElement,
  contact: HTMLElement,
  targets: readonly HTMLElement[],
  direction: number,
  cleanup: () => void,
): Scene {
  const positions = tenetFragmentsFor(direction);
  const fragments = createFragments(root, contact, positions);
  gsap.set(fragments, { x: 0, y: 0, rotation: 0, scale: 0, opacity: 0 });
  const inverted = gsap.timeline({ paused: true });
  inverted.to(fragments, {
    x: (i: number) => positions[i].x,
    y: (i: number) => positions[i].y,
    rotation: (i: number) => positions[i].rotation,
    scale: 1,
    opacity: 0.85,
    duration: 0.52,
    stagger: 0.025,
    ease: 'power3.out',
  }, 0);
  inverted.progress(1); // Future trace exists first; then returns to contact.
  const forward = gsap.timeline({
    paused: true,
    onUpdate: () => { inverted.progress(dualTime(forward.progress()).inverted); },
    onComplete: cleanup,
  });
  forward.fromTo(targets,
    { autoAlpha: 0, x: 10 * direction, y: 5, scale: 0.985 },
    {
      autoAlpha: 1, x: 0, y: 0, scale: 1, duration: 0.43, stagger: 0.005,
      ease: 'power2.out',
    },
    0);
  forward.play(0);
  return { forward, inverted, fragments };
}

export function tenetCalendarReveal(node: HTMLElement) {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const targets = Array.from(node.querySelectorAll<HTMLElement>('[data-kira-enter]'));
  const clock = node.querySelector<HTMLElement>('[data-calendar-clock-play]');
  let scene: Scene | null = null;

  function stop() {
    const old = scene;
    scene = null;
    old?.forward.kill();
    old?.inverted.kill();
    old?.fragments.forEach(fragment => fragment.remove());
    gsap.killTweensOf(targets);
    gsap.set(targets, { clearProps: 'opacity,visibility,transform' });
  }
  const onMotionChange = () => { if (reduced(motion)) stop(); };
  motion.addEventListener('change', onMotionChange);
  if (!reduced(motion) && clock) {
    node.dataset.tenetReveal = 'started';
    scene = dualScene(node, clock, targets, 1, () => {
      stop();
      node.dataset.tenetReveal = 'settled';
    });
  }

  return {
    destroy() {
      motion.removeEventListener('change', onMotionChange);
      stop();
    },
  };
}

/** Calendar navigation: day/date text moves FORWARD; four fragments move
 * INVERTED back into the newly selected day. The real date has already
 * changed in Svelte; motion never delays selection, focus or keyboard input.
 */
export function tenetCalendarGrid(
  node: HTMLElement,
  initial: { month: number; selected: string },
) {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  let lastMonth = initial.month;
  let lastSelected = initial.selected;
  let scene: Scene | null = null;
  let activeTargets: HTMLElement[] = [];

  function stop() {
    const old = scene;
    scene = null;
    old?.forward.kill();
    old?.inverted.kill();
    old?.fragments.forEach(fragment => fragment.remove());
    gsap.killTweensOf(activeTargets);
    gsap.set(activeTargets, { clearProps: 'opacity,visibility,transform' });
    activeTargets = [];
  }
  const onMotionChange = () => { if (reduced(motion)) stop(); };
  motion.addEventListener('change', onMotionChange);

  return {
    update(next: { month: number; selected: string }) {
      const changedMonth = next.month !== lastMonth;
      const changedSelection = next.selected !== lastSelected;
      const direction = next.month < lastMonth ? -1 : 1;
      lastMonth = next.month;
      lastSelected = next.selected;
      if (!changedMonth && !changedSelection) return;

      stop();
      if (reduced(motion)) return;
      const selected = node.querySelector<HTMLElement>('[data-calendar-selected="true"]');
      if (!selected) return;
      activeTargets = changedMonth
        ? Array.from(node.querySelectorAll<HTMLElement>('[data-calendar-day]'))
        : [selected];
      node.dataset.tenetScenes = String(Number(node.dataset.tenetScenes || '0') + 1);
      node.dataset.tenetContact = next.selected;
      scene = dualScene(node, selected, activeTargets, direction, stop);
    },
    destroy() {
      motion.removeEventListener('change', onMotionChange);
      stop();
    },
  };
}

/** Each second: the stale numeral moves forward out of the cell while
 * the real current numeral replays a remembered outbound path INVERTED
 * into the SAME cell. The real datetime and accessible clock never pause.
 */
export function tenetClockDigit(node: HTMLElement, initial: string) {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  let previous = initial;
  let forward: gsap.core.Timeline | null = null;
  let inverted: gsap.core.Timeline | null = null;
  let ghost: HTMLSpanElement | null = null;

  function stop() {
    forward?.kill();
    inverted?.kill();
    forward = null;
    inverted = null;
    ghost?.remove();
    ghost = null;
    gsap.killTweensOf(node);
    gsap.set(node, { clearProps: 'opacity,visibility,transform,color' });
  }
  const onMotionChange = () => { if (reduced(motion)) stop(); };
  motion.addEventListener('change', onMotionChange);
  return {
    update(next: string) {
      if (next === previous) return;
      const outgoing = previous;
      previous = next;
      stop();
      if (reduced(motion) || !node.parentElement) return;
      const relic = node.ownerDocument.createElement('span');
      relic.textContent = outgoing;
      relic.setAttribute('aria-hidden', 'true');
      relic.className = 'pointer-events-none absolute inset-0 flex items-center justify-center text-kira-muted';
      node.parentElement.appendChild(relic);
      ghost = relic;

      inverted = gsap.timeline({ paused: true });
      inverted.to(node, { yPercent: 85, autoAlpha: 0, duration: 0.4, ease: 'power3.out' }, 0);
      inverted.progress(1); // New numeral is known; reverse its future exit.
      forward = gsap.timeline({
        paused: true,
        onUpdate: () => { inverted?.progress(dualTime(forward?.progress() ?? 1).inverted); },
        onComplete: stop,
      });
      forward.fromTo(relic,
        { yPercent: 0, autoAlpha: 1 },
        { yPercent: -85, autoAlpha: 0, duration: 0.4, ease: 'power3.out' }, 0);
      forward.play(0);
    },
    destroy() {
      motion.removeEventListener('change', onMotionChange);
      stop();
    },
  };
}

/** Clock easter egg: every click chooses a fresh set of six paths, but
 * each single event uses ONE GSAP timeline played FORWARD and then
 * REVERSED. This reverses the exact easing, stagger, rotation and opacity,
 * unlike separate "fly away" and "spring back" tweens.
 */
export function tenetClockJuggle(node: HTMLButtonElement) {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  let animation: gsap.core.Timeline | null = null;
  let restoreTimer: number | null = null;
  let previousFlights: ClockFlight[] = [];
  const cells = () => Array.from(node.querySelectorAll<HTMLElement>('[data-clock-cell]'));

  function reset() {
    if (restoreTimer !== null) {
      window.clearTimeout(restoreTimer);
      restoreTimer = null;
    }
    const current = animation;
    animation = null;
    current?.kill();
    const targets = cells();
    gsap.killTweensOf(targets);
    gsap.set(targets, { clearProps: 'transform,opacity,visibility' });
    node.dataset.tenetPlayback = 'settled';
  }

  function play() {
    reset();
    if (reduced(motion)) return;
    const targets = cells();
    if (!targets.length) return;
    const flights = nextClockFlights(targets.length, previousFlights);
    previousFlights = flights; // Next event differs; reverse never rerolls.
    let returning = false;
    node.dataset.tenetEvents = String(Number(node.dataset.tenetEvents || '0') + 1);
    node.dataset.tenetPlayback = 'forward';
    animation = gsap.timeline({
      paused: true,
      onComplete: () => {
        if (!returning) {
          returning = true;
          node.dataset.tenetPlayback = 'inverted';
          animation?.reverse(); // Same recorded physical history backwards.
        }
      },
      onReverseComplete: reset,
    });
    animation.to(targets, {
      x: (i: number) => flights[i].x,
      y: (i: number) => flights[i].y,
      rotation: (i: number) => flights[i].rotation,
      scale: (i: number) => flights[i].scale,
      opacity: (i: number) => flights[i].opacity,
      duration: 0.38,
      stagger: 0.026,
      ease: 'power3.out',
    }, 0);
    animation.play(0);
    // Guard against throttled frames in WebKitGTK. Never strand digits.
    restoreTimer = window.setTimeout(reset, 1450);
  }

  const onMotionChange = () => { if (reduced(motion)) reset(); };
  node.addEventListener('click', play);
  motion.addEventListener('change', onMotionChange);
  return {
    destroy() {
      node.removeEventListener('click', play);
      motion.removeEventListener('change', onMotionChange);
      reset();
    },
  };
}
