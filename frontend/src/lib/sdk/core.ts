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

type Methods = {
  ping: string;
  get_state: CoreState;
  get_workspaces: WorkspaceState;
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
    webkit?: { messageHandlers?: { kira?: Bridge; kiraDiagnostics?: Bridge } };
  }
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
  const channel = typeof window === 'undefined' ? undefined : window.webkit?.messageHandlers?.kira;
  if (!channel) return Promise.reject(new Error('Kira bridge unavailable (browser preview)'));

  const id = ++nextId;
  return new Promise<Methods[M]>((resolve, reject) => {
    const timeout = setTimeout(() => {
      pending.delete(id);
      reject(new Error('Kira Core request timed out'));
    }, 3000);
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject, timeout });
    try {
      channel.postMessage(JSON.stringify({ jsonrpc: '2.0', id, method }));
    } catch (error) {
      clearTimeout(timeout);
      pending.delete(id);
      reject(error instanceof Error ? error : new Error('Kira bridge failed'));
    }
  });
}
