import { afterEach, describe, expect, it, vi } from 'vitest';
import { deliverReply, request, focusWorkspace, onSystemMetrics, onWorkspaceUpdate, onConfigUpdate, toggleCalendar, toggleSystemPanel, openLauncher } from './core';

const originalWindow = globalThis.window;
afterEach(() => {
  vi.useRealTimers();
  globalThis.window = originalWindow;
});

describe('Core SDK JSON-RPC bridge', () => {
  it('opens Launcher via its fixed command and reports missing or failing host bridge', () => {
    expect(openLauncher()).toBe(false);
    const postMessage = vi.fn();
    globalThis.window = {
      webkit: { messageHandlers: { kiraLauncher: { postMessage } } },
    } as unknown as Window & typeof globalThis;
    expect(openLauncher()).toBe(true);
    expect(postMessage).toHaveBeenCalledExactlyOnceWith('open');
    postMessage.mockImplementation(() => { throw new Error('host disconnected'); });
    expect(openLauncher()).toBe(false);
  });

  it('opens native read-only system monitor only through the fixed bridge', () => {
    expect(toggleSystemPanel()).toBe(false);
    const postMessage = vi.fn();
    globalThis.window = {
      webkit: { messageHandlers: { kiraSystem: { postMessage } } },
    } as unknown as Window & typeof globalThis;
    expect(toggleSystemPanel()).toBe(true);
    expect(postMessage).toHaveBeenCalledWith('toggle');
    postMessage.mockImplementation(() => { throw new Error('gone'); });
    expect(toggleSystemPanel()).toBe(false);
  });

  it('toggles the calendar via a dedicated fixed-purpose host bridge', () => {
    expect(toggleCalendar()).toBe(false);
    const postMessage = vi.fn();
    globalThis.window = {
      webkit: { messageHandlers: { kiraCalendar: { postMessage } } },
    } as unknown as Window & typeof globalThis;
    expect(toggleCalendar()).toBe(true);
    expect(postMessage).toHaveBeenCalledExactlyOnceWith('toggle');
  });

  it('does not claim calendar activation when the bridge throws', () => {
    globalThis.window = {
      webkit: { messageHandlers: { kiraCalendar: { postMessage: () => { throw new Error('closed'); } } } },
    } as unknown as Window & typeof globalThis;
    expect(toggleCalendar()).toBe(false);
  });

  it('rejects outside the host bridge', async () => {
    await expect(request('ping')).rejects.toThrow('bridge unavailable');
  });

  it('correlates responses by request id', async () => {
    const postMessage = vi.fn();
    // Browser-like mock for the native WebKit postMessage API.
    globalThis.window = { webkit: { messageHandlers: { kira: { postMessage } } } } as unknown as Window & typeof globalThis;
    const result = request('get_state');
    const payload = JSON.parse(postMessage.mock.calls[0][0] as string) as { id: number; method: string };
    expect(payload.method).toBe('get_state');
    deliverReply({ jsonrpc: '2.0', id: payload.id, result: { version: '0.1.0', status: 'bootstrap' } });
    await expect(result).resolves.toEqual({ version: '0.1.0', status: 'bootstrap' });
  });

  it('requests typed real Niri workspaces with JS-safe IDs', async () => {
    const postMessage = vi.fn();
    globalThis.window = { webkit: { messageHandlers: { kira: { postMessage } } } } as unknown as Window & typeof globalThis;
    const result = request('get_workspaces');
    const payload = JSON.parse(postMessage.mock.calls[0][0] as string) as { id: number; method: string };
    expect(payload.method).toBe('get_workspaces');
    const workspace = {
      id: '18446744073709551615', idx: 2, name: 'code', output: 'DP-1',
      is_active: true, is_focused: false, is_urgent: true,
    };
    deliverReply({
      jsonrpc: '2.0', id: payload.id,
      result: { connected: true, workspaces: [workspace] },
    });
    await expect(result).resolves.toEqual({ connected: true, workspaces: [workspace] });
  });

  it('sends a single focus action by stable string id, never index or unsafe JS number', async () => {
    const postMessage = vi.fn();
    globalThis.window = { webkit: { messageHandlers: { kira: { postMessage } } } } as unknown as Window & typeof globalThis;
    const pending = focusWorkspace('18446744073709551615');
    const payload = JSON.parse(postMessage.mock.calls[0][0] as string) as {
      jsonrpc: string; id: number; method: string; params: { id: string };
    };
    expect(payload).toEqual({
      jsonrpc: '2.0', id: expect.any(Number),
      method: 'focus_workspace', params: { id: '18446744073709551615' },
    });
    expect(payload.params.id).toBe('18446744073709551615');
    deliverReply({ jsonrpc: '2.0', id: payload.id, result: { accepted: true } });
    await expect(pending).resolves.toBeUndefined();
  });

  it('rejects malformed workspace IDs without sending any native request', async () => {
    const postMessage = vi.fn();
    globalThis.window = { webkit: { messageHandlers: { kira: { postMessage } } } } as unknown as Window & typeof globalThis;
    for (const id of ['-1', '01', '', 'code', '18446744073709551616', '3;quit']) {
      await expect(focusWorkspace(id)).rejects.toThrow('Invalid workspace ID');
    }
    expect(postMessage).not.toHaveBeenCalled();
  });

  it('propagates Niri focus failures rather than optimistically changing workspace', async () => {
    const postMessage = vi.fn();
    globalThis.window = { webkit: { messageHandlers: { kira: { postMessage } } } } as unknown as Window & typeof globalThis;
    const pending = focusWorkspace('9');
    const payload = JSON.parse(postMessage.mock.calls[0][0] as string) as { id: number };
    deliverReply({ jsonrpc: '2.0', id: payload.id, error: { code: -32602, message: 'Unknown workspace id' } });
    await expect(pending).rejects.toThrow('Unknown workspace id');
  });

  it('delivers live workspace events and unsubscribes without polling', () => {
    const handlers = new Map<string, EventListener>();
    const addEventListener = vi.fn((name: string, listener: EventListener) => {
      handlers.set(name, listener);
    });
    const removeEventListener = vi.fn();
    globalThis.window = { addEventListener, removeEventListener } as unknown as Window & typeof globalThis;
    const onChange = vi.fn();
    const stop = onWorkspaceUpdate(onChange);
    const handler = handlers.get('kira:workspaces');
    expect(handler).toBeDefined();
    handler!({ detail: { connected: true, workspaces: [{
      id: '33', idx: 2, name: null, output: 'winit',
      is_active: true, is_focused: true, is_urgent: false,
    }] } } as unknown as Event);
    expect(onChange).toHaveBeenCalledOnce();
    expect(onChange.mock.calls[0][0].workspaces[0].idx).toBe(2);
    handler!({ detail: { workspaces: [] } } as unknown as Event);
    expect(onChange).toHaveBeenCalledOnce();
    stop();
    expect(removeEventListener).toHaveBeenCalledWith('kira:workspaces', handler);
  });

  it('validates native metrics, delivers valid samples and unsubscribes', () => {
    const handlers = new Map<string, EventListener>();
    const addEventListener = vi.fn((name: string, listener: EventListener) => handlers.set(name, listener));
    const removeEventListener = vi.fn();
    globalThis.window = { addEventListener, removeEventListener } as unknown as Window & typeof globalThis;
    const changed = vi.fn();
    const stop = onSystemMetrics(changed);
    const listener = handlers.get('kira:system-metrics');
    expect(listener).toBeDefined();
    const metrics = {
      cpu_percent: 42, memory_total_kib: 16000, memory_available_kib: 8000,
      root_total_kib: 20000, root_available_kib: 12000,
      battery_percent: null, battery_charging: null,
    };
    for (const invalid of [
      null, { ...metrics, cpu_percent: 120 }, { ...metrics, cpu_percent: -1 },
      { ...metrics, root_available_kib: Number.NaN }, { ...metrics, battery_percent: 101 },
      { ...metrics, battery_charging: 'unknown' },
    ]) {
      listener!({ detail: invalid } as unknown as Event);
    }
    expect(changed).not.toHaveBeenCalled();
    listener!({ detail: metrics } as unknown as Event);
    expect(changed).toHaveBeenCalledExactlyOnceWith(metrics);
    stop();
    expect(removeEventListener).toHaveBeenCalledWith('kira:system-metrics', listener);
  });

  it('returns an inert event unsubscribe without a browser window', () => {
    globalThis.window = undefined as unknown as Window & typeof globalThis;
    expect(() => onSystemMetrics(vi.fn())()).not.toThrow();
    expect(() => onWorkspaceUpdate(vi.fn())()).not.toThrow();
    expect(() => onConfigUpdate(vi.fn())()).not.toThrow();
  });

  it('reads validated config state through read-only JSON-RPC', async () => {
    const postMessage = vi.fn();
    globalThis.window = { webkit: { messageHandlers: { kira: { postMessage } } } } as unknown as Window & typeof globalThis;
    const pending = request('get_config');
    const data = JSON.parse(postMessage.mock.calls[0][0] as string) as { id: number; method: string };
    expect(data.method).toBe('get_config');
    const config = { compact: true, title: 'KIRA QA', show_seconds: true, show_names: false, max_workspaces: 3 };
    deliverReply({ jsonrpc: '2.0', id: data.id, result: { config, error: null } });
    await expect(pending).resolves.toEqual({ config, error: null });
  });

  it('applies pushed KDL configuration, rejects malformed updates and unsubscribes', () => {
    const handlers = new Map<string, EventListener>();
    const addEventListener = vi.fn((name: string, listener: EventListener) => handlers.set(name, listener));
    const removeEventListener = vi.fn();
    globalThis.window = { addEventListener, removeEventListener } as unknown as Window & typeof globalThis;
    const changed = vi.fn();
    const stop = onConfigUpdate(changed);
    const listener = handlers.get('kira:config');
    expect(listener).toBeDefined();
    const config = { compact: true, title: 'Panel', show_seconds: true, show_names: false, max_workspaces: 8 };
    listener!({ detail: { config, error: null } } as unknown as Event);
    expect(changed).toHaveBeenCalledOnce();
    listener!({ detail: { config: { ...config, max_workspaces: 0 }, error: null } } as unknown as Event);
    expect(changed).toHaveBeenCalledOnce();
    stop();
    expect(removeEventListener).toHaveBeenCalledWith('kira:config', listener);
  });

  it('rejects Core errors', async () => {
    const postMessage = vi.fn();
    globalThis.window = { webkit: { messageHandlers: { kira: { postMessage } } } } as unknown as Window & typeof globalThis;
    const result = request('ping');
    const payload = JSON.parse(postMessage.mock.calls[0][0] as string) as { id: number };
    deliverReply({ jsonrpc: '2.0', id: payload.id, error: { code: -32000, message: 'unavailable' } });
    await expect(result).rejects.toThrow('unavailable');
  });
});
