/** GSAP is Kira Desktop's only JavaScript motion engine.
 * Shared entrances are triggered when the native GTK surface becomes
 * VISIBLE, never when Svelte mounts into an opacity-zero WebKit window.
 * A real FORWARD content assembly is primary; subtle INVERTED traces
 * replay saved paths to the same physical editorial contact point.
 */
import { gsap } from 'gsap';
import { Flip } from 'gsap/Flip';
import { tick } from 'svelte';
import { notifyOverlayClosed } from './sdk/core';

gsap.registerPlugin(Flip);
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
  let closing = false;
  let closedAcknowledged = false;

  // Closing is not unmount: keep the SAME physical history, including
  // stagger, bounce and inverted traces, until Rust acknowledges the exit.
  function acknowledgeClose() {
    if (closedAcknowledged) return;
    closedAcknowledged = true;
    node.dataset.tenetReveal = 'reversed';
    notifyOverlayClosed(Number(node.dataset.kiraGeneration));
  }

  function settle() {
    if (restore !== null) window.clearTimeout(restore);
    if (awaitVisible !== null) window.clearTimeout(awaitVisible);
    restore = null;
    awaitVisible = null;
    master?.kill();
    master = null;
    closing = false;
    closedAcknowledged = false;
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

    master = gsap.timeline({
      paused: true,
      onComplete: () => { node.dataset.tenetReveal = 'settled'; },
      onReverseComplete: acknowledgeClose,
    });
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
    // WebKitGTK can throttle frames. Settle the exact ending geometry but
    // preserve the timeline and fragments as temporal memory for closing.
    restore = window.setTimeout(() => {
      restore = null;
      if (!closing && master) {
        master.progress(1).pause();
        node.dataset.tenetReveal = 'settled';
      }
    }, 1200);
  }

  function isCurrentGeneration(event: Event): boolean {
    const generation = (event as CustomEvent<{ generation?: unknown }>).detail?.generation;
    return Number.isSafeInteger(generation) && generation === Number(node.dataset.kiraGeneration);
  }
  function onVisible(event: Event) {
    if (isCurrentGeneration(event)) begin();
  }
  function onClose(event: Event) {
    if (!isCurrentGeneration(event) || closing) return;
    closing = true;
    closedAcknowledged = false;
    if (restore !== null) window.clearTimeout(restore);
    restore = null;
    if (awaitVisible !== null) window.clearTimeout(awaitVisible);
    awaitVisible = null;
    // Closing before the first painted frame, without motion, or after a
    // failed mount must not wait for a timeline that never ran.
    if (preference.matches || !master || master.progress() <= 0) {
      acknowledgeClose();
      return;
    }
    node.dataset.tenetReveal = 'reversing';
    // A GSAP reverse walks EVERY recorded intermediate state and the
    // saved INVERTED fragment tracks in precisely inverse temporal order.
    // No new random seed, mirrored ease or replacement exit tween.
    master.reverse();
  }
  function onReopen(event: Event) {
    if (!isCurrentGeneration(event) || !closing) return;
    closing = false;
    closedAcknowledged = false;
    if (master) {
      node.dataset.tenetReveal = 'started';
      master.play(); // Retarget from the live position, without a snap.
    }
  }
  function onHide(event: Event) {
    if (isCurrentGeneration(event)) {
      started = true;
      settle();
    }
  }
  function onPreference() {
    if (preference.matches) {
      if (closing) acknowledgeClose();
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
  window.addEventListener('kira:overlay:close', onClose);
  window.addEventListener('kira:overlay:reopen', onReopen);
  window.addEventListener('kira:overlay:hide', onHide);
  preference.addEventListener('change', onPreference);
  return {
    destroy() {
      window.removeEventListener('kira:overlay:visible', onVisible);
      window.removeEventListener('kira:overlay:close', onClose);
      window.removeEventListener('kira:overlay:reopen', onReopen);
      window.removeEventListener('kira:overlay:hide', onHide);
      preference.removeEventListener('change', onPreference);
      settle();
    },
  };
}

/** A single marker owns the selected state across tabs, calendar days,
 * launcher rows and workspace chips. It always retargets from the actual
 * in-flight position instead of cross-fading between separate highlights.
 */
export interface TenetFlowTarget {
  active: string;
  axis?: 'x' | 'both';
}

export function tenetFlowIndicator(node: HTMLElement, initial: TenetFlowTarget) {
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  const indicator = node.querySelector<HTMLElement>('[data-kira-flow-indicator]');
  let active = initial.active;
  const axis = initial.axis ?? 'x';
  if (!indicator) return { update(_next: TenetFlowTarget) {}, destroy() {} };
  let last: { x: number; y: number; width: number; height: number } | null = null;
  let recovery: number | null = null;
  function cancelRecovery() {
    if (recovery !== null) window.clearTimeout(recovery);
    recovery = null;
  }
  const endTravel = () => { delete node.dataset.kiraFlowMoving; };

  function geometry(key: string) {
    const target = Array.from(node.querySelectorAll<HTMLElement>('[data-kira-tab]'))
      .find(item => item.dataset.kiraTab === key);
    if (!target || !target.isConnected || target.offsetWidth <= 0) return null;
    return {
      x: target.offsetLeft, y: target.offsetTop,
      width: target.offsetWidth, height: target.offsetHeight,
    };
  }

  function relocate(key: string, animate: boolean) {
    const next = geometry(key);
    if (!next) {
      cancelRecovery();
      endTravel();
      gsap.killTweensOf(indicator);
      gsap.set(indicator, { autoAlpha: 0 });
      last = null;
      return;
    }
    const vars = axis === 'both'
      ? { x: next.x, y: next.y, width: next.width, height: next.height }
      : { x: next.x, width: next.width };
    if (last && Object.keys(vars).every(prop => next[prop as keyof typeof next] === last![prop as keyof typeof last])) {
      return;
    }
    const moving = animate && !preference.matches && last !== null;
    cancelRecovery();
    last = next;
    if (!moving) {
      endTravel();
      gsap.killTweensOf(indicator);
      gsap.set(indicator, { ...vars, autoAlpha: 1 });
      return;
    }
    // Selected black text must never be black-on-black before the orange
    // background arrives. The selected text stays light only while moving.
    node.dataset.kiraFlowMoving = 'true';
    // Only geometry springs. Visibility stays monotonic and never overshoots.
    gsap.to(indicator, {
      ...vars, duration: 0.42, ease: 'back.out(1.18)',
      overwrite: 'auto', force3D: true, onComplete: endTravel,
    });
    gsap.to(indicator, { autoAlpha: 1, duration: 0.12, ease: 'power2.out', overwrite: 'auto' });
    // rAF can stall for a compact, obscured WebKitGTK surface. Correctness
    // always wins: reconcile exact geometry after one bounded grace period.
    recovery = window.setTimeout(() => {
      recovery = null;
      gsap.killTweensOf(indicator);
      gsap.set(indicator, { ...vars, autoAlpha: 1 });
      endTravel();
    }, 650);
  }

  relocate(active, false);
  const onPreference = () => {
    if (preference.matches) {
      last = null;
      relocate(active, false);
    }
  };
  preference.addEventListener('change', onPreference);
  const observer = typeof ResizeObserver === 'undefined' ? null :
    new ResizeObserver(() => relocate(active, true));
  observer?.observe(node);

  return {
    update(next: TenetFlowTarget) {
      active = next.active;
      // Same selection can still change physical coordinates after keyed
      // reorder or filter; its marker must follow rather than teleport.
      relocate(active, true);
    },
    destroy() {
      cancelRecovery();
      endTravel();
      observer?.disconnect();
      preference.removeEventListener('change', onPreference);
      gsap.killTweensOf(indicator);
      gsap.set(indicator, { clearProps: 'transform,width,height,opacity,visibility,willChange' });
    },
  };
}

/** User-driven FLIP transitions are bounded and owned by the list's lifetime. */
type RunningReflow = { tween: gsap.core.Timeline; release: () => void };
const reflows = new WeakMap<HTMLElement, RunningReflow>();
const reflowEpochs = new WeakMap<HTMLElement, number>();

function stopReflow(container: HTMLElement, finish: boolean) {
  const running = reflows.get(container);
  if (!running) return;
  reflows.delete(container);
  if (finish) running.tween.progress(1);
  running.tween.kill();
  running.release();
}

export function tenetReflowScope(node: HTMLElement) {
  return {
    destroy() {
      reflowEpochs.set(node, (reflowEpochs.get(node) ?? 0) + 1);
      stopReflow(node, false);
    },
  };
}

/** Retain painted positions of stable rows during explicit user reorder/filter.
 * Reacting to sensor samples this way would animate facts and is forbidden.
 */
export function tenetReflow(container: HTMLElement | undefined, mutate: () => void): void {
  if (!container) {
    mutate();
    return;
  }
  const epoch = (reflowEpochs.get(container) ?? 0) + 1;
  reflowEpochs.set(container, epoch);
  stopReflow(container, false);
  const rows = Array.from(container.querySelectorAll<HTMLElement>('[data-kira-reflow]')).slice(0, 24);
  if (!rows.length || reducedMotion()) {
    mutate();
    return;
  }
  let state: ReturnType<typeof Flip.getState>;
  try {
    state = Flip.getState(rows);
  } catch {
    // Motion is always optional; malformed geometry must never block typing.
    mutate();
    return;
  }
  mutate();
  void tick().then(() => {
    if (!container.isConnected || reflowEpochs.get(container) !== epoch || reducedMotion()) return;
    const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
    let tween: gsap.core.Timeline | null = null;
    const onPreference = () => {
      if (preference.matches) stopReflow(container, true);
    };
    const release = () => {
      preference.removeEventListener('change', onPreference);
      if (reflows.get(container)?.tween === tween) reflows.delete(container);
    };
    try {
      tween = Flip.from(state, {
        duration: 0.34,
        ease: 'back.out(1.14)',
        absolute: false,
        scale: false,
        prune: true,
        overwrite: true,
        onComplete: release,
      });
      reflows.set(container, { tween, release });
      preference.addEventListener('change', onPreference);
      // An unchanged keyed list can produce a zero-length FLIP scene.
      // Do not retain its listeners or entries after a synchronous settle.
      if (tween.duration() === 0) release();
    } catch {
      release();
      // Keep the already-committed user action, even if the FLIP plugin fails.
    }
  });
}

/** Reusable, interruptible GSAP scroll with a reliable reduced-motion reset.
 * The nearest visible position, not scrollIntoView's implicit browser jump.
 */
export function tenetScrollIntoView(
  list: HTMLElement, item: HTMLElement, animate: boolean, axis: 'x' | 'y' = 'x',
): void {
  const vertical = axis === 'y';
  const start = vertical ? item.offsetTop : item.offsetLeft;
  const end = start + (vertical ? item.offsetHeight : item.offsetWidth);
  const current = vertical ? list.scrollTop : list.scrollLeft;
  const size = vertical ? list.clientHeight : list.clientWidth;
  const destination = Math.max(0, start < current ? start : end > current + size ? end - size : current);
  if (Math.abs(destination - current) < 1) return;
  const property = vertical ? 'scrollTop' : 'scrollLeft';
  gsap.killTweensOf(list, property);
  if (!animate || reducedMotion()) {
    if (vertical) list.scrollTop = destination;
    else list.scrollLeft = destination;
    return;
  }
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  const release = () => preference.removeEventListener('change', onPreference);
  const onPreference = () => {
    if (!preference.matches) return;
    gsap.killTweensOf(list, property);
    if (vertical) list.scrollTop = destination;
    else list.scrollLeft = destination;
    release();
  };
  preference.addEventListener('change', onPreference);
  gsap.to(list, {
    [property]: destination,
    duration: 0.34,
    ease: 'power3.out',
    overwrite: 'auto',
    onComplete: release,
    onInterrupt: release,
  });
}

export function tenetScrollScope(node: HTMLElement) {
  return {
    destroy() {
      gsap.killTweensOf(node, 'scrollLeft,scrollTop');
    },
  };
}

/** A controlled disclosure retains one content surface while height follows
 * a short elastic trajectory, rather than popping its layout instantly.
 */
export function tenetDisclosure(node: HTMLDetailsElement) {
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  const summary = node.querySelector('summary');
  const content = node.querySelector<HTMLElement>('[data-kira-disclosure-content]');
  if (!summary || !content) return { destroy() {} };
  let expanded = node.open;
  let animation: gsap.core.Tween | null = null;
  const finish = () => {
    animation?.kill();
    animation = null;
    gsap.set(content, { clearProps: 'height,overflow,opacity,visibility' });
    if (!expanded) node.open = false;
  };
  const onClick = (event: MouseEvent) => {
    event.preventDefault();
    const next = !expanded;
    expanded = next;
    animation?.kill();
    animation = null;
    summary.setAttribute('aria-expanded', String(next));
    if (preference.matches) {
      node.open = next;
      gsap.set(content, { clearProps: 'height,overflow,opacity,visibility' });
      return;
    }
    const previousHeight = node.open ? content.getBoundingClientRect().height : 0;
    node.open = true;
    const full = content.scrollHeight;
    gsap.set(content, { overflow: 'hidden', height: previousHeight, opacity: next ? 0.88 : 1 });
    animation = gsap.to(content, {
      height: next ? full : 0,
      opacity: next ? 1 : 0.88,
      duration: 0.34,
      ease: 'power3.out',
      overwrite: 'auto',
      onComplete: finish,
    });
  };
  const onPreference = () => { if (preference.matches) finish(); };
  summary.addEventListener('click', onClick);
  preference.addEventListener('change', onPreference);
  return {
    destroy() {
      summary.removeEventListener('click', onClick);
      preference.removeEventListener('change', onPreference);
      animation?.kill();
      gsap.set(content, { clearProps: 'height,overflow,opacity,visibility' });
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
