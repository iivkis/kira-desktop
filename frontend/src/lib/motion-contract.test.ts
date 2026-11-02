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

  it('prepares compositor layers before native reveal, no visible opacity snap', () => {
    expect(motion).toContain("gsap.set(content, {");
    expect(motion).toContain("y: 9,");
    expect(motion).toContain("scale: 0.988,");
    expect(motion).toContain("opacity: 0.94,");
    expect(motion).toContain("willChange: 'transform, opacity'");
    expect(motion).toContain("node.dataset.tenetReveal = 'ready'");
    expect(motion).toContain("window.setTimeout(() => {");
    expect(motion).toContain("}, 550)");
  });

  it('runs the recorded inverse and forward on one GPU-friendly master', () => {
    expect(motion).toContain("slice(0, options.sections ?? 3)");
    expect(motion).toContain("master = gsap.timeline({ paused: true, onComplete: settle })");
    expect(motion).toContain("master.to(content, {");
    expect(motion).toContain("master.fromTo(traces,");
    expect(motion).toContain("duration: 0.44");
    expect(motion).toContain("duration: 0.22");
    expect(motion).toContain("ease: 'back.out(1.22)'");
    expect(motion).not.toContain("ease: 'sine.inOut'");
    expect(motion).toContain("ease: 'power3.in'");
    expect(motion).toContain("from: 'end'");
    expect(motion).toContain("force3D: true");
    expect(motion).not.toContain("onUpdate:");
    expect(motion).not.toContain("inverted?.progress(");
  });

  it('cleans all transforms, listeners, memory and fallback on interrupt', () => {
    expect(motion).toContain("window.setTimeout(settle, 1200)");
    expect(motion).toContain("window.clearTimeout(awaitVisible)");
    expect(motion).toContain("master?.kill()");
    expect(motion).toContain("trace.remove()");
    expect(motion).toContain("clearProps: 'transform,opacity,willChange'");
    expect(motion).toContain("window.addEventListener('kira:overlay:visible', onVisible)");
    expect(motion).toContain("window.removeEventListener('kira:overlay:visible', onVisible)");
    expect(motion).toContain("preference.removeEventListener");
    expect(motion).toContain("generation === Number(node.dataset.kiraGeneration)");
  });

  it('uses a single shared tab indicator with interruptible GSAP travel', () => {
    expect(system).toContain('use:tenetFlowIndicator={{ active }}');
    expect(system).toContain('data-kira-flow-indicator');
    expect(system).toContain('data-kira-tab="overview"');
    expect(system).toContain('data-kira-tab="processes"');
    expect(system).not.toContain("'border-kira-accent text-kira-accent'");
    expect(motion).toContain('gsap.to(indicator, {');
    expect(motion).toContain("overwrite: 'auto', force3D: true");
    expect(motion).toContain("new ResizeObserver(() => relocate(active, false))");
    expect(motion).toContain('gsap.killTweensOf(indicator)');
    expect(motion).toContain('export function tenetPanelArrival');
    expect(system).toContain('use:tenetPanelArrival');
  });

  it('uses physical widget contacts without duplicating legacy stagger animations', () => {
    expect(calendar).toContain('return tenetWidgetReveal(node, {');
    expect(launcher).toContain("use:tenetWidgetReveal={{ contact: '[data-kira-contact]', direction: -1 }}");
    expect(system).toContain("use:tenetWidgetReveal={{ contact: '[data-kira-contact]', direction: 1, sections: 2 }}");
    expect(launcher).toContain('data-kira-contact');
    expect(system).toContain('data-kira-contact');
    expect(system).not.toContain('use:revealStagger');
  });
});
