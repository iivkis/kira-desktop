import { describe, expect, it, vi } from 'vitest';
import { describeBootstrapError, startUi, whenDocumentReady } from './bootstrap';

function targetFixture() {
  const loading = { textContent: 'Loading...', remove: vi.fn() };
  let mounted = false;
  const target = {
    querySelector: vi.fn((selector: string) => {
      if (selector === '#kira-loading') return loading;
      if (selector === 'main.bar') return mounted ? {} : null;
      return null;
    }),
  } as unknown as HTMLElement;
  return { target, loading, markMounted: () => { mounted = true; } };
}

describe('Svelte bootstrap diagnostics', () => {
  it('keeps visible fallback when Svelte mount throws', () => {
    const { target, loading } = targetFixture();
    const logs: string[] = [];
    const result = startUi(target, () => {
      throw new TypeError('DOM API unavailable');
    }, (value) => logs.push(value));
    expect(result).toBe(false);
    expect(loading.textContent).toContain('TypeError: DOM API unavailable');
    expect(loading.remove).not.toHaveBeenCalled();
    expect(logs.join('\n')).toContain('bootstrap exception: TypeError: DOM API unavailable');
  });

  it('only removes the fallback after a rendered panel exists', () => {
    const { target, loading, markMounted } = targetFixture();
    const events: string[] = [];
    const result = startUi(target, markMounted, (value) => events.push(value));
    expect(result).toBe(true);
    expect(loading.remove).toHaveBeenCalledOnce();
    expect(events).toContain('bootstrap: Svelte mount and flush returned');
  });

  it('reports synchronous mounting that did not produce the panel', () => {
    const { target, loading } = targetFixture();
    const logs: string[] = [];
    expect(startUi(target, () => undefined, (value) => logs.push(value))).toBe(false);
    expect(logs.join(' ')).toContain('main.bar is missing');
    expect(loading.remove).not.toHaveBeenCalled();
  });

  it('reports errors with usable stack traces', () => {
    const error = new Error('bad mount');
    expect(describeBootstrapError(error)).toContain('bad mount');
    expect(describeBootstrapError(error)).toContain('stack=');
    expect(describeBootstrapError('failure')).toBe('failure');
  });
});

describe('Classic script bootstrap timing', () => {
  it('waits for the DOMContentLoaded event if body is still parsing', () => {
    const listeners = new Map<string, () => void>();
    const addEventListener = vi.fn((event: string, fn: () => void) => {
      listeners.set(event, fn);
    });
    const page = {
      readyState: 'loading',
      addEventListener,
    } as unknown as Pick<Document, 'readyState' | 'addEventListener'>;
    const start = vi.fn();
    whenDocumentReady(page, start);
    expect(start).not.toHaveBeenCalled();
    expect(addEventListener).toHaveBeenCalledWith('DOMContentLoaded', start, { once: true });
    listeners.get('DOMContentLoaded')?.();
    expect(start).toHaveBeenCalledOnce();
  });

  it('mounts immediately when the document has already loaded', () => {
    const addEventListener = vi.fn();
    const page = {
      readyState: 'complete',
      addEventListener,
    } as unknown as Pick<Document, 'readyState' | 'addEventListener'>;
    const start = vi.fn();
    whenDocumentReady(page, start);
    expect(start).toHaveBeenCalledOnce();
    expect(addEventListener).not.toHaveBeenCalled();
  });
});
