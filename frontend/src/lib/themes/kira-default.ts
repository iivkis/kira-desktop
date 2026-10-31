/** Canonical Kira Default palette. Five colors supplied by the project owner.
 * CSS semantic tokens and contrast combinations are contract-tested.
 */
export const kiraDefaultPalette = Object.freeze({
  black: '#0a0908',
  nightBordeaux: '#49111c',
  whiteSmoke: '#f2f4f3',
  dustyTaupe: '#a9927d',
  stoneBrown: '#5e503f',
} as const);

/** Semantic roles, including safe reuse of Dusty Taupe as both secondary text
 * and focus accent. Stone Brown is for structural borders, never small text.
 */
export const kiraDefaultRoles = Object.freeze({
  background: kiraDefaultPalette.black,
  surface: kiraDefaultPalette.nightBordeaux,
  muted: kiraDefaultPalette.dustyTaupe,
  accent: kiraDefaultPalette.dustyTaupe,
  text: kiraDefaultPalette.whiteSmoke,
  earth: kiraDefaultPalette.stoneBrown,
} as const);

export type KiraThemeRole = keyof typeof kiraDefaultRoles;
