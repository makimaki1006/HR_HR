import codes from './hrhCodeLabels.json';
export function displayCode(key: string, value: string): string {
  const labels = (codes as Record<string, Record<string, string>>)[key];
  if (!labels) return value;
  const raw = value.normalize('NFKC').trim();
  return labels[raw] ?? (/^[+-]?\d+(?:\.\d+)?$/.test(raw) ? '確認が必要' : value);
}
export function holidayValue(fields: ReadonlyMap<string, string>): string | undefined {
  if (fields.get('休日休暇')?.trim()) return fields.get('休日休暇');
  for (const prefix of ['仕事情報補足', '自由項目']) for (let n = 1; n <= 4; n++) {
    if (['休日', '休日・休暇', '休日休暇', '休暇'].includes(fields.get(`${prefix}${String(n)}のタイトル`)?.trim() ?? '') && fields.get(`${prefix}${String(n)}の内容`)?.trim()) return fields.get(`${prefix}${String(n)}の内容`);
  }
  return undefined;
}
