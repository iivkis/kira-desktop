/** Canonical Kira Default palette. CSS values are contract-tested against this file. */
export const kiraDefaultPalette = Object.freeze({
  spaceIndigo: '#22223b',
  dustyGrape: '#4a4e69',
  lilacAsh: '#9a8c98',
  almondSilk: '#c9ada7',
  parchment: '#f2e9e4',
} as const);

/** Semantic roles shared by the shell and future trusted widgets. */
export const kiraDefaultRoles = Object.freeze({
  background: kiraDefaultPalette.spaceIndigo,
  surface: kiraDefaultPalette.dustyGrape,
  muted: kiraDefaultPalette.lilacAsh,
  accent: kiraDefaultPalette.almondSilk,
  text: kiraDefaultPalette.parchment,
} as const);

export type KiraThemeRole = keyof typeof kiraDefaultRoles;
