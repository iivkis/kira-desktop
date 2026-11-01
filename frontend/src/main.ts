import { flushSync, mount } from 'svelte';
import App from './App.svelte';
import CalendarWidget from './lib/components/CalendarWidget.svelte';
import { closeCalendarOverlay, copyCalendarDate } from './lib/sdk/core';
import { startUi, whenDocumentReady } from './lib/bootstrap';
import './style.css';

function report(message: string): void {
  try {
    window.webkit?.messageHandlers?.kiraDiagnostics?.postMessage(message);
  } catch {
    // Diagnostic transport is best-effort and must not break the UI.
  }
}

Reflect.set(window, '__kiraEntryStarted', true);
report('Svelte entrypoint evaluating; document.readyState=' + document.readyState);

// The original Vite <script type="module"> ran after parsing the body.
// Our single-file build inlines it as a classic <script> in <head>, so
// attempting mount() immediately would fail because #app does not exist yet.
whenDocumentReady(document, () => {
  report('Svelte bootstrap after DOMContentLoaded; readyState=' + document.readyState);
  const target = document.getElementById('app');
  const mounted = startUi(
    target,
    (container) => {
      // The same self-contained frontend is hosted by the Top Bar and
      // independent layer-shell popups. Rust sets the surface marker before
      // load_html(), never from a navigable URL or an untrusted message.
      if (document.documentElement.dataset.kiraSurface === 'calendar') {
        mount(CalendarWidget, {
          target: container,
          props: { onClose: closeCalendarOverlay, onCopy: copyCalendarDate },
        });
      } else {
        mount(App, { target: container });
      }
      flushSync();
    },
    report,
  );
  Reflect.set(window, '__kiraMountReturned', mounted);
  if (mounted) {
    report('Svelte entrypoint mounted');
  }
});
