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
