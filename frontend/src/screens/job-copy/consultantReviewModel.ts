function validDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const parsed = new Date(`${value}T00:00:00Z`);
  return Number.isFinite(parsed.getTime()) && parsed.toISOString().slice(0, 10) === value;
}

export function observationWindowError(start: string, end: string): string | null {
  if (!start && !end) return null;
  if (!start || !end) return '観測開始日と終了日を両方入力してください。';
  if (!validDate(start) || !validDate(end)) return '観測期間に有効な日付を入力してください。';
  return start > end ? '観測終了日は開始日以降にしてください。' : null;
}
