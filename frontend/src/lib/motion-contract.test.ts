import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { dualTime, tenetFragmentsFor } from './calendar-tenet-model';

const motion = readFileSync(new URL('./motion.ts', import.meta.url), 'utf8');
const calendar = readFileSync(new URL('./calendar-tenet-motion.ts', import.meta.url), 'utf8');
const launcher = readFileSync(new URL('./components/LauncherWidget.svelte', import.meta.url), 'utf8');
const system = readFileSync(new URL('./components/SystemWidget.svelte', import.meta.url), 'utf8');

describe('TENET shared widget appearance', () => {
  it('preserves one recorded trajectory in reverse chronological order', () => {
    for (const direction of [-1, 1] as const) {
      const saved = tenetFragmentsFor(direction);
      expect(saved).toHaveLength(4);
      for (const t of [0, 0.2, 0.5, 0.8, 1]) {
        const inverse = dualTime(t).inverted;
        expect(inverse).toBeCloseTo(dualTime(1 - t).forward);
        expect(saved.map(f => [f.x * inverse, f.y * inverse, f.rotation * inverse]))
          .toEqual(saved.map(f => [f.x * (1 - t), f.y * (1 - t), f.rotation * (1 - t)]));
      }
    }
  });

  it('requires a shared master, cancellable streams, reduced motion and DOM cleanup', () => {
    expect(motion).toContain('gsap.timeline({ paused: true })');
    expect(motion).toContain('dualTime(forward?.progress() ?? 1).inverted');
    expect(motion).toContain('preference.addEventListener');
    expect(motion).toContain("preference.removeEventListener");
    expect(motion).toContain('forward?.kill()');
    expect(motion).toContain('inverted?.kill()');
    expect(motion).toContain('trace.remove()');
    expect(motion).toContain('window.setTimeout(settle, 1100)');
    expect(motion).toContain("node.dataset.tenetReveal = 'settled'");
  });

  it('uses physical widget contacts without duplicating legacy stagger animations', () => {
    expect(calendar).toContain('return tenetWidgetReveal(node, {');
    expect(launcher).toContain("use:tenetWidgetReveal={{ contact: '[data-kira-contact]', direction: -1 }}");
    expect(system).toContain("use:tenetWidgetReveal={{ contact: '[data-kira-contact]', direction: 1 }}");
    expect(launcher).toContain('data-kira-contact');
    expect(system).toContain('data-kira-contact');
    expect(system).not.toContain('use:revealStagger');
  });
});
