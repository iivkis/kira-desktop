/** Sections are declared once for both native window navigation and popup
 * selector. A disabled destination is never represented as a working page. */
export const SETTINGS_SECTIONS = [
  { id: 'about', label: 'О системе', available: true },
  { id: 'appearance', label: 'Внешний вид', available: false },
  { id: 'displays', label: 'Дисплеи', available: false },
  { id: 'network', label: 'Сеть', available: false },
  { id: 'devices', label: 'Устройства', available: false },
] as const;

export type SettingsSectionId = (typeof SETTINGS_SECTIONS)[number]['id'];

export function sectionIsAvailable(id: string): id is SettingsSectionId {
  return SETTINGS_SECTIONS.some(section => section.id === id && section.available);
}

export function formatInstalledMemory(kib: number | null): string {
  if (kib === null || !Number.isSafeInteger(kib) || kib <= 0) return 'Недоступно';
  const gib = kib / (1024 * 1024);
  return gib >= 1 ? gib.toLocaleString('ru-RU', { maximumFractionDigits: 1 }) + ' ГиБ'
    : Math.round(kib / 1024).toLocaleString('ru-RU') + ' МиБ';
}

export function systemValue(value: string | null | undefined): string {
  return value && value.trim() ? value : 'Недоступно';
}
