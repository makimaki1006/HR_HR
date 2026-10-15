import { useEffect, useState } from 'react';
import { isPhoneHearing } from './phoneHearingResponse';
import { apiGet } from '../../api/client';
import type { PhoneHearing as Hearing } from '../../generated/PhoneHearing';
import type { JobCopyRecord } from './data';
import { formatDateJst, formatDateTimeJst } from './format';
import { applicantText } from './listingEventsModel';
import './phone-hearing.css';

export function PhoneHearingContent({ data, loading = false, applicationRows = [] }: { data: Hearing | null; loading?: boolean; applicationRows?: JobCopyRecord['applicationRows'] }) {
  return <details className="lo-card ph-section"><summary>{data ? `電話ヒアリング（記録あり ${String(data.items.length)}件 / 応募 ${String(data.total_applicants)}件）` : '電話ヒアリング'}</summary>
    {loading ? <p role="status">電話ヒアリングを読み込み中です</p> : !data ? <p role="status">電話ヒアリングを読み込めませんでした</p> : <>
      {data.truncated && <p className="lo-note">一部だけ表示しています</p>}
      {!data.items.length && <p>この求人の応募には、電話で聞いた内容の記録がありません</p>}
      {data.items.map(card => <article className="ph-card" key={card.applicant}>
        <h3>応募者：{applicantText(applicationRows.find(row => row.key === card.applicant) ?? { gender: null, age: null, prefecture: null, municipality: null })}</h3>
        <p>応募日：{formatDateJst(card.application_date, '不明')} / 応募媒体：{card.media ?? '記録なし'}</p>
        {card.multi_listing && <p className="lo-note">ほかの求人にも関連する応募</p>}
        <dl>{card.fields.map(field => <div key={field.label}><dt>{field.label}</dt><dd>{field.text}</dd></div>)}</dl>
        {card.notes.length > 0 && <section aria-label="メモ"><h4>メモ</h4>{card.notes.map((note, index) => <div className="ph-note" key={index}><p>{formatDateTimeJst(note.time, '時刻不明')}</p><p>{note.text}</p></div>)}</section>}
      </article>)}
    </>}
  </details>;
}

export function PhoneHearing({ job }: { job: JobCopyRecord }) {
  const [loaded, setLoaded] = useState<{ id: string; data: Hearing | null } | null>(null);
  useEffect(() => {
    if (job.phoneHearing || !job.hubspotId) return;
    const controller = new AbortController();
    void apiGet<unknown>(`/api/job-copy/listings/${encodeURIComponent(job.hubspotId)}/phone-hearing`, { signal: controller.signal, timeoutMs: 120_000 }).then(result => {
      if (!controller.signal.aborted) setLoaded({ id: job.id, data: result.ok && isPhoneHearing(result.data) ? result.data : null });
    });
    return () => { controller.abort(); };
  }, [job.id, job.hubspotId, job.phoneHearing]);
  const current = loaded?.id === job.id ? loaded : null;
  return <PhoneHearingContent applicationRows={job.applicationRows ?? []} data={job.phoneHearing ?? current?.data ?? null} loading={!job.phoneHearing && Boolean(job.hubspotId) && !current} />;
}
