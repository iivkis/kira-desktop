/** TENET temporal material deformation, not a glow or a glass overlay.
 *
 * An actual SVG feDisplacementMap warps SourceGraphic pixels (the selected
 * control or its glyphs). GSAP evolves its displacement amplitude and field
 * phase over time; the cursor writes a bounded causal path. Pointer departure
 * retraces that same path in reverse before settling to the exact unfiltered
 * original. The clickable layout rectangle is never transformed.
 */
import { gsap } from 'gsap';
import {
  inverseLensPath, lensPoint, rememberLensPoint, type LensPoint,
} from './tenet-lens-model';

const NS = 'http://www.w3.org/2000/svg';
const SELECTOR = 'button:not(:disabled), a[href], [role="button"]:not([aria-disabled="true"])';
let serial = 0;

type Field = { amplitude: number; phaseX: number; phaseY: number; frequency: number };

function svgNode(doc: Document, name: string): SVGElement {
  return doc.createElementNS(NS, name);
}

/** One in-document SVG filter per mounted widget. No CDN, image fetch,
 * permanent animation loop, pseudo-element or backdrop blur.
 */
function createWarpFilter(root: HTMLElement) {
  const id = 'kira-temporal-warp-' + (++serial);
  const doc = root.ownerDocument;
  const svg = svgNode(doc, 'svg');
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('focusable', 'false');
  svg.setAttribute('width', '0');
  svg.setAttribute('height', '0');
  svg.style.cssText = 'position:absolute;width:0;height:0;overflow:hidden;pointer-events:none';
  const defs = svgNode(doc, 'defs');
  const filter = svgNode(doc, 'filter');
  filter.setAttribute('id', id);
  filter.setAttribute('x', '-24%');
  filter.setAttribute('y', '-48%');
  filter.setAttribute('width', '148%');
  filter.setAttribute('height', '196%');
  filter.setAttribute('color-interpolation-filters', 'sRGB');

  const noise = svgNode(doc, 'feTurbulence');
  noise.setAttribute('type', 'fractalNoise');
  // Broad, low-frequency field: high-frequency noise and 10px offsets
  // were rasterizing tiny 11px tab glyphs into visible pixel stairs.
  noise.setAttribute('baseFrequency', '0.016 0.045');
  noise.setAttribute('numOctaves', '1');
  noise.setAttribute('seed', String(serial % 97 + 1));
  noise.setAttribute('result', 'materialField');
  // Blur ONLY the noise displacement field, never the rendered text.
  // Continuous vectors retain glyph anti-aliasing instead of snapping
  // adjacent source pixels onto abrupt blocky noise transitions.
  const smooth = svgNode(doc, 'feGaussianBlur');
  smooth.setAttribute('in', 'materialField');
  smooth.setAttribute('stdDeviation', '2.4');
  smooth.setAttribute('result', 'continuousField');
  const offset = svgNode(doc, 'feOffset');
  offset.setAttribute('in', 'continuousField');
  offset.setAttribute('result', 'shiftedField');
  const displacement = svgNode(doc, 'feDisplacementMap');
  displacement.setAttribute('in', 'SourceGraphic');
  displacement.setAttribute('in2', 'shiftedField');
  displacement.setAttribute('xChannelSelector', 'R');
  displacement.setAttribute('yChannelSelector', 'G');
  displacement.setAttribute('scale', '0');
  displacement.setAttribute('result', 'displacedPixels');
  // Gentle reconstruction of subpixel edges AFTER displacement; this
  // reduces stair-stepped font rasterization without blurring the tab
  // background or the border (only the text span is filtered).
  const edgeAA = svgNode(doc, 'feGaussianBlur');
  edgeAA.setAttribute('in', 'displacedPixels');
  edgeAA.setAttribute('stdDeviation', '0.24');
  filter.append(noise, smooth, offset, displacement, edgeAA);
  defs.appendChild(filter);
  svg.appendChild(defs);
  root.appendChild(svg);

  return {
    id, svg, displacement,
    apply(field: Field) {
      displacement.setAttribute('scale', field.amplitude.toFixed(2));
      offset.setAttribute('dx', field.phaseX.toFixed(2));
      offset.setAttribute('dy', field.phaseY.toFixed(2));
      noise.setAttribute('baseFrequency', field.frequency.toFixed(4) + ' 0.045');
    },
  };
}

