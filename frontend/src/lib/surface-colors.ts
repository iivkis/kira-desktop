import { kiraDefaultPalette } from './themes/kira-default';

/** Read semantic CSS values at the drawing surface. ECharts and Canvas share
 * the same palette as Svelte instead of embedding their own hex constants.
 * Later KDL themes can override these roles without rewriting widgets. */
export function surfaceColors(element?: Element | null) {
  const style = element && typeof window !== 'undefined'
    ? window.getComputedStyle(element) : null;
  const value = (token: string, fallback: string) =>
    style?.getPropertyValue(token).trim() || fallback;
  return {
    background: value('--kira-color-background', kiraDefaultPalette.black),
    surface: value('--kira-color-surface', kiraDefaultPalette.prussianBlue),
    accent: value('--kira-color-accent', kiraDefaultPalette.orange),
    muted: value('--kira-color-muted', kiraDefaultPalette.alabasterGrey),
    text: value('--kira-color-text', kiraDefaultPalette.white),
  };
}

/** Canvas alpha derives from a semantic color, never a duplicated palette. */
export function withOpacity(color: string, opacity: number): string {
  const hex = /^#([0-9a-f]{6})(?:[0-9a-f]{2})?$/i.exec(color);
  if (!hex) return color;
  const value = hex[1];
  const channels = [0, 2, 4].map(offset => parseInt(value.slice(offset, offset + 2), 16));
  return 'rgba(' + channels.join(',') + ',' + Math.max(0, Math.min(1, opacity)) + ')';
}
