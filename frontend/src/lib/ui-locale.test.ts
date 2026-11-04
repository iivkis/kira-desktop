import { describe, expect, it } from 'vitest';
import { UI_LOCALE, healthLabel, monitorLabel, matchingAppsLabel, russianCountLabel } from './ui-locale';

describe('one built-in user-facing locale', () => {
  it('retains machine status codes but localizes their presentation', () => {
    expect(UI_LOCALE).toBe('ru-RU');
    expect(healthLabel('STALE')).toBe('ДАННЫЕ УСТАРЕЛИ');
    expect(healthLabel('READY')).toBe('ГОТОВО');
    expect(monitorLabel('ALL SYSTEMS NOMINAL')).toBe('ВСЁ В НОРМЕ');
    expect(monitorLabel('NIRI DISCONNECTED')).toBe('NIRI ОТКЛЮЧЁН');
  });

  it('reuses Russian declension for measured counts', () => {
    for (const [count, expected] of [
      [0, '0 реальных замеров'], [1, '1 реальный замер'],
      [2, '2 реальных замера'], [3, '3 реальных замера'],
      [4, '4 реальных замера'], [5, '5 реальных замеров'],
      [11, '11 реальных замеров'], [21, '21 реальный замер'],
      [23, '23 реальных замера'], [114, '114 реальных замеров'],
    ] as const) {
      expect(russianCountLabel(count, 'реальный замер', 'реальных замера', 'реальных замеров'))
        .toBe(expected);
    }
  });

  it('counts launcher matches with Russian plural forms', () => {
    for (const [count, expected] of [
      [0, '0 результатов'], [1, '1 результат'], [2, '2 результата'],
      [5, '5 результатов'], [11, '11 результатов'], [21, '21 результат'],
      [24, '24 результата'], [111, '111 результатов'],
    ] as const) {
      expect(matchingAppsLabel(count)).toBe(expected);
    }
  });
});
