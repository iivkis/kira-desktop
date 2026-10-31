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
    if (!target.querySelector('main.bar')) {
      throw new Error('Svelte mount returned but main.bar is missing after flushSync');
    }
    fallback?.remove();
    return true;
  } catch (error) {
    const detail = describeBootstrapError(error);
    report('bootstrap exception: ' + detail);
    if (fallback) {
      fallback.textContent = 'Kira UI error: ' + detail.slice(0, 220);
    }
    return false;
  }
}
