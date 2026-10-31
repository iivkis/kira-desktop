/** Human-readable synchronous bootstrap failure, even when WebKit window.onerror is opaque. */
export function describeBootstrapError(error: unknown): string {
  if (error instanceof Error) {
    return [error.name + ': ' + error.message, error.stack].filter(Boolean).join(' | stack=');
  }
  if (error && typeof error === 'object') {
    const record = error as Record<string, unknown>;
    const message = typeof record.message === 'string' ? record.message : String(error);
    const stack = typeof record.stack === 'string' ? record.stack : '';
    return [message, stack].filter(Boolean).join(' | stack=');
  }
  return String(error);
}

/**
 * Keep the visible fallback until Svelte has rendered. Report every milestone.
 * Errors thrown inside a bundled inline script are caught here, rather than
 * relying on WebKit's opaque "Script error." window event.
 */
export function startUi(
  target: HTMLElement | null,
  mountAndFlush: (target: HTMLElement) => void,
  report: (message: string) => void,
): boolean {
  if (!target) {
    report('bootstrap failure: #app not found; readyState=' + document.readyState);
    return false;
  }

  const fallback = target.querySelector('#kira-loading');
  report('bootstrap: target found; fallback=' + Boolean(fallback));
  try {
    report('bootstrap: entering Svelte mount');
    mountAndFlush(target);
    report('bootstrap: Svelte mount and flush returned');
    // A semantic mount marker is independent of Tailwind / component CSS.
    if (!target.querySelector('main[data-kira-ui-root]')) {
      throw new Error('Svelte mount returned but main[data-kira-ui-root] is missing after flushSync');
    }
    fallback?.remove();
    return true;
  } catch (error) {
    const detail = describeBootstrapError(error);
    if (typeof window !== 'undefined') {
      const boot = Reflect.get(window, '__kiraBoot');
      if (boot && typeof boot === 'object') Reflect.set(boot, 'bootstrapError', detail);
    }
    report('bootstrap exception: ' + detail);
    if (fallback) {
      fallback.textContent = 'Kira UI error: ' + detail.slice(0, 220);
    }
    return false;
  }
}

/**
 * Vite emits module scripts in <head> with deferred semantics. Our build
 * inlines that bundle as a classic script, which executes immediately.
 * Wait for DOMContentLoaded so #app exists before Svelte's mount().
 */
export function whenDocumentReady(
  page: Pick<Document, 'readyState' | 'addEventListener'>,
  start: () => void,
): void {
  if (page.readyState === 'loading') {
    page.addEventListener('DOMContentLoaded', start, { once: true });
  } else {
    start();
  }
}
