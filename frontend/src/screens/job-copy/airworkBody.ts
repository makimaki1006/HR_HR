/**
 * AirWork の求人票の本文（HubSpot の aw_kyuujinhyou_honbun）を項目に分ける。
 * 本文は夜の取得で XLSX の列から作られ、次の見出しがこの順で行頭に「見出し：」と並ぶ。
 * 項目の中にも「基本給：」「給与：本採用時と同じ」のような行があるので、見出しは決まった順に前から探す。
 */
export const AIRWORK_BODY_LABELS = ['キャッチコピー', '給与', '給与の詳細', '仕事内容', '職種', '求める人材', '勤務時間', '休日休暇', '社会保険・福利厚生', '試用・研修期間', '勤務地', 'アクセス'] as const;
export type AirworkBodyLabel = typeof AIRWORK_BODY_LABELS[number];

/** 見出しごとの中身。給与と仕事内容の見出しが無い本文（仕事内容だけの古い本文など）は null。 */
export function airworkBodyFields(body: string): Map<AirworkBodyLabel, string> | null {
  const lines = body.split(/\r?\n/);
  const starts: { label: AirworkBodyLabel; line: number; first: string }[] = [];
  let cursor = 0;
  for (const label of AIRWORK_BODY_LABELS) {
    for (let index = cursor; index < lines.length; index++) {
      const match = /^(.+?)[：:](.*)$/.exec(lines[index] ?? '');
      if (match?.[1]?.trim() !== label) continue;
      starts.push({ label, line: index, first: match[2] ?? '' });
      cursor = index + 1;
      break;
    }
  }
  if (!starts.some(start => start.label === '給与') || !starts.some(start => start.label === '仕事内容')) return null;
  const fields = new Map<AirworkBodyLabel, string>();
  starts.forEach((start, at) => {
    const end = starts[at + 1]?.line ?? lines.length;
    fields.set(start.label, [start.first, ...lines.slice(start.line + 1, end)].join('\n').trim());
  });
  return fields;
}
