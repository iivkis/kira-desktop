import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const css = readFileSync(new URL('../../style.css', import.meta.url), 'utf8');
const markup = readFileSync(new URL('../../App.svelte', import.meta.url), 'utf8');
const workspaces = readFileSync(new URL('../components/WorkspacePreview.svelte', import.meta.url), 'utf8');
const core = readFileSync(new URL('../components/SystemHealth.svelte', import.meta.url), 'utf8');
const vite = readFileSync(new URL('../../../vite.config.ts', import.meta.url), 'utf8');
const bootstrap = readFileSync(new URL('../bootstrap.ts', import.meta.url), 'utf8');

describe('Tailwind v4 frontend and Top Bar architecture', () => {
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
    expect(markup).toContain('after:bg-kira-accent');
    expect(markup).toContain('aria-label="Kira Launcher"');
    expect(markup).toContain('aria-label="Niri workspaces"');
    expect(markup).toContain('aria-label="System status and local time"');
    expect(markup).toContain('configState.config.compact');
    expect(markup).toContain('class={`flex shrink-0 items-center justify-end gap-3');
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

  it('keeps launcher, KDL settings and live clock functional', () => {
    expect(markup).toContain('onclick={activateLauncher}');
    expect(markup).toContain('configState.config.title');
    expect(markup).toContain('configState.config.show_seconds');
    expect(markup).toContain('formatClock(now, undefined, configState.config.show_seconds)');
    expect(markup).toContain('focus-visible:outline-kira-accent');
  });

  it('preserves accessible live workspace and Core states without component CSS', () => {
    expect(workspaces).toContain('aria-pressed={workspace.is_focused}');
    expect(workspaces).toContain('data-urgent={workspace.is_urgent}');
    expect(workspaces).toContain('disabled={pending}');
    expect(workspaces).toContain('focus-visible:outline-kira-text');
    expect(workspaces).toContain('bg-kira-accent text-kira-on-accent');
    expect(core).toContain('aria-live="polite"');
    expect(core).toContain('data-phase={health.phase}');
    expect(core).toContain('aria-label={health.details.replace');
    expect(core).toContain('border-kira-accent bg-kira-accent');
    expect(workspaces).not.toContain('<style>');
    expect(core).not.toContain('<style>');
    expect(core).not.toContain('min-h-7');
    expect(markup).toContain("weekday: 'long'");
    expect(markup).toContain('items-center justify-end');
    expect(markup).not.toContain('bar__system-item');
  });
});
