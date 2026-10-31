import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const bar = readFileSync(new URL('../../style.css', import.meta.url), 'utf8');
const markup = readFileSync(new URL('../../App.svelte', import.meta.url), 'utf8');
const workspaces = readFileSync(new URL('../components/WorkspacePreview.svelte', import.meta.url), 'utf8');
const core = readFileSync(new URL('../components/CoreStatus.svelte', import.meta.url), 'utf8');

describe('Top Bar editorial composition contract', () => {
  it('keeps the native 42px strip as a single surface without inset cards', () => {
    expect(bar).toContain('height: 42px;');
    expect(bar).toContain('border-bottom: 1px');
    expect(bar).not.toContain('box-shadow:');
    expect(bar).not.toContain('var(--kira-shadow-brutal)');
    expect(bar).not.toContain('.bar__separator');
    expect(bar).not.toContain('.bar__center');
    expect(bar).not.toContain('.brand__symbol');
    expect(bar).not.toContain('.clock__icon');
    expect(markup).not.toContain('bar__separator');
    expect(markup).not.toContain('bar__center');
    expect(markup).not.toContain('brand__symbol');
  });

  it('keeps the brand clickable and custom title in the content flow', () => {
    expect(markup).toContain('onclick={activateLauncher}');
    expect(markup).toContain('class="bar__context"');
    expect(markup).toContain('configState.config.title');
    expect(markup).toContain('formatClock(now, undefined, configState.config.show_seconds)');
  });

  it('reserves orange fill for selection while preserving keyboard and urgency', () => {
    expect(workspaces).toContain('background: var(--kira-color-accent);');
    expect(workspaces).toContain('color: var(--kira-color-on-accent);');
    expect(workspaces).not.toContain('box-shadow:');
    expect(workspaces).toContain('class:preview-selected={workspace.is_focused}');
    expect(workspaces).toContain('class:preview-urgent={workspace.is_urgent}');
    expect(workspaces).toContain('aria-pressed={workspace.is_focused}');
    expect(workspaces).toContain('disabled={pending}');
    expect(workspaces).toContain(':focus-visible');
    expect(workspaces).toContain('.workspace-preview__item.preview-selected .workspace-preview__button:focus-visible');
  });

  it('communicates status without an extra framed badge', () => {
    expect(core).toContain('aria-live="polite"');
    expect(core).toContain("status !== 'connected'");
    expect(core).not.toContain('box-shadow:');
    expect(core).not.toContain('border: var(--kira-border-strong)');
  });
});
