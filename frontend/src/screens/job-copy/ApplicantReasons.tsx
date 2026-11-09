import type { ApplicationRow, CopyVersion, JobCopyRecord } from './data';
import { APPLICATION_TEXT_SOURCES, reasonSourceLabels } from './applicantReasonsModel';
import { formatDateJst } from './format';
import { maskPersonalDetails } from './personalText';
import { MASK_NOTE } from './reasonCategories';
import { SelectedReasonTables } from './SelectedReasonTables';
import { selectionNote, versionApplications } from './selectedReasonsModel';
import './applicant-reasons.css';

export function ApplicationReasonTexts({ job, rows }: { job: JobCopyRecord; rows?: readonly ApplicationRow[] }) {
  const items = (job.applicantReasons?.items ?? []).filter(item => APPLICATION_TEXT_SOURCES.includes(item.sourceProperty) && (!rows || rows.some(row => row.key === item.applicant)));
  const recorded = APPLICATION_TEXT_SOURCES.reduce((sum, property) => sum + (job.applicantReasons?.sourceCounts[property]?.nonblank ?? 0), 0);
  const emptyText = !job.applicantReasons?.available ? '応募理由の文章は未取得です。' : job.applicantReasons.truncated && recorded > 0 ? '読み込み上限により、表示できる応募理由の文章はありません。' : '応募理由の文章は記録されていません。';
  const groups = new Map<string, typeof items>();
  for (const item of items) {
    const key = item.applicant ?? item.id;
    groups.set(key, [...(groups.get(key) ?? []), item]);
  }
  return <details className="cr-texts"><summary>内容を見る（応募ごとの文章・社内確認用）</summary><p>{MASK_NOTE}顧客向けの印刷には含めません。</p>
    {job.applicantReasons?.truncated && <p>文章は読み込み上限のため一部を表示しています。</p>}
    {groups.size === 0 ? <p>{emptyText}</p> : <ol className="ar-texts">{[...groups].map(([key, texts]) => <li key={key}><p>応募日: {formatDateJst(texts[0]?.applicationDate, '不明')}</p>{texts.map(item => <div key={item.id}><p>{reasonSourceLabels[item.sourceProperty]}</p><blockquote>{maskPersonalDetails(item.text)}</blockquote></div>)}</li>)}</ol>}
  </details>;
}

export function ApplicantReasons({ job, before, after }: { job: JobCopyRecord; before?: CopyVersion | undefined; after?: CopyVersion | undefined }) {
  return <section className="ar-reasons" aria-label="応募理由の集計"><h2>応募理由</h2><p>{selectionNote}</p>
    <SelectedReasonTables job={job} /><ApplicationReasonTexts job={job} />
    <div className="ar-pair">{[{ title: '比較元', version: before }, { title: '比較先', version: after }].map(({ title, version }) => {
      if (!version) return <section key={title}><h3>{title}</h3><p>本文未取得</p></section>;
      const rows = versionApplications(job, version.id);
      return <section key={title}><h3>{title}: {version.label}</h3><p>応募 {job.applicationRows ? `${String(rows.length)}件` : '未取得'}</p><SelectedReasonTables job={job} rows={rows} /><ApplicationReasonTexts job={job} rows={rows} /></section>;
    })}</div>
  </section>;
}
