import { describe, expect, it } from 'vitest';
import { surfaceColors, withOpacity } from './surface-colors';
import { kiraDefaultPalette } from './themes/kira-default';

describe('shared color roles for Canvas and ECharts', () => {
  it('falls back to exactly the canonical palette without DOM styling', () => {
    expect(surfaceColors()).toEqual({
      background: kiraDefaultPalette.black,
      surface: kiraDefaultPalette.prussianBlue,
      accent: kiraDefaultPalette.orange,
      muted: kiraDefaultPalette.alabasterGrey,
      text: kiraDefaultPalette.white,
    });
  });

  it('derives opacity from a palette role rather than another magic RGB', () => {
    expect(withOpacity(kiraDefaultPalette.orange, 0.48)).toBe('rgba(252,163,17,0.48)');
    expect(withOpacity(kiraDefaultPalette.alabasterGrey, 0.9)).toBe('rgba(229,229,229,0.9)');
  });
});
