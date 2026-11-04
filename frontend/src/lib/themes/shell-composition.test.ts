import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const css = readFileSync(new URL('../../style.css', import.meta.url), 'utf8');
const markup = readFileSync(new URL('../../App.svelte', import.meta.url), 'utf8');
const workspaces = readFileSync(new URL('../components/WorkspacePreview.svelte', import.meta.url), 'utf8');
const core = readFileSync(new URL('../components/SystemHealth.svelte', import.meta.url), 'utf8');
const vite = readFileSync(new URL('../../../vite.config.ts', import.meta.url), 'utf8');
const bootstrap = readFileSync(new URL('../bootstrap.ts', import.meta.url), 'utf8');

describe('Tailwind v4 frontend and Top Bar architecture', () => {
  it('renders subdued orange depth and a soft black shadow on every popup edge', () => {
    expect(css).toContain('html[data-kira-surface="calendar"]:not([data-kira-detached="true"]) #app');
    expect(css).toMatch(/filter:\s*drop-shadow\(var\(--kira-widget-shadow-black\)\)\s*drop-shadow\(var\(--kira-widget-shadow-accent\)\)\s*drop-shadow\(var\(--kira-widget-shadow-soft\)\)/);
    expect(css).not.toContain('main[data-kira-ui-root]::after');
    expect(css).not.toContain('kira-widget-outline');
    expect(css).toContain('padding: var(--kira-widget-shadow-offset);');
    expect(css).toContain('max-height: calc(var(--kira-overlay-max-height, 560px) - var(--kira-widget-shadow-offset) - var(--kira-widget-shadow-offset));');
    expect(css).toContain('html[data-kira-detached="true"] #app {');
    expect(css).toContain('filter: none;');
    expect(markup).not.toContain('kira-widget-shadow');
  });

  it('uses the Vite plugin, CSS-first tokens and canonical palette', () => {
    expect(vite).toContain("import tailwindcss from '@tailwindcss/vite'");
    expect(vite).toContain('tailwindcss()');
    expect(css).toContain('@import "tailwindcss";');
    expect(css).toContain('@import "./lib/themes/kira-default.css";');
    expect(css).toContain('@theme inline {');
    expect(css).toContain('--color-kira-accent: var(--kira-color-accent);');
    expect(css).not.toContain('.bar__identity');
  });

  it('keeps a 42px three-zone Top Bar with compact and responsive layouts', () => {
    expect(markup).toContain('h-[42px] min-h-[42px]');
    expect(markup).toContain('bg-kira-surface');
    expect(markup).toContain('bg-kira-surface-soft');
    expect(markup).toContain('border-b border-kira-muted/15');
    expect(markup).not.toContain('after:bg-kira-accent');
    expect(markup).toContain('aria-label="Запуск приложений Kira"');
    expect(markup).toContain('aria-label="Рабочие пространства Niri"');
    expect(markup).toContain('aria-label="Система и местное время"');
    expect(markup).toContain('configState.config.compact');
    expect(markup).toContain('items-center justify-end gap-1 border-l');
    expect(markup).not.toContain('WORKSPACE</span>');
    expect(markup).not.toContain("'DESKTOP' :");
    expect(markup).toContain("configState.config.title !== 'KIRA DESKTOP'");
    expect(markup).not.toContain('>SYSTEM</span>');
    expect(markup).not.toContain('>LOCAL</span>');
    expect(markup).toContain('niriConnected={workspaceState?.connected ?? null}');
    expect(markup).toContain('max-[540px]:');
    expect(markup).not.toContain('<style>');
    expect(markup).not.toContain('bar__center');
    // Regression: the Svelte root and bootstrap must share the same marker.
    expect(markup).toContain('data-kira-ui-root');
    expect(bootstrap).toContain("querySelector('main[data-kira-ui-root]')");
    expect(bootstrap).not.toContain('main.bar');
    expect(markup).not.toContain('box-shadow');
  });

  it('retains focus scrolling and hidden urgency in workspace navigation', () => {
    expect(workspaces).toContain('tenetScrollIntoView(list, match, animate)');
    expect(workspaces).toContain("use:tenetFlowIndicator={{ active: focusedId ?? '', axis: 'both' }}");
    expect(workspaces).toContain('data-kira-flow-indicator');
    expect(workspaces).toContain('data-workspace-id={workspace.id}');
    expect(workspaces).toContain('windowed.beforeUrgent');
    expect(workspaces).toContain('windowed.afterUrgent');
    expect(workspaces).toContain("workspace.is_urgent ? ', требует внимания' : ''");
  });

    it('keeps launcher, KDL settings and live clock functional', () => {
    expect(markup).toContain('onclick={activateLauncher}');
    expect(markup).toContain('configState.config.title');
    expect(markup).toContain('configState.config.show_seconds');
    expect(markup).toContain('formatClock(now, undefined, configState.config.show_seconds)');
    expect(markup).toContain('focus-visible:outline-kira-accent');
    expect(markup).toContain('onclick={activateCalendar}');
    expect(markup).toContain('onOpen={activateSystemPanel}');
    expect(markup).toContain('metrics={systemMetrics}');
    expect(markup).toContain('stopMetrics();');
    expect(markup).toContain('metricsLastReceivedAt = Date.now()');
    expect(markup).toContain('warnings.length');
    expect(markup).toContain('aria-haspopup="dialog"');
    expect(markup).toContain('<time class=');
    expect(markup).toContain('КАЛЕНДАРЬ НЕДОСТУПЕН');
  });

  it('preserves accessible live workspace and Core states without component CSS', () => {
    expect(workspaces).toContain('aria-pressed={workspace.is_focused}');
    expect(workspaces).toContain('data-urgent={workspace.is_urgent}');
    expect(workspaces).toContain('disabled={pending}');
    expect(workspaces).toContain('focus-visible:outline-kira-on-accent focus-visible:-outline-offset-2');
    expect(workspaces).toContain("? 'text-kira-on-accent'");
    expect(workspaces).toContain('data-kira-tab={workspace.id}');
    expect(workspaces).toContain('rounded-[2px] bg-kira-accent');
    expect(workspaces).toContain('visibleWorkspaceWindow(');
    expect(workspaces).toContain('data-workspace-strip');
    expect(workspaces).toContain('windowed.before > 0');
    expect(workspaces).toContain('windowed.after > 0');
    expect(workspaces).toContain('rounded-[2px]');
    expect(core).toContain('aria-live="polite"');
    expect(core).toContain('data-phase={health.phase}');
    expect(core).toContain('aria-haspopup="dialog"');
    expect(core).toContain('onclick={onOpen}');
    expect(core).toContain("aria-label={'Открыть монитор системы: ' + label}");
    expect(core).toContain('border-kira-accent bg-kira-accent');
    expect(workspaces).not.toContain('<style>');
    expect(core).not.toContain('<style>');
    expect(core).not.toContain('min-h-7');
    expect(core).toContain("health.phase === 'ready' ? 'sr-only' : ''");
    expect(markup).toContain('title="Открыть календарь"');
    expect(markup).toContain('items-center justify-end');
    expect(markup).not.toContain('bar__system-item');
  });
});

