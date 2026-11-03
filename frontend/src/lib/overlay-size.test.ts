import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { observeOverlaySize } from './overlay-size';
import { notifyOverlayHeight } from './sdk/core';

vi.mock('./sdk/core', () => ({ notifyOverlayHeight: vi.fn() }));

afterEach(() => vi.unstubAllGlobals());
beforeEach(() => vi.mocked(notifyOverlayHeight).mockReset());

describe('native popup sizing with outer neo-brutalist shadow', () => {
  it('includes the transparent shadow gutter in the first, synchronous size report', () => {
    vi.stubGlobal('ResizeObserver', undefined);
    vi.stubGlobal('getComputedStyle', () => ({ paddingTop: '8px', paddingBottom: '8px' }));
    const root = {
      isConnected: true,
      parentElement: {},
      getBoundingClientRect: () => ({ height: 320.5 }),
    } as unknown as HTMLElement;
    const stop = observeOverlaySize(root, 7);
    expect(notifyOverlayHeight).toHaveBeenCalledExactlyOnceWith(7, 337);
    stop();
  });

  it('uses no gutter for detached windows and disconnects its observer on hide', () => {
    let resize: () => void = () => {};
    const disconnect = vi.fn();
    class Observer {
      constructor(callback: ResizeObserverCallback) { resize = () => callback([], this as unknown as ResizeObserver); }
      observe() {}
      disconnect() { disconnect(); }
    }
    vi.stubGlobal('ResizeObserver', Observer);
    vi.stubGlobal('getComputedStyle', () => ({ paddingTop: '0px', paddingBottom: '0px' }));
    let contentHeight = 410;
    const root = {
      isConnected: true,
      parentElement: {},
      getBoundingClientRect: () => ({ height: contentHeight }),
    } as unknown as HTMLElement;
    const stop = observeOverlaySize(root, 9);
    expect(notifyOverlayHeight).toHaveBeenCalledExactlyOnceWith(9, 410);
    contentHeight = 412;
    resize();
    resize(); // Unchanged geometry must not re-notify GTK.
    expect(notifyOverlayHeight).toHaveBeenCalledTimes(2);
    expect(notifyOverlayHeight).toHaveBeenLastCalledWith(9, 412);
    stop();
    expect(disconnect).toHaveBeenCalledOnce();
    contentHeight = 415;
    resize();
    expect(notifyOverlayHeight).toHaveBeenCalledTimes(2);
  });
});
