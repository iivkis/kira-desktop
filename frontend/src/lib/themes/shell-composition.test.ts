import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const bar = readFileSync(new URL('../../style.css', import.meta.url), 'utf8');
const markup = readFileSync(new URL('../../App.svelte', import.meta.url), 'utf8');
const workspaces = readFileSync(new URL('../components/WorkspacePreview.svelte', import.meta.url), 'utf8');
const core = readFileSync(new URL('../components/CoreStatus.svelte', import.meta.url), 'utf8');

describe('Top Bar architectural composition', () => {
  it('has exactly 42px and three differentiated functional zones', () => {
    expect(bar).toContain('height: 42px;');
    expect(bar).toContain('.bar__identity');
    expect(bar).toContain('background: var(--kira-color-surface);');
    expect(bar).toContain('.bar__navigation');
    expect(bar).toContain('.bar__system');
    expect(bar).toContain('background: var(--kira-color-surface-subtle);');
    expect(bar).toContain('.bar__identity::after');
    expect(bar).toContain('background: var(--kira-color-accent);');
    expect(bar).not.toContain('box-shadow:');
    expect(markup).not.toContain('bar__center');
    expect(markup).not.toContain('bar__separator');
  });

  it('preserves keyboard-friendly Launcher, workspace navigation and KDL title', () => {
    expect(markup).toContain('onclick={activateLauncher}');
    expect(markup).toContain('class="bar__identity"');
    expect(markup).toContain('class="bar__navigation"');
    expect(markup).toContain('class="bar__workspace-meta"');
    expect(markup).toContain('configState.config.title');
    expect(markup).toContain('class:compact={configState.config.compact}');
    expect(markup).toContain('formatClock(now, undefined, configState.config.show_seconds)');
    expect(bar).toContain('.brand:focus-visible');
    expect(bar).toContain('@media (max-width: 540px)');
    expect(bar).toContain('.bar.compact');
  });

  it('keeps workspace and Core state distinct and accessible', () => {
    expect(workspaces).toContain('background: var(--kira-color-accent);');
    expect(workspaces).toContain('color: var(--kira-color-on-accent);');
    expect(workspaces).not.toContain('box-shadow:');
    expect(workspaces).toContain('class:preview-selected={workspace.is_focused}');
    expect(workspaces).toContain('class:preview-urgent={workspace.is_urgent}');
    expect(workspaces).toContain('aria-pressed={workspace.is_focused}');
    expect(workspaces).toContain('disabled={pending}');
    expect(workspaces).toContain('.workspace-preview__item.preview-selected .workspace-preview__button:focus-visible');
    expect(core).toContain('aria-live="polite"');
    expect(core).toContain("status !== 'connected'");
  });
});
