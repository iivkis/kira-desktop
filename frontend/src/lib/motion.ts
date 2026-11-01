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

