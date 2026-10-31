import { afterEach, describe, expect, it, vi } from 'vitest';
import { deliverReply, request, onWorkspaceUpdate } from './core';

const originalWindow = globalThis.window;
afterEach(() => {
  vi.useRealTimers();
  globalThis.window = originalWindow;
});

describe('Core SDK JSON-RPC bridge', () => {
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

  it('rejects Core errors', async () => {
    const postMessage = vi.fn();
    globalThis.window = { webkit: { messageHandlers: { kira: { postMessage } } } } as unknown as Window & typeof globalThis;
    const result = request('ping');
    const payload = JSON.parse(postMessage.mock.calls[0][0] as string) as { id: number };
    deliverReply({ jsonrpc: '2.0', id: payload.id, error: { code: -32000, message: 'unavailable' } });
    await expect(result).rejects.toThrow('unavailable');
  });
});
