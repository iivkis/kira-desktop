/** GSAP is the project's sole JavaScript motion engine.
 * Actions are scoped to the short-lived Svelte widget DOM. They never animate
 * GTK/Wayland windows, and reduced-motion is a functional no-animation path.
 */
import { gsap } from 'gsap';

const reducedMotion = () => typeof window !== 'undefined' &&
  window.matchMedia('(prefers-reduced-motion: reduce)').matches;

/** Deliberately short, staggered entry animation for a newly mounted tab. */
export function revealStagger(node: HTMLElement) {
  if (reducedMotion()) return;
  const targets = node.querySelectorAll<HTMLElement>('[data-kira-enter]');
  const animation = gsap.fromTo(targets,
    { autoAlpha: 0, y: 14, scale: 0.985 },
    {
      autoAlpha: 1, y: 0, scale: 1, duration: 0.55,
      stagger: 0.065, ease: 'power3.out',
      clearProps: 'opacity,visibility,transform',
    },
  );
  return {
    destroy() {
      animation.kill();
      gsap.set(targets, { clearProps: 'opacity,visibility,transform' });
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

/**
 * Calendar-only GSAP interaction: horizontal month/year reveal followed by a
 * restrained selection pop. Svelte owns actual dates and focus; GSAP changes
 * presentation only. No timers, fake dates, or transitional DOM copies.
 */
export function calendarGridMotion(
  node: HTMLElement,
  initial: { month: number; selected: string },
) {
  let lastMonth = initial.month;
  let lastSelected = initial.selected;
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const buttons = () => node.querySelectorAll<HTMLElement>('[data-calendar-day]');
  const clear = () => {
    const days = buttons();
    gsap.killTweensOf(days);
    gsap.set(days, { clearProps: 'opacity,visibility,transform' });
  };
  const onMotionChange = () => {
    if (motion.matches) clear();
  };
  motion.addEventListener('change', onMotionChange);
  return {
    update(next: { month: number; selected: string }) {
      const monthChanged = next.month !== lastMonth;
      const selectedChanged = next.selected !== lastSelected;
      const direction = Math.sign(next.month - lastMonth);
      lastMonth = next.month;
      lastSelected = next.selected;
      if ((!monthChanged && !selectedChanged) || motion.matches) {
        if (motion.matches) clear();
        return;
      }
      if (monthChanged) {
        const days = buttons();
        gsap.killTweensOf(days);
        gsap.fromTo(days,
          { autoAlpha: 0, x: direction * 9 },
          {
            autoAlpha: 1, x: 0, duration: 0.3, stagger: 0.004,
            ease: 'power2.out', overwrite: true,
            clearProps: 'opacity,visibility,transform',
          },
        );
      } else {
        const selected = node.querySelector<HTMLElement>('[data-calendar-selected="true"]');
        if (!selected) return;
        gsap.killTweensOf(selected);
        gsap.fromTo(selected,
          { scale: 0.89, transformOrigin: 'center center' },
          {
            scale: 1, duration: 0.32, ease: 'back.out(2)',
            overwrite: true, clearProps: 'transform,transformOrigin',
          },
        );
      }
    },
    destroy() {
      motion.removeEventListener('change', onMotionChange);
      clear();
    },
  };
}

/** Odometer-style clock glyph with short, interruptible GSAP transitions.
 * The actual character, ARIA label and datetime remain Svelte-owned. This
 * action decorates only changed numerals, never replays unchanged digits.
 */
export function rollingClockDigit(node: HTMLElement, initial: string) {
  let previous = initial;
  let animation: gsap.core.Timeline | null = null;
  let ghost: HTMLSpanElement | null = null;
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');

  const stop = () => {
    animation?.kill();
    animation = null;
    ghost?.remove();
    ghost = null;
    gsap.set(node, { clearProps: 'opacity,transform,filter,color' });
  };
  const onMotionChange = () => { if (motion.matches) stop(); };
  motion.addEventListener('change', onMotionChange);

  return {
    update(next: string) {
      if (next === previous) return;
      const outgoing = previous;
      previous = next;
      stop();
      if (motion.matches) return;

      const parent = node.parentElement;
      if (!parent) return;
      ghost = node.ownerDocument.createElement('span');
      ghost.textContent = outgoing;
      ghost.setAttribute('aria-hidden', 'true');
      ghost.className = 'pointer-events-none absolute inset-0 flex items-center justify-center text-kira-muted';
      parent.appendChild(ghost);

      const currentGhost = ghost;
      animation = gsap.timeline({
        defaults: { duration: 0.42, ease: 'power3.out' },
        onComplete: () => {
          currentGhost.remove();
          if (ghost === currentGhost) ghost = null;
          animation = null;
          gsap.set(node, { clearProps: 'opacity,transform,filter,color' });
        },
      });
      animation.fromTo(node,
        { yPercent: 85, autoAlpha: 0, filter: 'blur(2px)', color: '#fca311' },
        { yPercent: 0, autoAlpha: 1, filter: 'blur(0px)', color: 'inherit' },
        0);
      animation.to(currentGhost, {
        yPercent: -85, autoAlpha: 0, duration: 0.35, ease: 'power2.inOut',
      }, 0);
    },
    destroy() {
      motion.removeEventListener('change', onMotionChange);
      stop();
    },
  };
}

/** Intentionally useless clock easter egg. Clicking the actual time sends its
 * six glyph cells flying in different directions before restoring every one.
 * GSAP owns decoration only: the live wall clock and odometer can tick in
 * their inner spans throughout. Repeated clicks restart cleanly.
 */
export function clockJuggle(node: HTMLButtonElement) {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  let animation: gsap.core.Timeline | null = null;
  let restoreTimer: ReturnType<typeof setTimeout> | null = null;
  const cells = () => Array.from(node.querySelectorAll<HTMLElement>('[data-clock-cell]'));

  function reset() {
    // A one-shot safety reset is important when GTK/WebKit throttles GSAP
    // requestAnimationFrame during compositor or window visibility changes.
    if (restoreTimer !== null) {
      window.clearTimeout(restoreTimer);
      restoreTimer = null;
    }
    animation?.kill();
    animation = null;
    const targets = cells();
    gsap.killTweensOf(targets);
    gsap.set(targets, { clearProps: 'transform,opacity,visibility' });
  }

  function play() {
    reset();
    if (motion.matches) return;
    const targets = cells();
    if (!targets.length) return;
    const xOffsets = [-27, -13, 23, -20, 15, 32];
    const yOffsets = [-28, 30, -35, 26, -25, 35];
    const rotations = [-32, 25, 40, -44, -20, 38];
    animation = gsap.timeline({ onComplete: reset });
    animation.to(targets, {
      x: (index: number) => xOffsets[index % xOffsets.length],
      y: (index: number) => yOffsets[index % yOffsets.length],
      rotation: (index: number) => rotations[index % rotations.length],
      scale: 0.72,
      opacity: 0.65,
      duration: 0.27,
      stagger: 0.026,
      ease: 'power3.out',
    }, 0);
    animation.to(targets, {
      x: 0, y: 0, rotation: 0, scale: 1, opacity: 1,
      duration: 0.48, stagger: { each: 0.02, from: 'end' },
      ease: 'back.out(2.3)',
    }, 0.34);
    // GSAP completes in under a second during normal rendering. If
    // requestAnimationFrame is suspended, never leave digits scattered.
    // The timeout is scoped to the click and cancelled on complete/destroy.
    restoreTimer = window.setTimeout(reset, 1200);
  }

  const onMotionChange = () => { if (motion.matches) reset(); };
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
