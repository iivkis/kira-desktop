import { describe, expect, it } from 'vitest';
import { SETTINGS_SECTIONS, sectionIsAvailable, formatInstalledMemory, systemValue } from './settings-model';

describe('Settings navigation and truthful host details', () => {
  it('exposes About and Displays in both navigation modes', () => {
    expect(SETTINGS_SECTIONS.find(s => s.id === 'about')?.available).toBe(true);
    expect(SETTINGS_SECTIONS.filter(s => s.available)).toHaveLength(2);
    expect(sectionIsAvailable('about')).toBe(true);
    expect(sectionIsAvailable('displays')).toBe(true);
    expect(sectionIsAvailable('network')).toBe(false);
    expect(sectionIsAvailable('arbitrary-script')).toBe(false);
  });

  it('does not invent missing memory or identity', () => {
    expect(formatInstalledMemory(null)).toBe('Недоступно');
    expect(formatInstalledMemory(0)).toBe('Недоступно');
    expect(formatInstalledMemory(8 * 1024 * 1024)).toContain('ГиБ');
    expect(systemValue(null)).toBe('Недоступно');
    expect(systemValue('Linux')).toBe('Linux');
  });
});
