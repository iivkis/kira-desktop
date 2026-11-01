import { afterEach, describe, expect, it, vi } from 'vitest';
import { deliverReply, request, focusWorkspace, onSystemMetrics, onWorkspaceUpdate, onConfigUpdate, toggleCalendar, toggleSystemPanel, openLauncher, closeCalendarOverlay, copyCalendarDate, notifyCalendarHostReady, notifyCalendarMounted, notifyOverlayHeight, closeSystemOverlay, copySystemReport, setSystemMonitorTab, onSystemOverlayData, onSystemPulse, onSystemProcesses, requestLauncherCatalog, launchDesktopApp, closeLauncherOverlay, onLauncherCatalog, onLauncherError, onLauncherIcons, requestLauncherIcons } from './core';

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

  it('reports only bounded positive widget heights scoped to the current generation', () => {
    const postMessage = vi.fn();
    globalThis.window = {
      webkit: { messageHandlers: { kiraCalendarUi: { postMessage } } },
    } as unknown as Window & typeof globalThis;
    expect(notifyOverlayHeight(3, 420)).toBe(true);
    for (const generation of [0, -1, 1.5, Number.NaN, 4294967296]) {
      expect(notifyOverlayHeight(generation, 210)).toBe(false);
    }
    for (const height of [0, -1, 1.5, 20001, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(notifyOverlayHeight(3, height)).toBe(false);
    }
    expect(postMessage.mock.calls.map(call => call[0])).toEqual(['size:3:420']);
  });

  it('allows only generation-scoped indexed Gio launches, no path or shell command', () => {
    const postMessage = vi.fn();
    globalThis.window = {
      webkit: { messageHandlers: { kiraCalendarUi: { postMessage } } },
    } as unknown as Window & typeof globalThis;
    closeLauncherOverlay();
    expect(requestLauncherCatalog(8)).toBe(true);
    expect(launchDesktopApp(8, 42)).toBe(true);
    for (const bad of [-1, 1.5, 1024, Number.NaN]) {
      expect(launchDesktopApp(8, bad)).toBe(false);
    }
    expect(launchDesktopApp(0, 1)).toBe(false);
    expect(launchDesktopApp(4294967296, 1)).toBe(false);
    expect(requestLauncherCatalog(-1)).toBe(false);
    expect(postMessage.mock.calls.map(call => call[0])).toEqual([
      'close', 'apps-request:8', 'launch:8:42',
    ]);
  });

  it('requests only bounded catalog icon IDs, never arbitrary paths', () => {
    const postMessage = vi.fn();
    globalThis.window = {
      webkit: { messageHandlers: { kiraCalendarUi: { postMessage } } },
    } as unknown as Window & typeof globalThis;
    expect(requestLauncherIcons(42, [0, 7, 1023])).toBe(true);
    expect(requestLauncherIcons(42, [])).toBe(false);
    expect(requestLauncherIcons(42, Array.from({ length: 17 }, (_, i) => i))).toBe(false);
    expect(requestLauncherIcons(42, [1, 1])).toBe(false);
    expect(requestLauncherIcons(0, [1])).toBe(false);
    expect(requestLauncherIcons(42, [-1])).toBe(false);
    expect(requestLauncherIcons(42, [1024])).toBe(false);
    expect(requestLauncherIcons(42, [1.5])).toBe(false);
    expect(postMessage.mock.calls.map(call => call[0])).toEqual(['icons-request:42:0,7,1023']);
  });

  it('accepts image-only data URI batches and rejects paths, web URLs or oversized SVG', () => {
    const handlers = new Map<string, EventListener>();
    globalThis.window = {
      addEventListener(name: string, listener: EventListener) { handlers.set(name, listener); },
      removeEventListener: vi.fn(),
    } as unknown as Window & typeof globalThis;
    const callback = vi.fn();
    const stop = onLauncherIcons(callback);
    const send = (batch: unknown) => handlers.get('kira:launcher:icons')!({ detail: batch } as unknown as Event);
    const valid = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==';
    send({ icons: [{ id: 7, data_url: valid }, { id: 8, data_url: null }] });
    expect(callback).toHaveBeenCalledOnce();
    for (const invalid of ['file:///etc/passwd', 'https://example.com/icon.png',
      'javascript:alert(1)', 'data:text/html;base64,PHNjcmlwdD4=',
      'data:image/svg+xml;base64,' + 'A'.repeat(180_000)]) {
      send({ icons: [{ id: 7, data_url: invalid }] });
    }
    send({ icons: [{ id: 1024, data_url: valid }] });
    send({ icons: Array.from({ length: 17 }, (_, id) => ({ id, data_url: valid })) });
    expect(callback).toHaveBeenCalledOnce();
    stop();
  });

  it('validates launcher catalogs, errors and unsubscribes after hide', () => {
    const handlers = new Map<string, EventListener>();
    const removeEventListener = vi.fn();
    globalThis.window = {
      addEventListener(name: string, listener: EventListener) { handlers.set(name, listener); },
      removeEventListener,
    } as unknown as Window & typeof globalThis;
    const onCatalog = vi.fn();
    const onError = vi.fn();
    const stopCatalog = onLauncherCatalog(onCatalog);
    const stopError = onLauncherError(onError);
    const dispatch = (name: string, detail: unknown) => {
      handlers.get(name)!({ detail } as unknown as Event);
    };
    dispatch('kira:launcher:apps', {
      apps: [{ id: 7, name: 'Browser', description: 'Open the web' }],
    });
    expect(onCatalog).toHaveBeenCalledOnce();
    dispatch('kira:launcher:apps', { apps: [{ id: 1024, name: 'Unknown', description: '' }] });
    dispatch('kira:launcher:apps', { apps: [{ id: 1, name: 'x'.repeat(121), description: '' }] });
    expect(onCatalog).toHaveBeenCalledOnce();
    dispatch('kira:launcher:error', { message: 'Cannot open application' });
    expect(onError).toHaveBeenCalledWith('Cannot open application');
    stopCatalog();
    stopError();
    expect(removeEventListener).toHaveBeenCalledTimes(2);
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

  it('exposes only close and validated date-copy to the calendar WebKit host', () => {
    const postMessage = vi.fn();
    globalThis.window = {
      webkit: { messageHandlers: { kiraCalendarUi: { postMessage } } },
    } as unknown as Window & typeof globalThis;
    closeCalendarOverlay();
    copyCalendarDate('2024-02-29');
    for (const invalid of ['bogus', '2024-2-29', '2024-02-29;exit', '', '99999-01-01']) {
      copyCalendarDate(invalid);
    }
    expect(postMessage.mock.calls.map((call) => call[0])).toEqual(['close', 'copy:2024-02-29']);
  });

  it('reports calendar Svelte mount completion only via the private host channel', () => {
    const postMessage = vi.fn();
    globalThis.window = {
      webkit: { messageHandlers: { kiraCalendarUi: { postMessage } } },
    } as unknown as Window & typeof globalThis;
    notifyCalendarHostReady();
    notifyCalendarMounted(42);
    notifyCalendarMounted(0);
    notifyCalendarMounted(-1);
    notifyCalendarMounted(Number.NaN);
    notifyCalendarMounted(4294967296);
    expect(postMessage.mock.calls.map((call) => call[0])).toEqual(['host-ready', 'mounted:42']);
  });

  it('does not throw without a calendar host, or when its channel fails', () => {
    expect(() => closeCalendarOverlay()).not.toThrow();
    expect(() => copyCalendarDate('2024-02-29')).not.toThrow();
    globalThis.window = {
      webkit: { messageHandlers: { kiraCalendarUi: { postMessage: () => { throw new Error('window closed'); } } } },
    } as unknown as Window & typeof globalThis;
    expect(() => closeCalendarOverlay()).not.toThrow();
    expect(() => copyCalendarDate('2024-02-29')).not.toThrow();
  });

  it('allows only fixed System Monitor overlay actions', () => {
    const postMessage = vi.fn();
    globalThis.window = {
      webkit: { messageHandlers: { kiraCalendarUi: { postMessage } } },
    } as unknown as Window & typeof globalThis;
    closeSystemOverlay();
    copySystemReport();
    setSystemMonitorTab('overview');
    setSystemMonitorTab('processes');
    setSystemMonitorTab('execute' as 'processes');
    expect(postMessage.mock.calls.map(call => call[0])).toEqual([
      'close', 'copy-report', 'tab:overview', 'tab:processes',
    ]);
  });

  it('accepts bounded native 100ms pulse events and cleans up listeners', () => {
    const events = new Map<string, EventListener>();
    const removeEventListener = vi.fn();
    globalThis.window = {
      addEventListener(name: string, listener: EventListener) { events.set(name, listener); },
      removeEventListener,
    } as unknown as Window & typeof globalThis;
    const callback = vi.fn();
    const stop = onSystemPulse(callback);
    const dispatch = (detail: unknown) =>
      events.get('kira:system:pulse')!({ detail } as unknown as Event);
    const sample = { cpu_percent: 4, memory_total_kib: 1000, memory_available_kib: 800 };
    dispatch(sample);
    expect(callback).toHaveBeenCalledExactlyOnceWith(sample);
    for (const invalid of [
      { ...sample, cpu_percent: 101 },
      { ...sample, cpu_percent: -1 },
      { ...sample, cpu_percent: NaN },
      { ...sample, memory_total_kib: 1.5 },
      { ...sample, memory_total_kib: -100 },
      { ...sample, memory_available_kib: 1001 },
      { ...sample, memory_available_kib: Infinity },
      { ...sample, memory_available_kib: '800' },
      { ...sample, cpu_percent: undefined },
    ]) dispatch(invalid);
    expect(callback).toHaveBeenCalledTimes(1);
    dispatch({ ...sample, cpu_percent: null, memory_total_kib: null, memory_available_kib: null });
    expect(callback).toHaveBeenCalledTimes(2);
    stop();
    expect(removeEventListener).toHaveBeenCalledOnce();
    expect(removeEventListener).toHaveBeenCalledWith('kira:system:pulse', expect.any(Function));
  });

  it('delivers only valid read-only monitor metrics and bounded processes', () => {
    const events = new Map<string, EventListener>();
    const removeEventListener = vi.fn();
    globalThis.window = {
      addEventListener(name: string, callback: EventListener) { events.set(name, callback); },
      removeEventListener,
    } as unknown as Window & typeof globalThis;
    const onData = vi.fn();
    const onProcesses = vi.fn();
    const stopData = onSystemOverlayData(onData);
    const stopProcesses = onSystemProcesses(onProcesses);
    const metrics = {
      cpu_percent: 23, cpu_temperature_c: 61.5,
      gpus: [{ label: 'iGPU / Intel (card0)', utilization_percent: 12, temperature_c: 46.7 }],
      memory_total_kib: 100, memory_available_kib: 70,
      swap_total_kib: 0, swap_free_kib: 0, root_total_kib: 100, root_available_kib: 40,
      uptime_seconds: 3600, load: [1, 2, 3], battery_percent: null, battery_charging: null,
    };
    const dispatch = (name: string, detail: unknown) => {
      events.get(name)!({ detail } as unknown as Event);
    };
    dispatch('kira:system:data', { core: true, niri: true, metrics });
    expect(onData).toHaveBeenCalledOnce();
    dispatch('kira:system:data', { core: true, niri: true, metrics: { ...metrics, load: [NaN, 0, 1] } });
    dispatch('kira:system:data', { core: true, niri: true, metrics: { ...metrics, gpus: [{ ...metrics.gpus[0], utilization_percent: 101 }] } });
    dispatch('kira:system:data', { core: true, niri: true, metrics: { ...metrics, cpu_temperature_c: 200 } });
    expect(onData).toHaveBeenCalledOnce();
    const process = {
      rows: [{ pid: 123, name: 'shell', state: 'S', cpu_percent: 1, rss_kib: 1024, pss_kib: 768 }],
      total: 1, truncated: false, error: null,
    };
    dispatch('kira:system:processes', process);
    expect(onProcesses).toHaveBeenCalledOnce();
    dispatch('kira:system:processes', { ...process, rows: [{ ...process.rows[0], name: 'x'.repeat(65) }] });
    dispatch('kira:system:processes', { ...process, rows: [{ ...process.rows[0], cpu_percent: 200 }] });
    dispatch('kira:system:processes', { ...process, rows: [{ ...process.rows[0], pss_kib: -1 }] });
    expect(onProcesses).toHaveBeenCalledOnce();
    stopData();
    stopProcesses();
    expect(removeEventListener).toHaveBeenCalledTimes(2);
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
      cpu_percent: 42, cpu_temperature_c: null, gpus: [],
      memory_total_kib: 16000, memory_available_kib: 8000,
      root_total_kib: 20000, root_available_kib: 12000,
      battery_percent: null, battery_charging: null,
    };
    for (const invalid of [
      null, { ...metrics, cpu_percent: 120 }, { ...metrics, cpu_percent: -1 },
      { ...metrics, root_available_kib: Number.NaN }, { ...metrics, battery_percent: 101 },
      { ...metrics, battery_charging: 'unknown' },
      { ...metrics, gpus: [{ label: 'bad', utilization_percent: NaN, temperature_c: null }] },
      { ...metrics, cpu_temperature_c: Infinity },
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
