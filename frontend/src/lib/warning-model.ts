/** Keep a stable warning identity while errors appear or disappear. */
export interface UiWarning {
  id: string;
  text: string;
  short: string;
}

export function visibleWarningIndex(warnings: readonly UiWarning[], selectedId: string | null): number {
  if (warnings.length === 0) return -1;
  const position = warnings.findIndex(warning => warning.id === selectedId);
  return position === -1 ? 0 : position;
}

/** Advance by one with a single action. Never insert a no-op "expand" click. */
export function nextWarningId(warnings: readonly UiWarning[], selectedId: string | null): string | null {
  const position = visibleWarningIndex(warnings, selectedId);
  return position === -1 ? null : warnings[(position + 1) % warnings.length].id;
}
