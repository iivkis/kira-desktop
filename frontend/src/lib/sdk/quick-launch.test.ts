import { describe, expect, it, vi } from 'vitest';
import {
  launchQuickApp, onQuickLaunchApps, onQuickLaunchIcons,
  requestQuickLaunchCatalog, requestQuickLaunchIcons,
} from './core';

describe('Quick Launch narrow trusted bridge', () => {
  it('sends only bounded native indices, never command strings', () => {
    const postMessage = vi.fn();
    globalThis.window = {
      webkit: { messageHandlers: { kiraQuickLaunch: { postMessage } } },
    } as unknown as Window & typeof globalThis;
    expect(requestQuickLaunchCatalog()).toBe(true);
    expect(requestQuickLaunchIcons([8, 2])).toBe(true);
    expect(launchQuickApp(8)).toBe(true);
    expect(postMessage.mock.calls.map(([message]) => message))
      .toEqual(['catalog', 'icons:8,2', 'launch:8']);
    for (const id of [-1, 1.5, 1024, Number.NaN]) {
      expect(launchQuickApp(id)).toBe(false);
    }
    expect(requestQuickLaunchIcons([])).toBe(false);
    expect(requestQuickLaunchIcons([1, 1])).toBe(false);
    expect(requestQuickLaunchIcons([1, 2, 3, 4])).toBe(false);
    expect(postMessage).toHaveBeenCalledTimes(3);
  });

  it('validates tiny catalog and image-only event payloads', () => {
    const handlers = new Map<string, EventListener>();
    const removeEventListener = vi.fn();
    globalThis.window = {
      addEventListener(name: string, listener: EventListener) { handlers.set(name, listener); },
      removeEventListener,
    } as unknown as Window & typeof globalThis;
    const onApps = vi.fn();
    const onIcons = vi.fn();
    const stopApps = onQuickLaunchApps(onApps);
    const stopIcons = onQuickLaunchIcons(onIcons);
    const notify = (name: string, detail: unknown) =>
      handlers.get(name)!({ detail } as unknown as Event);
    const app = { id: 8, name: 'Terminal', description: '' };
    notify('kira:quick:apps', { apps: [app] });
    expect(onApps).toHaveBeenCalledExactlyOnceWith([app]);
    notify('kira:quick:apps', { apps: [app, app, app, app] });
    notify('kira:quick:apps', { apps: [{ ...app, id: -1 }] });
    expect(onApps).toHaveBeenCalledTimes(1);
    notify('kira:quick:icons', { icons: [{ id: 8, data_url: null }] });
    notify('kira:quick:icons', { icons: [{ id: 8, data_url: 'file:///tmp/bad' }] });
    expect(onIcons).toHaveBeenCalledTimes(1);
    stopApps();
    stopIcons();
    expect(removeEventListener).toHaveBeenCalledTimes(2);
  });
});
