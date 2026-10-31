import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { kiraDefaultPalette, kiraDefaultRoles } from './kira-default';

const css = readFileSync(new URL('./kira-default.css', import.meta.url), 'utf8');

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

describe('Kira Default premium design tokens', () => {
  it('contains exactly the five approved canonical base colors', () => {
    expect(Object.values(kiraDefaultPalette)).toEqual([
      '#22223b', '#4a4e69', '#9a8c98', '#c9ada7', '#f2e9e4',
    ]);
    expect(new Set(Object.values(kiraDefaultPalette)).size).toBe(5);
  });

  it('keeps public CSS tokens synchronized with the typed palette', () => {
    for (const [role, value] of Object.entries(kiraDefaultRoles)) {
      const declaration = '--kira-color-' + role + ': ' + value + ';';
      expect(css).toContain(declaration);
    }
    const hexDeclarations = [...css.matchAll(/--kira-color-[\w-]+:\s*(#[0-9a-f]{6})\s*;/g)];
    expect(hexDeclarations).toHaveLength(5);
  });

  it('chooses text/accent pairs that meet WCAG AA', () => {
    const { spaceIndigo, dustyGrape, lilacAsh, almondSilk, parchment } = kiraDefaultPalette;
    expect(contrast(parchment, spaceIndigo)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(parchment, dustyGrape)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(lilacAsh, spaceIndigo)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(spaceIndigo, almondSilk)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(lilacAsh, dustyGrape)).toBeLessThan(4.5);
  });

  it('defines compact spacing, consistent radii and reduced motion', () => {
    expect(css).toContain('--kira-space-1: 4px;');
    expect(css).toContain('--kira-radius-md: 10px;');
    expect(css).toContain('@media (prefers-reduced-motion: reduce)');
    expect(css).toContain('--kira-motion-fast: 0ms;');
  });
});
