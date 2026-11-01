import { flushSync, mount, unmount } from 'svelte';
import App from './App.svelte';
import CalendarWidget from './lib/components/CalendarWidget.svelte';
import { closeCalendarOverlay, copyCalendarDate, notifyCalendarHostReady, notifyCalendarMounted } from './lib/sdk/core';
import { createLazyWidget } from './lib/lazy-widget';
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

// The inline bundle is a classic <script> in <head>; wait for #app.
// Calendar's page is bootstrapped without mounting any Svelte widget.
whenDocumentReady(document, () => {
  report('Svelte bootstrap after DOMContentLoaded; readyState=' + document.readyState);
  const target = document.getElementById('app');
  if (document.documentElement.dataset.kiraSurface === 'calendar') {
    if (!target) {
      report('Calendar bootstrap failure: #app missing');
      return;
    }
    const host = createLazyWidget(
      async () => {
        // The component code is in the self-contained bundle, but its
        // Svelte instance/DOM/timer only exist while the popup is visible.
        return {
          mount() {
            const instance = mount(CalendarWidget, {
              target,
              props: { onClose: closeCalendarOverlay, onCopy: copyCalendarDate },
            });
            flushSync();
            if (!target.querySelector('main[data-kira-ui-root][aria-label="Kira Calendar"]')) {
              throw new Error('Calendar Svelte root missing after mount');
            }
            return instance;
          },
          unmount(instance: ReturnType<typeof mount>) {
            return unmount(instance);
          },
          onMounted: notifyCalendarMounted,
        };
      },
      (error) => report('Calendar lazy mount/unmount failed: ' + String(error)),
    );
    window.addEventListener('kira:calendar:show', (event: Event) => {
      host.show((event as CustomEvent<{ generation: number }>).detail?.generation);
    });
    window.addEventListener('kira:calendar:hide', () => host.hide());
    // Remove the HTML fallback so no widget DOM remains when hidden.
    target.querySelector('#kira-loading')?.remove();
    Reflect.set(window, '__kiraMountReturned', true);
    report('Calendar host ready; no widget mounted');
    notifyCalendarHostReady();
    return;
  }

  const mounted = startUi(
    target,
    (container) => {
      mount(App, { target: container });
      flushSync();
    },
    report,
  );
  Reflect.set(window, '__kiraMountReturned', mounted);
  if (mounted) report('Svelte entrypoint mounted');
});
