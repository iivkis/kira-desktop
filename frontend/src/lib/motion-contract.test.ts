import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { dualTime, tenetFragmentsFor } from './calendar-tenet-model';

const motion = readFileSync(new URL('./motion.ts', import.meta.url), 'utf8');
const calendar = readFileSync(new URL('./calendar-tenet-motion.ts', import.meta.url), 'utf8');
const launcher = readFileSync(new URL('./components/LauncherWidget.svelte', import.meta.url), 'utf8');
const system = readFileSync(new URL('./components/SystemWidget.svelte', import.meta.url), 'utf8');
const workspace = readFileSync(new URL('./components/WorkspacePreview.svelte', import.meta.url), 'utf8');
const calendarWidget = readFileSync(new URL('./components/CalendarWidget.svelte', import.meta.url), 'utf8');
const styles = readFileSync(new URL('../style.css', import.meta.url), 'utf8');

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
    expect(motion).toContain("new ResizeObserver(() => relocate(active, true))");
    expect(motion).toContain('gsap.killTweensOf(indicator)');
    expect(motion).toContain("recovery = window.setTimeout(() => {");
    expect(motion).toContain("}, 650)");
    expect(motion).toContain("node.dataset.kiraFlowMoving = 'true'");
    expect(motion).toContain("onComplete: endTravel");
    expect(styles).toContain('[data-kira-flow-moving="true"] [data-calendar-selected="true"]');
    expect(styles).toContain('[data-kira-flow-moving="true"] [data-focused="true"]');
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

describe('TENET continuous interaction contract', () => {
  it('shares one physical 2D marker across every selection surface', () => {
    for (const source of [calendarWidget, launcher, workspace]) {
      expect(source).toContain('data-kira-flow-indicator');
      expect(source).toContain('use:tenetFlowIndicator={{ active:');
      expect(source).toContain("axis: 'both'");
      expect(source).toContain('data-kira-tab=');
    }
    expect(system).toContain("use:tenetFlowIndicator={{ active }}");
    expect(motion).toContain("ease: 'back.out(1.18)'");
    expect(motion).toContain("overwrite: 'auto', force3D: true");
    expect(motion).toContain("const preference = window.matchMedia('(prefers-reduced-motion: reduce)')");
    expect(motion).toContain('observer?.disconnect()');
    expect(motion).toContain('gsap.killTweensOf(indicator)');
  });

  it('uses keyed GSAP FLIP for purposeful row reordering, not live sensor values', () => {
    expect(motion).toContain("import { Flip } from 'gsap/Flip'");
    expect(motion).toContain('Flip.getState(rows)');
    expect(motion).toContain('Flip.from(state, {');
    expect(motion).toContain('rows.length || reducedMotion()');
    expect(motion).toContain('slice(0, 24)');
    expect(motion).toContain('export function tenetReflowScope(');
    expect(motion).toContain('reflows.delete(container)');
    expect(launcher).toContain('use:tenetReflowScope use:tenetScrollScope');
    expect(system).toContain('use:tenetReflowScope');
    for (const source of [launcher, system]) {
      expect(source).toContain('tenetReflow(');
      expect(source).toContain('data-kira-reflow');
    }
  });

  it('moves workspace scroll along the correct axis and keeps calendar contact history', () => {
    expect(workspace).toContain('tenetScrollIntoView(list, match, animate)');
    expect(launcher).toContain("tenetScrollIntoView(results, item, true, 'y')");
    expect(motion).toContain("gsap.killTweensOf(list, property)");
    expect(motion).toContain('gsap.to(list, {');
    expect(motion).toContain('export function tenetScrollScope(');
    expect(workspace).toContain('use:tenetScrollScope');
    expect(calendar).toContain('activeTargets = changedMonth ? [node] : []');
    expect(calendar).toContain("ease: 'back.out(1.18)'");
    expect(calendar).toContain('inverted.progress(dualTime(forward.progress()).inverted)');
  });

  it('expands a single accessible diagnostics surface and cleans up all tweens', () => {
    expect(system).toContain('use:tenetDisclosure');
    expect(system).toContain('data-kira-disclosure-content');
    expect(motion).toContain("summary.addEventListener('click', onClick)");
    expect(motion).toContain("summary.setAttribute('aria-expanded', String(next))");
    expect(motion).toContain("summary.removeEventListener('click', onClick)");
    expect(motion).toContain("preference.removeEventListener('change', onPreference)");
  });
});
