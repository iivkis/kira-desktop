/** Typed, request/response bridge for trusted Svelte UI modules. */
export interface CoreState {
  version: string;
  status: 'bootstrap';
}

export interface Workspace {
  /** Stable Niri u64 ID encoded as a string (JavaScript numbers lose precision). */
  id: string;
  /** 1-based workspace position on its own output, NOT a globally unique ID. */
  idx: number;
  name: string | null;
  output: string | null;
  is_active: boolean;
  is_focused: boolean;
  is_urgent: boolean;
}

export interface WorkspaceState {
  connected: boolean;
  workspaces: Workspace[];
}

/** Linux resource snapshot forwarded by the trusted native Shell.
 * Null means unavailable, never zero. Units are KiB and seconds.
 */
export interface SystemMetrics {
  cpu_percent: number | null;
  memory_total_kib: number | null;
  memory_available_kib: number | null;
  swap_total_kib: number | null;
  swap_free_kib: number | null;
  root_total_kib: number | null;
  root_available_kib: number | null;
  uptime_seconds: number | null;
  load: [number, number, number] | null;
  battery_percent: number | null;
  battery_charging: boolean | null;
}

/** Event-driven system metrics: does not add browser polling or an RPC method. */
export function onSystemMetrics(callback: (metrics: SystemMetrics) => void): () => void {
  if (typeof window === 'undefined') return () => {};
  const listener = (event: Event) => {
    const value = (event as CustomEvent<SystemMetrics>).detail;
    const valid = (n: unknown) => n === null ||
      (typeof n === 'number' && Number.isFinite(n) && n >= 0);
    if (!value || typeof value !== 'object' ||
        !valid(value.cpu_percent) || !valid(value.memory_total_kib) ||
        !valid(value.memory_available_kib) || !valid(value.root_total_kib) ||
        !valid(value.root_available_kib) ||
        (value.cpu_percent !== null && value.cpu_percent > 100) ||
        (value.battery_percent !== null && (!valid(value.battery_percent) || value.battery_percent > 100)) ||
        ![null, true, false].includes(value.battery_charging)) return;
    callback(value);
  };
  window.addEventListener('kira:system-metrics', listener);
  return () => window.removeEventListener('kira:system-metrics', listener);
}

/** Listen for typed workspace push events emitted by the native Shell.
 * No Niri or Core polling is performed here. Unsubscribe on Svelte unmount.
 */
export function onWorkspaceUpdate(callback: (state: WorkspaceState) => void): () => void {
  if (typeof window === 'undefined') return () => {};
  const listener = (event: Event) => {
    const state = (event as CustomEvent<WorkspaceState>).detail;
    if (!state || typeof state.connected !== 'boolean' ||
        !Array.isArray(state.workspaces)) return;
    callback(state);
  };
  window.addEventListener('kira:workspaces', listener);
  return () => window.removeEventListener('kira:workspaces', listener);
}

export interface UiConfig {
  compact: boolean;
  title: string;
  show_seconds: boolean;
  show_names: boolean;
  max_workspaces: number;
}

export interface ConfigState {
  config: UiConfig;
  error: string | null;
}

export const defaultUiConfig: UiConfig = {
  compact: false,
  title: 'KIRA DESKTOP',
  show_seconds: false,
  show_names: true,
  max_workspaces: 12,
};

/** Subscribe to Core's validated KDL config snapshots, without polling. */
export function onConfigUpdate(callback: (state: ConfigState) => void): () => void {
  if (typeof window === 'undefined') return () => {};
  const listener = (event: Event) => {
    const state = (event as CustomEvent<ConfigState>).detail;
    const cfg = state?.config;
    if (!cfg || typeof cfg.compact !== 'boolean' ||
        typeof cfg.title !== 'string' || typeof cfg.show_seconds !== 'boolean' ||
        typeof cfg.show_names !== 'boolean' || typeof cfg.max_workspaces !== 'number' ||
        !Number.isInteger(cfg.max_workspaces) || cfg.max_workspaces < 1 ||
        cfg.max_workspaces > 32) return;
    callback(state);
  };
  window.addEventListener('kira:config', listener);
  return () => window.removeEventListener('kira:config', listener);
}

