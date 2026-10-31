/** Canonical fully opaque Kira Default palette; CSS and GTK must match. */
export const kiraDefaultPalette = Object.freeze({
  black: '#000000ff',
  prussianBlue: '#14213dff',
  orange: '#fca311ff',
  alabasterGrey: '#e5e5e5ff',
  white: '#ffffffff',
} as const);

/** Semantic roles keep color usage accessible. The legacy earth role is
 * preserved as a structural alias for compatibility with older widgets.
 */
export const kiraDefaultRoles = Object.freeze({
  background: kiraDefaultPalette.black,
  surface: kiraDefaultPalette.prussianBlue,
  muted: kiraDefaultPalette.alabasterGrey,
  accent: kiraDefaultPalette.orange,
  text: kiraDefaultPalette.white,
  earth: kiraDefaultPalette.prussianBlue,
} as const);

export type KiraThemeRole = keyof typeof kiraDefaultRoles;