export function tenetHoverLens(root: HTMLElement) {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const finePointer = window.matchMedia('(hover: hover) and (pointer: fine)');
  // Do not instantiate even the SVG noise field until the first genuine
  // mouse hover. Closed/idle widgets allocate no material compositor work.
  let warp: ReturnType<typeof createWarpFilter> | null = null;
  const ensureWarp = () => warp ?? (warp = createWarpFilter(root));
  const field: Field = { amplitude: 0, phaseX: 0, phaseY: 0, frequency: 0.016 };
  let active: HTMLElement | null = null;
  let painted: HTMLElement | null = null;
  let priorFilter = '';
  let history: LensPoint[] = [];
  let forward: gsap.core.Tween | null = null;
  let reverse: gsap.core.Timeline | null = null;
  let restoreTimer: number | null = null;

  const eligible = (event: Event): HTMLElement | null => {
    if (!(event.target instanceof Element)) return null;
    const target = event.target.closest<HTMLElement>(SELECTOR);
    return target && root.contains(target) ? target : null;
  };
  const enabled = (event: PointerEvent) => !motion.matches && finePointer.matches &&
    (event.pointerType === 'mouse' || event.pointerType === 'pen');

  /** Displacement strength and phase are derived from real stored positions.
   * No random rerolls or invented cursor samples during inverse playback.
   */
  function fieldAt(point: LensPoint): Field {
    return {
      // Keep displacement below ~6px even at the corner. Stronger
      // amplitude bends small text into visibly jagged, aliased edges.
      amplitude: 3.4 + Math.hypot(point.dx, point.dy) * 0.45,
      phaseX: point.dx * 2.4,
      phaseY: point.dy * 1.8,
      frequency: 0.014 + point.x * 0.00004,
    };
  }

  function kill() {
    if (restoreTimer !== null) {
      window.clearTimeout(restoreTimer);
      restoreTimer = null;
    }
    forward?.kill();
    reverse?.kill();
    forward = null;
    reverse = null;
  }

  function restore() {
    kill();
    if (painted) {
      painted.style.filter = priorFilter;
      painted.removeAttribute('data-tenet-warp-pixels');
    }
    if (active) {
      active.removeAttribute('data-tenet-warp-state');
    }
    painted = null;
    active = null;
    history = [];
    field.amplitude = 0;
    field.phaseX = 0;
    field.phaseY = 0;
    field.frequency = 0.016;
    warp?.apply(field);
  }

  function forwardTo(point: LensPoint, duration: number) {
    forward?.kill();
    forward = gsap.to(field, {
      ...fieldAt(point),
      duration, ease: 'power3.out', overwrite: true,
      onUpdate: () => warp?.apply(field),
    });
  }

  function onOver(event: PointerEvent) {
    if (!enabled(event)) return;
    const target = eligible(event);
    if (!target || target === active) return;
    restore();
    active = target;
    // Prefer a glyph-only layer: borders, separators and layout edges
    // remain crisp. Falling back to a whole control is reserved for
    // controls without a dedicated label.
    painted = target.querySelector<HTMLElement>('[data-tenet-lens-body]') ?? target;
    priorFilter = painted.style.filter;
    const rect = target.getBoundingClientRect();
    const point = lensPoint(event.clientX, event.clientY,
      rect.left, rect.top, rect.width, rect.height);
    history = rememberLensPoint([], point);
    painted.style.filter = 'url("#' + ensureWarp().id + '")';
    painted.dataset.tenetWarpPixels = 'true';
    target.dataset.tenetWarpState = 'forward';
    // Initial contact bends the real pixels; the material settles with
    // slight inertia as more cursor positions arrive.
    forwardTo(point, 0.34);
  }

  function onMove(event: PointerEvent) {
    if (!active || !enabled(event) || eligible(event) !== active) return;
    const rect = active.getBoundingClientRect();
    const point = lensPoint(event.clientX, event.clientY,
      rect.left, rect.top, rect.width, rect.height);
    history = rememberLensPoint(history, point);
    forwardTo(point, 0.22);
  }

  function onOut(event: PointerEvent) {
    if (!active || eligible(event) !== active) return;
    if (event.relatedTarget instanceof Node && active.contains(event.relatedTarget)) return;
    const reversePoints = inverseLensPath(history);
    kill();
    active.dataset.tenetWarpState = 'inverted';
    reverse = gsap.timeline({ onComplete: restore });
    // Reconstruct the cursor's exact recorded sequence in reverse; each
    // segment has a continuous material field and a shared temporal cursor.
    for (const point of reversePoints) {
      reverse.to(field, {
        ...fieldAt(point), duration: 0.025, ease: 'none',
        onUpdate: () => warp?.apply(field),
      });
    }
    reverse.to(field, {
      amplitude: 0, phaseX: 0, phaseY: 0, frequency: 0.016,
      duration: 0.20, ease: 'power3.out',
      onUpdate: () => warp?.apply(field),
    });
    // If WebKitGTK throttles animation frames, the control still recovers.
    restoreTimer = window.setTimeout(restore, 750);
  }

  const onEnvironmentChange = () => {
    if (motion.matches || !finePointer.matches) restore();
  };
  const onDown = () => restore();
  const onFocus = () => restore();
  root.addEventListener('pointerover', onOver);
  root.addEventListener('pointermove', onMove);
  root.addEventListener('pointerout', onOut);
  root.addEventListener('pointerdown', onDown);
  root.addEventListener('focusin', onFocus);
  motion.addEventListener('change', onEnvironmentChange);
  finePointer.addEventListener('change', onEnvironmentChange);
  return {
    destroy() {
      root.removeEventListener('pointerover', onOver);
      root.removeEventListener('pointermove', onMove);
      root.removeEventListener('pointerout', onOut);
      root.removeEventListener('pointerdown', onDown);
      root.removeEventListener('focusin', onFocus);
      motion.removeEventListener('change', onEnvironmentChange);
      finePointer.removeEventListener('change', onEnvironmentChange);
      restore();
      warp?.svg.remove();
      warp = null;
    },
  };
}
