/** The current preview ships one coherent Russian UI locale.
 * Keep machine status codes stable; translate only their presentation.
 * A future KDL language switch can replace this single seam. */
export const UI_LOCALE = 'ru-RU';

const health: Record<string, string> = {
  'CORE OFF': 'НЕТ CORE',
  STARTING: 'ЗАПУСК',
  'NIRI OFF': 'НЕТ NIRI',
  WAITING: 'ОЖИДАНИЕ',
  CHECKING: 'ПРОВЕРКА',
  'LOW DISK': 'МАЛО МЕСТА',
  'LOW MEM': 'МАЛО ПАМЯТИ',
  'LOW BAT': 'НИЗКИЙ ЗАРЯД',
  BUSY: 'НАГРУЗКА',
  PARTIAL: 'НЕТ ЧАСТИ ДАННЫХ',
  READY: 'ГОТОВО',
};

const monitor: Record<string, string> = {
  'CORE OFFLINE': 'CORE НЕДОСТУПЕН',
  'NIRI DISCONNECTED': 'NIRI ОТКЛЮЧЁН',
  MEASURING: 'ИЗМЕРЕНИЕ',
  'LOW STORAGE': 'МАЛО МЕСТА',
  'MEMORY PRESSURE': 'НЕХВАТКА ПАМЯТИ',
  'LOW BATTERY': 'НИЗКИЙ ЗАРЯД',
  'SYSTEM BUSY': 'ВЫСОКАЯ НАГРУЗКА',
  'PARTIAL DATA': 'НЕПОЛНЫЕ ДАННЫЕ',
  'ALL SYSTEMS NOMINAL': 'ВСЁ В НОРМЕ',
};

export const healthLabel = (code: string): string => health[code] ?? code;
export const monitorLabel = (code: string): string => monitor[code] ?? code;

/** Shared Russian noun agreement for real quantities, not UI decoration. */
export function russianCountLabel(n: number, one: string, few: string, many: string): string {
  const ends = Math.abs(n) % 100;
  const digit = ends % 10;
  const noun = ends >= 11 && ends <= 14 ? many
    : digit === 1 ? one
      : digit >= 2 && digit <= 4 ? few : many;
  return n + ' ' + noun;
}

export function matchingAppsLabel(n: number): string {
  return russianCountLabel(n, 'результат', 'результата', 'результатов');
}
