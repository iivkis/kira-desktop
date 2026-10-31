export function formatClock(date: Date, timeZone?: string, showSeconds = false): string {
  return new Intl.DateTimeFormat('en-GB', {
    hour: '2-digit',
    minute: '2-digit',
    ...(showSeconds ? { second: '2-digit' as const } : {}),
    hourCycle: 'h23',
    ...(timeZone ? { timeZone } : {}),
  }).format(date);
}
