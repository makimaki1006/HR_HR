import { useEffect, useState } from 'react';
import type { ApplicantPresence } from '../../generated/ApplicantPresence';
import type { ApplicantViewer } from '../../generated/ApplicantViewer';
import { applicantPresenceFixture } from './applicantFixture';

const BEAT_MS = 15_000;
const URL = '/api/crm/applicants/presence';
const HEADERS = { 'Content-Type': 'application/json', 'X-Requested-With': 'fetch' };

/** 電話番号の下 9 桁 (サーバの phone_key と同じ)。 */
export function phoneKey(raw: string): string {
  const digits = raw.replace(/\D/g, '');
  return digits.length < 9 ? '' : digits.slice(-9);
}

/** この応募 (または同じ電話番号の別の応募) を開いているほかの人。 */
export function viewersOf(viewers: readonly ApplicantViewer[], id: string, phone: string): { same: string[]; samePhone: string[] } {
  const key = phoneKey(phone);
  const same = new Set<string>();
  const samePhone = new Set<string>();
  for (const v of viewers) {
    if (v.applicant === id) same.add(v.name);
    else if (key && v.phone_key === key) samePhone.add(v.name);
  }
  return { same: [...same], samePhone: [...samePhone].filter(n => !same.has(n)) };
}

/** 目印の文。開いている人がいなければ空。 */
export function presenceText(on: { same: readonly string[]; samePhone: readonly string[] }): string {
  return [
    on.same.length ? `${on.same.join('・')} さんが開いています` : '',
    on.samePhone.length ? `${on.samePhone.join('・')} さんが同じ電話番号の応募を開いています` : '',
  ].filter(Boolean).join('。');
}

function leave() {
  void fetch(URL, { method: 'POST', credentials: 'same-origin', headers: HEADERS, keepalive: true, body: JSON.stringify({ tab: tabId, applicant: null }) }).catch(() => undefined);
}
/** 画面 (タブ) ごとの id。同じ人の別タブを分けて数える。 */
const tabId = typeof crypto.randomUUID === 'function' ? crypto.randomUUID() : String(Math.random()).slice(2);

/**
 * 開いている応募を 15 秒おきに知らせ、ほかの人が開いている応募を受け取る。
 * タブが隠れていても知らせ続ける (開きっぱなしも目印にする)。閉じるときは取り下げる。
 */
export function useApplicantPresence(fixture: boolean, applicant: string | null, phone: string): ApplicantViewer[] {
  const [viewers, setViewers] = useState<ApplicantViewer[]>([]);
  useEffect(() => {
    if (fixture) return;
    let abort: AbortController | null = null;
    const beat = () => {
      abort?.abort();
      const current = new AbortController();
      abort = current;
      void fetch(URL, { method: 'POST', credentials: 'same-origin', headers: HEADERS, signal: current.signal, body: JSON.stringify({ tab: tabId, applicant, phone }) })
        .then(async r => { if (!r.ok) throw new Error(String(r.status)); return await r.json() as ApplicantPresence; })
        .then(d => { if (!current.signal.aborted) setViewers(Array.isArray(d.viewers) ? d.viewers : []); })
        .catch(() => { /* 次の周回で知らせ直す */ });
    };
    beat();
    const timer = window.setInterval(beat, BEAT_MS);
    window.addEventListener('pagehide', leave);
    return () => { abort?.abort(); window.clearInterval(timer); window.removeEventListener('pagehide', leave); };
  }, [fixture, applicant, phone]);
  // 応募画面を離れたとき (取引への切り替え) も取り下げる。応募を切り替えるたびには送らない (新しい知らせと前後するため)。
  useEffect(() => { if (fixture) return; return leave; }, [fixture]);
  return fixture ? applicantPresenceFixture : viewers;
}
