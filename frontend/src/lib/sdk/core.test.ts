import { afterEach, describe, expect, it, vi } from 'vitest';
import { deliverReply, request } from './core';

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

  it('rejects Core errors', async () => {
    const postMessage = vi.fn();
    globalThis.window = { webkit: { messageHandlers: { kira: { postMessage } } } } as unknown as Window & typeof globalThis;
    const result = request('ping');
    const payload = JSON.parse(postMessage.mock.calls[0][0] as string) as { id: number };
    deliverReply({ jsonrpc: '2.0', id: payload.id, error: { code: -32000, message: 'unavailable' } });
    await expect(result).rejects.toThrow('unavailable');
  });
});
