// ⑦ 84 列 CSV (旧 static/jobgen.html の downloadCsv)。
// UTF-8 BOM 付き・ヘッダ 1 行 + データ 1 行・CRLF。列順は row のキー順 (= HRHACKER_COLUMNS)。

function quote(v: string | null | undefined): string {
  const s = v ?? '';
  return /[",\n\r]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s;
}

export function buildHrhackerCsv(row: Record<string, string>): string {
  const cols = Object.keys(row);
  return (
    '﻿' +
    cols.map(quote).join(',') +
    '\r\n' +
    cols.map((c) => quote(row[c])).join(',') +
    '\r\n'
  );
}

export function hrhackerCsvFileName(nowMs: number): string {
  return 'hrhacker_84col_' + String(nowMs) + '.csv';
}

/** ブラウザでダウンロードさせる (DOM 依存。テストは buildHrhackerCsv を見る)。 */
export function downloadHrhackerCsv(row: Record<string, string>): void {
  const blob = new Blob([buildHrhackerCsv(row)], { type: 'text/csv;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = hrhackerCsvFileName(Date.now());
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}
