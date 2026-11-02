import { notifyOverlayHeight } from './sdk/core';

/**
 * Svelte's intrinsic root height, not the WebKit viewport, drives the GTK
 * layer-shell surface. The CSS max-height is a native monitor-sized pixel
 * cap, so resizing the viewport cannot make the measurement oscillate.
 */
export function observeOverlaySize(root: HTMLElement, generation: number): () => void {
  let lastHeight = 0;
  let stopped = false;
  const measure = () => {
    if (stopped || !root.isConnected) return;
    // <main> excludes the transparent bottom strip used by the hard popup
    // shadow. Report the whole painted WebKit surface so GTK does not clip
    // its last pixels during the TENET entrance or final settled frame.
    const bottomInset = root.parentElement
      ? Number.parseFloat(getComputedStyle(root.parentElement).paddingBottom) || 0
      : 0;
    const height = Math.ceil(root.getBoundingClientRect().height + Math.max(0, bottomInset));
    if (!Number.isSafeInteger(height) || height <= 0 || height > 20000 || height === lastHeight) return;
    lastHeight = height;
    notifyOverlayHeight(generation, height);
  };

  // The first size report is synchronous and precedes mounted:<generation>,
  // allowing GTK to set window geometry before the double-rAF reveal.
  measure();
  const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure);
  observer?.observe(root);

  return () => {
    stopped = true;
    observer?.disconnect();
  };
}
