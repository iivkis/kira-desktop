import { describe, expect, it } from 'vitest';
import { nextWarningId, visibleWarningIndex, type UiWarning } from './warning-model';

const warnings: UiWarning[] = [
  { id: 'config', text: 'Ошибка конфигурации', short: 'КОНФИГ' },
  { id: 'launcher', text: 'Лаунчер недоступен', short: 'ЛАУНЧЕР' },
  { id: 'calendar', text: 'Календарь недоступен', short: 'КАЛЕНДАРЬ' },
];

describe('Top Bar warning selection', () => {
  it('does not invent a selection when there are no warnings', () => {
    expect(visibleWarningIndex([], 'config')).toBe(-1);
    expect(nextWarningId([], 'config')).toBeNull();
  });

  it('advances on the first click and wraps after the last warning', () => {
    expect(visibleWarningIndex(warnings, null)).toBe(0);
    expect(nextWarningId(warnings, null)).toBe('launcher');
    expect(nextWarningId(warnings, 'launcher')).toBe('calendar');
    expect(nextWarningId(warnings, 'calendar')).toBe('config');
  });

  it('preserves the displayed warning when other errors are added or removed', () => {
    expect(visibleWarningIndex(warnings.slice(1), 'calendar')).toBe(1);
    expect(visibleWarningIndex(warnings, 'calendar')).toBe(2);
    expect(visibleWarningIndex(warnings.slice(0, 2), 'calendar')).toBe(0);
    expect(nextWarningId(warnings.slice(0, 2), 'calendar')).toBe('launcher');
  });

  it('keeps a single warning selected without promising an empty next action', () => {
    expect(visibleWarningIndex(warnings.slice(0, 1), null)).toBe(0);
    expect(nextWarningId(warnings.slice(0, 1), null)).toBe('config');
  });
});
