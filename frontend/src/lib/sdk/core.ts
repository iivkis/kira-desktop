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

/** Subscribe to a trusted native event after checking its payload shape. */
function onNativeEvent<T>(
  name: string,
  isValid: (value: unknown) => value is T,
  callback: (value: T) => void,
): () => void {
  if (typeof window === 'undefined') return () => {};
  const listener = (event: Event) => {
    const value = (event as CustomEvent<unknown>).detail;
    if (isValid(value)) callback(value);
  };
  window.addEventListener(name, listener);
  return () => window.removeEventListener(name, listener);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isSystemMetrics(value: unknown): value is SystemMetrics {
  if (!isRecord(value)) return false;
  const valid = (n: unknown) => n === null ||
    (typeof n === 'number' && Number.isFinite(n) && n >= 0);
  const percent = (n: unknown) => valid(n) && (n === null || (typeof n === 'number' && n <= 100));
  return percent(value.cpu_percent) &&
    valid(value.memory_total_kib) && valid(value.memory_available_kib) &&
    valid(value.root_total_kib) && valid(value.root_available_kib) &&
    percent(value.battery_percent) &&
    (value.battery_charging === null || typeof value.battery_charging === 'boolean');
}

/** Event-driven system metrics: does not add browser polling or an RPC method. */
export function onSystemMetrics(callback: (metrics: SystemMetrics) => void): () => void {
  return onNativeEvent('kira:system-metrics', isSystemMetrics, callback);
}

export interface SystemOverlayData {
  metrics: SystemMetrics;
  core: boolean;
  niri: boolean;
}

export interface ProcessRow {
  pid: number;
  name: string;
  state: string;
  cpu_percent: number | null;
  rss_kib: number | null;
}

export interface SystemProcesses {
  rows: ProcessRow[];
  total: number;
  truncated: boolean;
  error: string | null;
}

/** Overlay-specific, read-only events with strict bounded payload shapes. */
export function onSystemOverlayData(callback: (data: SystemOverlayData) => void): () => void {
  return onNativeEvent('kira:system:data', (value): value is SystemOverlayData =>
    isRecord(value) && typeof value.core === 'boolean' &&
    typeof value.niri === 'boolean' && isSystemMetrics(value.metrics) &&
    isRecord(value.metrics) &&
    (value.metrics.swap_total_kib === null || Number.isSafeInteger(value.metrics.swap_total_kib)) &&
    (value.metrics.swap_free_kib === null || Number.isSafeInteger(value.metrics.swap_free_kib)) &&
    (value.metrics.uptime_seconds === null || Number.isSafeInteger(value.metrics.uptime_seconds)) &&
    (value.metrics.load === null ||
      (Array.isArray(value.metrics.load) && value.metrics.load.length === 3 &&
        value.metrics.load.every((n: unknown) => typeof n === 'number' && Number.isFinite(n) && n >= 0))),
  callback);
}

export function onSystemProcesses(callback: (data: SystemProcesses) => void): () => void {
  return onNativeEvent('kira:system:processes', (value): value is SystemProcesses => {
    if (!isRecord(value) || !Array.isArray(value.rows) || value.rows.length > 4096 ||
        !Number.isSafeInteger(value.total) || (value.total as number) < 0 ||
        typeof value.truncated !== 'boolean' ||
        !(value.error === null || (typeof value.error === 'string' && value.error.length <= 120))) return false;
    return value.rows.every((row: unknown) => isRecord(row) &&
      Number.isSafeInteger(row.pid) && (row.pid as number) >= 0 && (row.pid as number) <= 4294967295 &&
      typeof row.name === 'string' && row.name.length <= 64 &&
      typeof row.state === 'string' && row.state.length === 1 &&
      (row.cpu_percent === null || (typeof row.cpu_percent === 'number' && Number.isFinite(row.cpu_percent) && row.cpu_percent >= 0 && row.cpu_percent <= 100)) &&
      (row.rss_kib === null || (Number.isSafeInteger(row.rss_kib) && (row.rss_kib as number) >= 0)));
  }, callback);
}

/** Listen for typed workspace push events emitted by the native Shell.
 * No Niri or Core polling is performed here. Unsubscribe on Svelte unmount.
 */
export function onWorkspaceUpdate(callback: (state: WorkspaceState) => void): () => void {
  return onNativeEvent('kira:workspaces', (value): value is WorkspaceState =>
    isRecord(value) && typeof value.connected === 'boolean' && Array.isArray(value.workspaces),
  callback);
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
  return onNativeEvent('kira:config', (value): value is ConfigState => {
    if (!isRecord(value) || !isRecord(value.config)) return false;
    const cfg = value.config;
    return typeof cfg.compact === 'boolean' &&
      typeof cfg.title === 'string' && typeof cfg.show_seconds === 'boolean' &&
      typeof cfg.show_names === 'boolean' && typeof cfg.max_workspaces === 'number' &&
      Number.isInteger(cfg.max_workspaces) && cfg.max_workspaces >= 1 &&
      cfg.max_workspaces <= 32;
  }, callback);
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
    webkit?: { messageHandlers?: { kira?: Bridge; kiraDiagnostics?: Bridge; kiraLauncher?: Bridge; kiraCalendar?: Bridge; kiraCalendarUi?: Bridge; kiraSystem?: Bridge } };
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

/** Only the calendar overlay has access to this dedicated host channel.
 * The Top Bar cannot copy arbitrary text or request calendar window actions.
 */
function sendCalendarUi(command: string): boolean {
  const channel = typeof window === 'undefined'
    ? undefined
    : window.webkit?.messageHandlers?.kiraCalendarUi;
  if (!channel) return false;
  try {
    channel.postMessage(command);
    return true;
  } catch {
    return false;
  }
}

/** The WebKit document is ready, but no widget has been mounted yet. */
export function notifyCalendarHostReady(): void {
  sendCalendarUi('host-ready');
}

/** A specific popup generation has finished its lazy Svelte mount. */
export function notifyCalendarMounted(generation: number): void {
  if (!Number.isSafeInteger(generation) || generation <= 0 || generation > 0xffffffff) return;
  sendCalendarUi('mounted:' + generation);
}

export function closeCalendarOverlay(): void {
  sendCalendarUi('close');
}

export function copyCalendarDate(date: string): void {
  // Prevent arbitrary data crossing into the OS clipboard API. The Rust host
  // revalidates the exact date, including month lengths and leap years.
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return;
  sendCalendarUi('copy:' + date);
}

/** The shared overlay accepts only fixed read-only tab and copy actions. */
export function closeSystemOverlay(): void {
  sendCalendarUi('close');
}

export function setSystemMonitorTab(tab: 'overview' | 'processes'): void {
  if (tab === 'overview' || tab === 'processes') sendCalendarUi('tab:' + tab);
}

export function copySystemReport(): void {
  sendCalendarUi('copy-report');
}

/** Open or close the shared Svelte read-only system monitor. */
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