type Methods = {
  ping: string;
  get_state: CoreState;
  get_workspaces: WorkspaceState;
  get_config: ConfigState;
};

type RpcReply = {
  jsonrpc: '2.0';
  id: number | null;
  result?: unknown;
  error?: { code: number; message: string };
};

type Bridge = {
  postMessage(message: string): void;
};

declare global {
  interface Window {
    webkit?: { messageHandlers?: { kira?: Bridge; kiraDiagnostics?: Bridge; kiraLauncher?: Bridge; kiraCalendar?: Bridge; kiraSystem?: Bridge } };
  }
}

/** Only fixed commands are exposed to trusted Svelte components. Keep host
 * bridge failures non-throwing so an offline overlay does not crash the bar.
 */
function postNativeCommand(channelName: 'kiraLauncher' | 'kiraCalendar' | 'kiraSystem', command: 'open' | 'toggle'): boolean {
  const channel = typeof window === 'undefined'
    ? undefined
    : window.webkit?.messageHandlers?.[channelName];
  if (!channel) return false;
  try {
    channel.postMessage(command);
    return true;
  } catch {
    return false;
  }
}

/** Native launcher control is a fixed-purpose channel, never a shell command. */
export function openLauncher(): boolean {
  return postNativeCommand('kiraLauncher', 'open');
}

/** Toggle the native calendar overlay through its fixed-purpose channel. */
export function toggleCalendar(): boolean {
  return postNativeCommand('kiraCalendar', 'toggle');
}

/** Open or close the native read-only system monitor. */
export function toggleSystemPanel(): boolean {
  return postNativeCommand('kiraSystem', 'toggle');
}

let nextId = 0;
const pending = new Map<number, {
  resolve: (value: unknown) => void;
  reject: (reason: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
}>();

/** Deliver a reply from the Rust Host. Exported for contract tests. */
export function deliverReply(reply: RpcReply): void {
  if (reply.jsonrpc !== '2.0' || typeof reply.id !== 'number') return;
  const request = pending.get(reply.id);
  if (!request) return;
  pending.delete(reply.id);
  clearTimeout(request.timeout);
  if (reply.error) request.reject(new Error(`${reply.error.code}: ${reply.error.message}`));
  else request.resolve(reply.result);
}

if (typeof window !== 'undefined') {
  window.addEventListener('kira:rpc', (event: Event) => {
    deliverReply((event as CustomEvent<RpcReply>).detail);
  });
}

/**
 * Asks the Core for an allowlisted read-only method. Resolves only after the Rust
 * Host has returned a response; rejects if Core is unavailable or times out.
 */
export function request<M extends keyof Methods>(method: M): Promise<Methods[M]> {
  return send(method) as Promise<Methods[M]>;
}

/** One intentional, narrowly scoped Niri action.
 * Uses a stable string ID, never a per-monitor workspace index.
 * Core checks the live workspace snapshot before sending FocusWorkspace.
 */
export function focusWorkspace(workspaceId: string): Promise<void> {
  // u64 is decimal and may exceed Number.MAX_SAFE_INTEGER: never coerce to JS number.
  if (!/^(0|[1-9][0-9]*)$/.test(workspaceId) ||
      workspaceId.length > 20 ||
      BigInt(workspaceId) > 18446744073709551615n) {
    return Promise.reject(new Error('Invalid workspace ID'));
  }
  return send('focus_workspace', { id: workspaceId }).then(() => undefined);
}

function send(method: string, params?: { id: string }): Promise<unknown> {
  const channel = typeof window === 'undefined' ? undefined : window.webkit?.messageHandlers?.kira;
  if (!channel) return Promise.reject(new Error('Kira bridge unavailable (browser preview)'));

  const id = ++nextId;
  return new Promise<unknown>((resolve, reject) => {
    const timeout = setTimeout(() => {
      pending.delete(id);
      reject(new Error('Kira Core request timed out'));
    }, 3000);
    pending.set(id, { resolve, reject, timeout });
    try {
      channel.postMessage(JSON.stringify({
        jsonrpc: '2.0', id, method, ...(params ? { params } : {}),
      }));
    } catch (error) {
      clearTimeout(timeout);
      pending.delete(id);
      reject(error instanceof Error ? error : new Error('Kira bridge failed'));
    }
  });
}
