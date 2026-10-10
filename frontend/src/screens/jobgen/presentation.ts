/** CSV の項目名と検証コードを、利用者が確認する言葉へ置き換える。 */
export function fieldLabel(column: string): string {
  const labels: Record<string, string> = {
    求人id: '求人の管理番号', 店舗id: '事業所の管理番号', 職種id: '職種の管理番号',
    案件名: '求人の見出し', Indeed表示職種名: '検索結果に表示する職種',
    '基本給与 最小': '給与の下限', '基本給与 最大': '給与の上限',
    公開: '掲載状態', 制作メモ: '担当者向けメモ',
    '応募時通知先メールアドレス（カンマ区切りで複数指定可）': '応募を知らせるメールの宛先',
    '自由項目1のタイトル': '休日・休暇の見出し', '自由項目1の内容': '休日・休暇',
    '自由項目2のタイトル': '福利厚生・待遇の見出し', '自由項目2の内容': '福利厚生・待遇',
  };
  return labels[column] ?? column
    .replace(/^自由項目([34])のタイトル$/, '追加情報$1の見出し')
    .replace(/^自由項目([34])の内容$/, '追加情報$1')
    .replace(/仕事情報補足(\d)のタイトル/, '仕事の補足$1の見出し')
    .replace(/仕事情報補足(\d)の内容/, '仕事の補足$1')
    .replace(/条件付き給与(\d) /, '給与の条件$1：')
    .replace(/最小給与|基本給与 最小/, '給与の下限')
    .replace(/最大給与|基本給与 最大/, '給与の上限');
}

export function fieldValue(column: string, value: string | undefined): string {
  if (!value?.trim()) return '未取得';
  if (['求人id', '店舗id', '職種id'].includes(column)) return '設定済み';
  return value;
}

export function reviewIssue(issue: string): string {
  if (issue.startsWith('unsupported_numbers:')) return '原文で確認できない数値があります。元の資料を確認してください。';
  if (issue.startsWith('length_exceeded:')) return '文字数が上限を超えています。文章を短くしてください。';
  if (issue.startsWith('ng_word:')) return '掲載に適さない表現が含まれています。文章を確認してください。';
  if (issue === 'empty_output') return '文章を取得できませんでした。この工程を再実行してください。';
  return '内容を自動で確認できませんでした。元の資料と照らし合わせてください。';
}
