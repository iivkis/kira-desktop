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

describe('Kira Default Bordeaux / Taupe design tokens', () => {
  it('contains exactly the five approved canonical base colors', () => {
    expect(Object.values(kiraDefaultPalette)).toEqual([
      '#0a0908', '#49111c', '#f2f4f3', '#a9927d', '#5e503f',
    ]);
    expect(new Set(Object.values(kiraDefaultPalette)).size).toBe(5);
  });

  it('keeps public CSS tokens synchronized with the typed palette', () => {
    for (const [role, value] of Object.entries(kiraDefaultRoles)) {
      // Accent aliases the readable muted/taupe token; never add a sixth base.
      const token = role === 'accent' ? 'var(--kira-color-muted)' : value;
      expect(css).toContain('--kira-color-' + role + ': ' + token + ';');
    }
    const hexDeclarations = [...css.matchAll(/--kira-color-[\w-]+:\s*(#[0-9a-f]{6})\s*;/g)];
    expect(hexDeclarations).toHaveLength(5);
    expect(new Set(hexDeclarations.map((match) => match[1]))).toEqual(
      new Set(Object.values(kiraDefaultPalette)),
    );
  });

  it('chooses text/accent pairs that meet WCAG AA', () => {
    const { black, nightBordeaux, whiteSmoke, dustyTaupe, stoneBrown } = kiraDefaultPalette;
    expect(contrast(whiteSmoke, black)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(whiteSmoke, nightBordeaux)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(dustyTaupe, black)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(dustyTaupe, nightBordeaux)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(black, dustyTaupe)).toBeGreaterThanOrEqual(4.5);
    // Stone Brown is reserved for structure; low-contrast brown-on-black
    // must never be used as the primary/muted text.
    expect(contrast(stoneBrown, black)).toBeLessThan(4.5);
    expect(kiraDefaultRoles.muted).toBe(dustyTaupe);
    expect(kiraDefaultRoles.accent).toBe(dustyTaupe);
  });

  it('uses the default palette during pre-mount WebKit loading', () => {
    expect(html).toContain('<meta name="theme-color" content="' + kiraDefaultPalette.black + '"');
    expect(html).toContain('background:' + kiraDefaultPalette.black + ';');
    expect(html).toContain('color:' + kiraDefaultPalette.whiteSmoke + ';');
    expect(html).not.toContain('#22223b');
    expect(html).not.toContain('#f2e9e4');
  });

  it('defines compact spacing, consistent radii and reduced motion', () => {
    expect(css).toContain('--kira-space-1: 4px;');
    expect(css).toContain('--kira-radius-md: 10px;');
    expect(css).toContain('@media (prefers-reduced-motion: reduce)');
    expect(css).toContain('--kira-motion-fast: 0ms;');
  });
});