const monitor = readFileSync(new URL('../components/SystemWidget.svelte', import.meta.url), 'utf8');
const charts = readFileSync(new URL('../components/SystemCharts.svelte', import.meta.url), 'utf8');
const settings = readFileSync(new URL('../components/SettingsWidget.svelte', import.meta.url), 'utf8');
const html = readFileSync(new URL('../../../index.html', import.meta.url), 'utf8');

describe('Kira editorial polish', () => {
  it('exposes full warning text on-demand without changing the 42px Wayland surface', () => {
    expect(markup).toContain('onclick={showWarningDetails}');
    expect(markup).toContain('onclick={inspectWarnings}');
    expect(markup).toContain('const warningPosition = $derived(visibleWarningIndex(warnings, activeWarningId))');
    expect(markup).toContain('nextWarningId(warnings, activeWarningId)');
    expect(markup).toContain('Открыть предупреждение ');
    expect(markup).toContain('russianCountLabel(warnings.length');
    expect(markup).toContain('event.key !== \'Tab\'');
    expect(markup).toContain('last.focus()');
    expect(markup).toContain('first.focus()');
    expect(markup).toContain('aria-controls={inspectingWarnings ?');
    expect(markup).not.toContain('join(\'; \')');
    expect(markup).toContain('role="dialog" aria-modal="true" aria-label="Подробности предупреждения"');
    expect(markup).toContain('inert={inspectingWarnings}');
    expect(markup).toContain('resize-none overflow-y-auto');
    expect(markup).toContain('readonly rows="2" wrap="soft"');
    expect(markup).toContain('aria-label="Следующее предупреждение"');
    expect(markup).toContain('onkeydown={onWarningKeydown}');
    expect(markup).toContain('role="status" aria-live="polite"');
    expect(markup).toContain('focus-visible:outline-2');
    expect(markup).not.toContain("warnings[0]}{warnings.length > 1");
  });

  it('reserves orange for meaningful monitor states instead of static sections', () => {
    expect(monitor).toContain('text-kira-text">ЁМКОСТЬ</h2>');
    expect(monitor).toContain('text-kira-text">ОБОРУДОВАНИЕ</h2>');
    expect(charts).toContain('text-kira-text">ИСТОРИЯ НАГРУЗКИ</h2>');
    expect(charts).toContain('historyCaption = activityHistoryCaption(syncedHistory)');
    expect(charts).toContain('{historyCaption}</p>');
    expect(charts).not.toContain('До 60 с назад');
    expect(charts).not.toContain('Последний замер');
    expect(charts).toContain('animationDurationUpdate: reduced ? 0 : 180');
  });

  it('keeps unfinished settings passive and the current-page hierarchy secondary', () => {
    expect(settings).toContain('SETTINGS_SECTIONS.filter(item => item.available)');
    expect(settings).toContain('SETTINGS_SECTIONS.filter(item => !item.available)');
    expect(settings).toContain('В ПЛАНАХ');
    expect(settings).toContain('text-[26px] font-extrabold leading-tight tracking-tight">Настройки');
    expect(settings).toContain('text-[21px] font-extrabold leading-tight tracking-tight">О системе');
    expect(settings).toContain('use:tenetFlowIndicator={{ active: section, axis: \'both\' }}');
  });

  it('localizes visible bootstrap fallback states, preserving diagnostics', () => {
    expect(html).toContain('Загрузка интерфейса…');
    expect(html).toContain('Ошибка интерфейса Kira:');
    expect(html).toContain('Интерфейс Kira не запустился');
    expect(html).toContain("report('Svelte not mounted:");
  });
});
