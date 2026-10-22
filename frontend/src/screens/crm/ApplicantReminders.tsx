import { useEffect, useState } from 'react';
import type { ApplicantReminders as Reminders } from '../../generated/ApplicantReminders';
import type { ApplicantRow } from '../../generated/ApplicantRow';
import { applicantRemindersFixture } from './applicantFixture';
import type { ApplicantViewer } from '../../generated/ApplicantViewer';
import { presenceText, viewersOf } from './ApplicantPresence';

/** 約束架電日時の何分前から出すか */
export const REMINDER_LEAD_MS = 5 * 60_000;
const POLL_MS = 30_000;
const TICK_MS = 15_000;

function appointmentMs(raw: string): number {
  return new Date(/^\d{11,}$/.test(raw) ? Number(raw) : raw).getTime();
}
/** 5 分前を過ぎた応募。約束あり連絡から移すとサーバの結果から外れて消える */
export function dueReminders(items: readonly ApplicantRow[], now: number): ApplicantRow[] {
  return items.filter(i => { const t = appointmentMs(i.appointment); return !Number.isNaN(t) && t - REMINDER_LEAD_MS <= now; });
}
function when(raw: string, now: number): string {
  const t = appointmentMs(raw);
  const time = new Intl.DateTimeFormat('ja-JP', { timeZone: 'Asia/Tokyo', ...(Math.abs(t - now) > 12 * 3600_000 ? { dateStyle: 'short' as const } : {}), timeStyle: 'short' }).format(t);
  const min = Math.round((t - now) / 60_000);
  return `${time}（${min > 0 ? `あと${String(min)}分` : min === 0 ? '今' : `${String(-min)}分過ぎ`}）`;
}

/**
 * 約束あり連絡の応募を、約束架電日時の 5 分前から画面の上に出し続ける。
 * 応募画面を開いている全員に出す。タブが隠れている間は読み直さない。
 */
export function ApplicantReminderBand({ fixture, refresh, onOpen, viewers }: { fixture: boolean; refresh: number; onOpen: (row: ApplicantRow) => void; viewers: readonly ApplicantViewer[] }) {
  const [data, setData] = useState<Reminders | null>(null);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    let abort: AbortController | null = null;
    const load = () => {
      if (document.visibilityState === 'hidden') return;
      abort?.abort();
      const current = new AbortController();
      abort = current;
      const work = fixture ? Promise.resolve(applicantRemindersFixture()) : fetch('/api/crm/applicants/reminders', { signal: current.signal, credentials: 'same-origin' }).then(async r => { if (!r.ok) throw new Error(String(r.status)); return await r.json() as Reminders; });
      // 読めないときは前の結果を残す (通知が勝手に消えないように)
      void work.then(d => { if (!current.signal.aborted) setData(d); }).catch(() => { /* 次の周回で読み直す */ });
    };
    load();
    const poll = window.setInterval(load, POLL_MS);
    document.addEventListener('visibilitychange', load);
    return () => { abort?.abort(); window.clearInterval(poll); document.removeEventListener('visibilitychange', load); };
  }, [fixture, refresh]);
  useEffect(() => { const t = window.setInterval(() => { setNow(Date.now()); }, TICK_MS); return () => { window.clearInterval(t); }; }, []);
  const due = dueReminders(data?.items ?? [], now);
  if (due.length === 0) return null;
  const more = (data?.total ?? 0) > (data?.items.length ?? 0);
  return <section className="applicant-reminders" role="alert" aria-label="約束架電の時間">
    <strong>約束架電の時間です</strong>
    <ul>{due.map(r => <li key={r.id}><button type="button" onClick={() => { onOpen(r); }}>{r.name || '氏名未登録'}</button><span>{when(r.appointment, now)}</span>{r.deal && <span className="applicant-reminders-deal">{r.deal}</span>}<ReminderPresence text={presenceText(viewersOf(viewers, r.id, r.phone))} /></li>)}</ul>
    {more && <p>ほかにも約束あり連絡の応募があります。応募一覧で確認してください。</p>}
  </section>;
}
function ReminderPresence({ text }: { text: string }) {
  return text ? <span className="applicant-reminders-presence">{text}</span> : null;
}
