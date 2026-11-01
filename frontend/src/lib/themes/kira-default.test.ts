import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { kiraDefaultPalette, kiraDefaultRoles } from './kira-default';

const css = readFileSync(new URL('./kira-default.css', import.meta.url), 'utf8');
const html = readFileSync(new URL('../../../index.html', import.meta.url), 'utf8');

function luminance(hex: string): number {
  const channels = [1, 3, 5]
    .map((index) => parseInt(hex.slice(index, index + 2), 16) / 255)
    .map((value) => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrast(first: string, second: string): number {
  const light = Math.max(luminance(first), luminance(second));
  const dark = Math.min(luminance(first), luminance(second));
  return (light + 0.05) / (dark + 0.05);
}

describe('Kira Default Black / Prussian Blue / Orange design tokens', () => {
  it('contains exactly the five approved canonical base colors', () => {
    expect(Object.values(kiraDefaultPalette)).toEqual([
      '#000000ff', '#14213dff', '#fca311ff', '#e5e5e5ff', '#ffffffff',
    ]);
    expect(new Set(Object.values(kiraDefaultPalette)).size).toBe(5);
  });

  it('keeps exactly five canonical RGBA hex tokens synchronized', () => {
    const names = {
      black: 'black',
      prussianBlue: 'prussian-blue',
      orange: 'orange',
      alabasterGrey: 'alabaster-grey',
      white: 'white',
    } as const;
    for (const role of Object.keys(names) as Array<keyof typeof names>) {
      expect(css).toContain('--' + names[role] + ': ' + kiraDefaultPalette[role] + ';');
    }
    const colors = [...css.matchAll(/#[0-9a-f]{8}\b/g)].map((match) => match[0]);
    expect(colors).toHaveLength(5);
    expect(new Set(colors)).toEqual(new Set(Object.values(kiraDefaultPalette)));
    expect(css).not.toMatch(/#[0-9a-f]{6}(?![0-9a-f])/);
  });

  it('maps semantic tokens to canonical colors', () => {
    const aliases = {
      background: '--black',
      surface: '--prussian-blue',
      muted: '--alabaster-grey',
      accent: '--orange',
      text: '--white',
      earth: '--prussian-blue', // legacy alias for compatibility
    } as const;
    const source = {
      '--black': 'black',
      '--prussian-blue': 'prussianBlue',
      '--orange': 'orange',
      '--alabaster-grey': 'alabasterGrey',
      '--white': 'white',
    } as const;
    for (const role of Object.keys(aliases) as Array<keyof typeof aliases>) {
      const base = aliases[role];
      expect(css).toContain('--kira-color-' + role + ': var(' + base + ');');
      expect(kiraDefaultRoles[role]).toBe(kiraDefaultPalette[source[base]]);
    }
    expect(css).toContain('--kira-color-on-accent: var(--black);');
  });

  it('uses WCAG AA text pairs, including black on Orange', () => {
    const { black, prussianBlue, orange, alabasterGrey, white } = kiraDefaultPalette;
    for (const [foreground, background] of [
      [white, black], [white, prussianBlue],
      [alabasterGrey, black], [alabasterGrey, prussianBlue],
      [orange, black], [orange, prussianBlue], [black, orange],
    ]) {
      expect(contrast(foreground, background)).toBeGreaterThanOrEqual(4.5);
    }
    expect(contrast(white, orange)).toBeLessThan(4.5);
    expect(kiraDefaultRoles.muted).toBe(alabasterGrey);
    expect(kiraDefaultRoles.accent).toBe(orange);
  });

  it('uses the default palette during pre-mount WebKit loading', () => {
    expect(html).toContain('<meta name="theme-color" content="' + kiraDefaultPalette.black + '"');
    expect(html).toContain('background:' + kiraDefaultPalette.black + ';');
    expect(html).toContain('color:' + kiraDefaultPalette.white + ';');
    expect(html).not.toContain('#22223b');
    expect(html).not.toContain('#f2e9e4');
  });

  it('defines compact spacing, consistent radii and reduced motion', () => {
    expect(css).toContain('--kira-space-1: 4px;');
    expect(css).toContain('--kira-radius-md: 10px;');
    expect(css).not.toMatch(/--kira-(?:border-strong|shadow-brutal|shadow-panel)\\s*:/);
    expect(css).toContain('@media (prefers-reduced-motion: reduce)');
    expect(css).toContain('--kira-motion-fast: 0ms;');
  });
});
