import type { BodySection } from './hrhCopy';
// Keep values and continuation lines unchanged; only split actual labelled lines.
export function jobCopySections(body: string): BodySection[] {
  const sections: BodySection[] = [];
  for (const line of body.split(/\r?\n/)) {
    const previous = sections.at(-1);
    const field = /^([^：:]{1,40})[：:]\s*(.*)$/.exec(line);
    if (field && !/^https?$|^\d+$/i.test(field[1]?.trim() ?? '')) {
      sections.push({ heading: field[1]?.trim() ?? '', text: field[2] ?? '' });
    } else if (previous) previous.text += `\n${line}`;
    else sections.push({ heading: '仕事内容', text: line });
  }
  return sections.filter(section => section.heading || section.text.trim());
}
export function JobCopyBody({ body, sections }: { body: string | null; sections?: BodySection[] | undefined }) {
  if (!body?.trim()) return <p className="jc-notice" role="status">この版の本文は未取得です。別の版を選ぶか、時間を置いて再取得してください。</p>;
  return <div className="jc-job-sheet" aria-label="求人票">{(sections ?? jobCopySections(body)).map((section, index) => <section key={index} className={['仕事内容', 'キャッチコピー', '案件名', '給与詳細'].includes(section.heading) ? 'jc-sheet-wide' : ''}><h3>{section.heading === 'Indeed表示職種名' ? '職種' : section.heading}</h3><p>{section.text.trim() || '未取得'}</p></section>)}</div>;
}
