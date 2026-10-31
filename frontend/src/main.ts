import { flushSync, mount } from 'svelte';
import App from './App.svelte';
import { startUi } from './lib/bootstrap';
import './style.css';

function report(message: string): void {
  try {
    window.webkit?.messageHandlers?.kiraDiagnostics?.postMessage(message);
  } catch {
    // Diagnostic transport is best-effort and must not break the UI.
  }
}

Reflect.set(window, '__kiraEntryStarted', true);
report('Svelte entrypoint evaluating');
const target = document.getElementById('app');
const mounted = startUi(
  target,
  (container) => {
    mount(App, { target: container });
    flushSync();
  },
  report,
);
Reflect.set(window, '__kiraMountReturned', mounted);
if (mounted) {
  report('Svelte entrypoint mounted');
}
