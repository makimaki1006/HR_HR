import { knownHeadings } from './hrhCopy';
import type { BodySection } from './hrhCopy';
// Keep values and continuation lines unchanged; only split actual labelled lines.
// Two shapes start a new item: 「見出し：値」, and a heading alone on a line with the text on the next lines.
// A line alone counts as a heading when it is a known heading (hrhCopy). In a body that already uses
// that shape (at least one known heading alone on a line), other short heading-like lines count too:
// after a blank line, no sentence punctuation, and text on the next line.
const SENTENCE_MARK = /[。、，．！？!?：:]/;
export function jobCopySections(body: string): BodySection[] {
  const lines = body.split(/\r?\n/);
  const alone = (index: number) => lines[index]?.trim() ?? '';
  const textFollows = (index: number) => alone(index + 1) !== '';
  const usesBareHeadings = lines.some((_, index) => knownHeadings.has(alone(index)) && textFollows(index));
  const bareHeading = (index: number): boolean => {
    const text = alone(index);
    if (!text || !textFollows(index)) return false;
    if (knownHeadings.has(text)) return true;
    return usesBareHeadings && text.length <= 20 && !SENTENCE_MARK.test(text) && (index === 0 || alone(index - 1) === '');
  };
  const sections: BodySection[] = [];
  lines.forEach((line, index) => {
    const previous = sections.at(-1);
    const field = /^([^：:]{1,40})[：:]\s*(.*)$/.exec(line);
    if (field && !/^https?$|^\d+$/i.test(field[1]?.trim() ?? '')) {
      sections.push({ heading: field[1]?.trim() ?? '', text: field[2] ?? '' });
    } else if (bareHeading(index)) sections.push({ heading: alone(index), text: '' });
    else if (previous) previous.text += `\n${line}`;
    else sections.push({ heading: '仕事内容', text: line });
  });
  return sections.filter(section => section.heading || section.text.trim());
}
export function JobCopyBody({ body, sections }: { body: string | null; sections?: BodySection[] | undefined }) {
  if (!body?.trim()) return <p className="jc-notice" role="status">選んだ本文は未取得です。別の本文を選ぶか、時間を置いて再取得してください。</p>;
  return <div className="jc-job-sheet" aria-label="求人票">{(sections ?? jobCopySections(body)).map((section, index) => <section key={index} className={['仕事内容', 'キャッチコピー', '案件名', '給与詳細'].includes(section.heading) ? 'jc-sheet-wide' : ''}><h3>{section.heading === 'Indeed表示職種名' ? '職種' : section.heading}</h3><p>{section.text.trim() || '未取得'}</p></section>)}</div>;
}
