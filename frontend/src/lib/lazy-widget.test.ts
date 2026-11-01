import { describe, expect, it, vi } from 'vitest';
import { createLazyWidget } from './lazy-widget';

const flush = async () => {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
};

describe('lazy overlay widget lifecycle', () => {
  it('does not load or mount a hidden widget, and disposes it on hide', async () => {
    const mount = vi.fn(() => ({ mounted: true }));
    const unmount = vi.fn(async () => {});
    const onMounted = vi.fn();
    const load = vi.fn(async () => ({ mount, unmount, onMounted }));
    const errors = vi.fn();
    const widget = createLazyWidget(load, errors);
    await flush();
    expect(load).not.toHaveBeenCalled();

    widget.show(1);
    await flush();
    expect(load).toHaveBeenCalledTimes(1);
    expect(mount).toHaveBeenCalledTimes(1);
    expect(onMounted).toHaveBeenCalledWith(1);

    widget.hide();
    await flush();
    expect(unmount).toHaveBeenCalledTimes(1);
    expect(unmount).toHaveBeenCalledWith({ mounted: true });
    expect(errors).not.toHaveBeenCalled();

    widget.show(2);
    await flush();
    expect(mount).toHaveBeenCalledTimes(2);
    expect(onMounted).toHaveBeenCalledWith(2);
  });

  it('does not mount a component loaded after the window was hidden', async () => {
    const mount = vi.fn(() => ({}));
    const unmount = vi.fn();
    const onMounted = vi.fn();
    let finish!: (widget: { mount: typeof mount; unmount: typeof unmount; onMounted: typeof onMounted }) => void;
    const load = vi.fn(() => new Promise<{
      mount: typeof mount; unmount: typeof unmount; onMounted: typeof onMounted
    }>((resolve) => { finish = resolve; }));
    const widget = createLazyWidget(load, vi.fn());
    widget.show(7);
    widget.hide();
    finish({ mount, unmount, onMounted });
    await flush();
    expect(mount).not.toHaveBeenCalled();
    expect(onMounted).not.toHaveBeenCalled();
  });

  it('serializes unmount before the next mount during rapid toggles', async () => {
    const sequence: string[] = [];
    let completeUnmount!: () => void;
    const endUnmount = new Promise<void>((resolve) => { completeUnmount = resolve; });
    const widget = createLazyWidget(async () => ({
      mount() { sequence.push('mount'); return {}; },
      unmount() { sequence.push('unmount'); return endUnmount; },
      onMounted() { sequence.push('ready'); },
    }), vi.fn());

    widget.show(1);
    await flush();
    widget.hide();
    widget.show(2);
    await flush();
    expect(sequence).toEqual(['mount', 'ready', 'unmount']);
    completeUnmount();
    await flush();
    expect(sequence).toEqual(['mount', 'ready', 'unmount', 'mount', 'ready']);
  });

  it('ignores invalid generations without loading and reports loader failures', async () => {
    const load = vi.fn(async () => { throw new Error('test loader failure'); });
    const report = vi.fn();
    const widget = createLazyWidget(load, report);
    for (const invalid of [0, -1, 1.5, Number.NaN, 4294967296]) widget.show(invalid);
    await flush();
    expect(load).not.toHaveBeenCalled();
    widget.show(4);
    await flush();
    expect(report).toHaveBeenCalledTimes(1);
    expect(String(report.mock.calls[0][0])).toContain('test loader failure');
  });
});
