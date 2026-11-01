/** Delegated GSAP hover refraction for all clickable desktop overlay items.
 *
 * The functional button does not change its hitbox or receive transforms.
 * An optical layer is warped under the pointer; an optional inner glyph
 * layer deforms independently of Calendar's existing TENET/clock timelines.
 * On departure, the trace retraces the actual bounded pointer history
 * INVERTED, while the base UI continues FORWARD. One event, same geometry.
 */
import { gsap } from 'gsap';
import {
  inverseLensPath, lensPoint, rememberLensPoint, type LensPoint,
} from './tenet-lens-model';

const SELECTOR = 'button:not(:disabled), a[href], [role="button"]:not([aria-disabled="true"])';

export function tenetHoverLens(root: HTMLElement) {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const finePointer = window.matchMedia('(hover: hover) and (pointer: fine)');
  let active: HTMLElement | null = null;
  let history: LensPoint[] = [];
  let enterTween: gsap.core.Tween | null = null;
  let pointerTween: gsap.core.Tween | null = null;
  let returnTimeline: gsap.core.Timeline | null = null;
  let restoreTimer: number | null = null;
  let contentTween: gsap.core.Tween | null = null;
  let content: HTMLElement | null = null;

  const eligible = (event: Event): HTMLElement | null => {
    if (!(event.target instanceof Element)) return null;
    const candidate = event.target.closest<HTMLElement>(SELECTOR);
    return candidate && root.contains(candidate) ? candidate : null;
  };
  const enabled = (event: PointerEvent) => !motion.matches &&
    finePointer.matches && (event.pointerType === 'mouse' || event.pointerType === 'pen');

  function endAnimations() {
    if (restoreTimer !== null) {
      window.clearTimeout(restoreTimer);
      restoreTimer = null;
    }
    enterTween?.kill();
    pointerTween?.kill();
    returnTimeline?.kill();
    contentTween?.kill();
    enterTween = pointerTween = contentTween = null;
    returnTimeline = null;
  }

  function reset() {
    endAnimations();
    if (content) gsap.set(content, { clearProps: 'transform' });
    if (active) {
      gsap.set(active, { clearProps:
        '--kira-lens-opacity,--kira-lens-x,--kira-lens-y,--kira-lens-dx,--kira-lens-dy,--kira-lens-shear' });
      active.classList.remove('kira-tenet-lens-target');
      active.removeAttribute('data-tenet-lens-state');
    }
    content = null;
    active = null;
    history = [];
  }

  function values(point: LensPoint) {
    return {
      '--kira-lens-x': point.x + '%',
      '--kira-lens-y': point.y + '%',
      '--kira-lens-dx': point.dx + 'px',
      '--kira-lens-dy': point.dy + 'px',
      '--kira-lens-shear': point.dx * 0.65 + 'deg',
    };
  }

  function onOver(event: PointerEvent) {
    if (!enabled(event)) return;
    const target = eligible(event);
    if (!target || target === active) return;
    reset();
    active = target;
    content = target.querySelector<HTMLElement>('[data-tenet-lens-body]');
    const rect = target.getBoundingClientRect();
    const point = lensPoint(event.clientX, event.clientY,
      rect.left, rect.top, rect.width, rect.height);
    history = rememberLensPoint([], point);
    target.classList.add('kira-tenet-lens-target');
    target.dataset.tenetLensState = 'forward';
    gsap.set(target, { ...values(point), '--kira-lens-opacity': 0 });
    enterTween = gsap.to(target, {
      '--kira-lens-opacity': 0.94, duration: 0.22,
      ease: 'power2.out', overwrite: 'auto',
    });
    if (content) {
      contentTween = gsap.to(content, {
        x: point.dx * 0.5, y: point.dy * 0.5,
        skewX: point.dx * 0.55, scale: 1.055,
        duration: 0.24, ease: 'power2.out', overwrite: 'auto',
      });
    }
  }

  function onMove(event: PointerEvent) {
    if (!active || !enabled(event) || eligible(event) !== active) return;
    const rect = active.getBoundingClientRect();
    const point = lensPoint(event.clientX, event.clientY,
      rect.left, rect.top, rect.width, rect.height);
    history = rememberLensPoint(history, point);
    pointerTween?.kill();
    pointerTween = gsap.to(active, {
      ...values(point), duration: 0.105, ease: 'power2.out',
      overwrite: 'auto',
    });
    if (content) {
      contentTween?.kill();
      contentTween = gsap.to(content, {
        x: point.dx * 0.5, y: point.dy * 0.5,
        skewX: point.dx * 0.55, scale: 1.055,
        duration: 0.16, ease: 'power2.out',
        overwrite: 'auto',
      });
    }
  }

  function onOut(event: PointerEvent) {
    if (!active) return;
    const departed = eligible(event);
    if (departed !== active) return;
    const next = event.relatedTarget;
    if (next instanceof Node && active.contains(next)) return;
    const node = active;
    const reversed = inverseLensPath(history);
    endAnimations();
    node.dataset.tenetLensState = 'inverted';
    returnTimeline = gsap.timeline({ onComplete: reset });
    // One recorded sequence. Never sample new mouse positions after departure.
    for (const point of reversed) {
      returnTimeline.to(node, {
        ...values(point), duration: 0.018, ease: 'none',
      });
    }
    returnTimeline.to(node, {
      '--kira-lens-opacity': 0,
      '--kira-lens-dx': '0px', '--kira-lens-dy': '0px',
      '--kira-lens-shear': '0deg',
      duration: 0.2, ease: 'power2.out',
    });
    if (content) {
      returnTimeline.to(content, {
        x: 0, y: 0, skewX: 0, scale: 1, duration: 0.25,
        ease: 'power2.out',
      }, '<');
    }
    // GTK/WebKit may suspend requestAnimationFrame while transitioning
    // overlays. A one-shot safeguard restores everything, then cancels
    // itself on normal GSAP completion or new input.
    restoreTimer = window.setTimeout(reset, 750);
  }

  const onMotionChange = () => { if (motion.matches || !finePointer.matches) reset(); };
  const onDown = () => { if (active) reset(); };
  const onFocus = () => { if (active) reset(); };
  root.addEventListener('pointerover', onOver);
  root.addEventListener('pointermove', onMove);
  root.addEventListener('pointerout', onOut);
  root.addEventListener('pointerdown', onDown);
  root.addEventListener('focusin', onFocus);
  motion.addEventListener('change', onMotionChange);
  finePointer.addEventListener('change', onMotionChange);
  return {
    destroy() {
      root.removeEventListener('pointerover', onOver);
      root.removeEventListener('pointermove', onMove);
      root.removeEventListener('pointerout', onOut);
      root.removeEventListener('pointerdown', onDown);
      root.removeEventListener('focusin', onFocus);
      motion.removeEventListener('change', onMotionChange);
      finePointer.removeEventListener('change', onMotionChange);
      reset();
    },
  };
}
