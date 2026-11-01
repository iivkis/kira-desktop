import { flushSync, mount, unmount } from 'svelte';
import App from './App.svelte';
import CalendarWidget from './lib/components/CalendarWidget.svelte';
import SystemWidget from './lib/components/SystemWidget.svelte';
import LauncherWidget from './lib/components/LauncherWidget.svelte';
import { closeCalendarOverlay, copyCalendarDate, closeSystemOverlay, copySystemReport, setSystemMonitorTab, closeLauncherOverlay, notifyCalendarHostReady, notifyCalendarMounted } from './lib/sdk/core';
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
// The shared overlay page is bootstrapped without mounting any Svelte widget.
whenDocumentReady(document, () => {
  report('Svelte bootstrap after DOMContentLoaded; readyState=' + document.readyState);
  const target = document.getElementById('app');
  if (document.documentElement.dataset.kiraSurface === 'calendar') {
    if (!target) {
      report('Calendar bootstrap failure: #app missing');
      return;
    }
    let requestedKind: 'calendar' | 'system' | 'launcher' = 'calendar';
    let visibleGeneration: number | null = null;
    let lastGeneration = 0;
    const host = createLazyWidget(
      async () => {
        // Both components are prebundled, but neither owns DOM, timers or
        // subscriptions until its popup is explicitly requested.
        const kind = requestedKind;
        return {
          mount() {
            const instance = kind === 'calendar'
              ? mount(CalendarWidget, {
                target,
                props: { onClose: closeCalendarOverlay, onCopy: copyCalendarDate },
              })
              : kind === 'system'
                ? mount(SystemWidget, {
                  target,
                  props: { onClose: closeSystemOverlay, onCopy: copySystemReport, onTab: setSystemMonitorTab },
                })
                : mount(LauncherWidget, { target, props: { onClose: closeLauncherOverlay, generation: visibleGeneration ?? 0 } });
            flushSync();
            const label = kind === 'calendar' ? 'Kira Calendar' : kind === 'system' ? 'Kira System Monitor' : 'Kira Launcher';
            if (!target.querySelector('main[data-kira-ui-root][aria-label="' + label + '"]')) {
              throw new Error('Svelte ' + kind + ' root missing after mount');
            }
            return instance;
          },
          unmount(instance: ReturnType<typeof mount>) {
            return unmount(instance);
          },
          onMounted(generation: number) {
            report('Overlay Svelte ' + kind + ' mounted; generation=' + generation);
            notifyCalendarMounted(generation);
          },
        };
      },
      (error) => report('Overlay lazy mount/unmount failed: ' + String(error)),
    );
    window.addEventListener('kira:overlay:show', (event: Event) => {
      const detail = (event as CustomEvent<{ kind: string; generation: number }>).detail;
      if ((detail?.kind !== 'calendar' && detail?.kind !== 'system' && detail?.kind !== 'launcher') ||
          !Number.isSafeInteger(detail.generation) || detail.generation <= 0 ||
          detail.generation > 0xffffffff || detail.generation <= lastGeneration) {
        report('Overlay rejected invalid or stale show payload');
        return;
      }
      lastGeneration = detail.generation;
      visibleGeneration = detail.generation;
      report('Overlay received show: ' + detail.kind + ' generation=' + detail.generation);
      requestedKind = detail.kind;
      host.show(detail.generation);
    });
    window.addEventListener('kira:overlay:hide', (event: Event) => {
      const generation = (event as CustomEvent<{ generation: number }>).detail?.generation;
      if (generation !== visibleGeneration) {
        report('Overlay ignored stale hide generation=' + generation);
        return;
      }
      visibleGeneration = null;
      host.hide();
    });
    // Remove the HTML fallback so no widget DOM remains when hidden.
    target.querySelector('#kira-loading')?.remove();
    Reflect.set(window, '__kiraMountReturned', true);
    report('Overlay host ready; no widget mounted');
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
