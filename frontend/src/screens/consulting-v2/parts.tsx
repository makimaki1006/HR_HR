import type { ReactNode } from 'react';
import { caseManagementHref, flagTone, hubspotDealUrl, shortFlag } from './format';
import type { FreshMeta } from './types';
import type { Loaded } from './useApi';

export function Kpi({
  label,
  value,
  unit,
  note,
  onClick,
  delta,
}: {
  label: string;
  value: string;
  unit?: string | undefined;
  note?: string | undefined;
  onClick?: (() => void) | undefined;
  delta?: ReactNode;
}) {
  const body = (
    <>
      <span className="cv2-kpi-label">{label}</span>
      <strong>
        {value}
        {unit !== undefined && <span className="cv2-kpi-unit">{unit}</span>}
        {delta}
      </strong>
      {note !== undefined && <small>{note}</small>}
    </>
  );
  return onClick ? (
    <button type="button" className="cv2-kpi" onClick={onClick}>
      {body}
    </button>
  ) : (
    <div className="cv2-kpi">{body}</div>
  );
}

export function FlagChips({ flags, max = 2 }: { flags: string[] | null | undefined; max?: number }) {
  if (!flags || flags.length === 0) return <span className="cv2-dim">—</span>;
  const shown = flags.slice(0, max);
  const rest = flags.length - shown.length;
  return (
    <span className="cv2-flags">
      {shown.map((f) => {
        const t = flagTone(f);
        return (
          <abbr key={f} className={`cv2-flag cv2-flag-${t.tone}`} title={`${f}（${t.word}）`}>
            {shortFlag(f)}
          </abbr>
        );
      })}
      {rest > 0 && (
        <span className="cv2-flag cv2-flag-more" title={flags.join(' ／ ')}>
          他{rest}
        </span>
      )}
    </span>
  );
}

/** 案件管理で同じ案件を開くリンク。実取引 ID があるときだけ ?deal= を付ける (相対 URL) */
export function CaseLink({ dealId, label }: { dealId: string | null | undefined; label?: string }) {
  const { href, linked } = caseManagementHref(dealId);
  return (
    <a className="cv2-case-link" href={href} target="_blank" rel="noopener noreferrer">
      {label ?? (linked ? '案件管理で確認' : '案件管理の一覧を開く')}
      <span aria-hidden="true"> ↗</span>
    </a>
  );
}

/** HubSpot で同じ取引を開くリンク。portal id か取引 ID が無ければ何も出さない */
export function HubspotLink({ portal, dealId }: { portal: string | undefined; dealId: string | null | undefined }) {
  const href = hubspotDealUrl(portal, dealId);
  if (href === null) return null;
  return (
    <a className="cv2-case-link" href={href} target="_blank" rel="noopener noreferrer">
      HubSpot で開く<span aria-hidden="true"> ↗</span>
    </a>
  );
}

/** この画面の元データがまだ無い項目の枠。作った数字は出さない */
export function NoData({ label }: { label: string }) {
  return (
    <div className="cv2-nodata">
      <span>{label}</span>
      <strong>まだデータがありません</strong>
    </div>
  );
}

export function Section({
  title,
  aside,
  id,
  children,
}: {
  title: string;
  aside?: ReactNode;
  id?: string;
  children: ReactNode;
}) {
  return (
    <section className="cv2-section" id={id}>
      <div className="cv2-section-head">
        <h2>{title}</h2>
        {aside !== undefined && <div className="cv2-section-aside">{aside}</div>}
      </div>
      {children}
    </section>
  );
}

/** 読み込み中・失敗・成功を分けて描く。失敗を 0 件に見せない */
export function Async<T>({
  result,
  onRetry,
  children,
}: {
  result: Loaded<T>;
  onRetry: () => void;
  children: (data: T) => ReactNode;
}) {
  if (result.state === 'loading') {
    return (
      <p className="cv2-loading" role="status">
        読み込んでいます…
      </p>
    );
  }
  if (result.state === 'error') {
    return (
      <div className="cv2-error" role="alert">
        <p>読み込めませんでした。</p>
        <p className="cv2-dim">{result.message}</p>
        <button type="button" onClick={onRetry}>
          もう一度読み込む
        </button>
      </div>
    );
  }
  return <>{children(result.data)}</>;
}

function shortWhen(s: string): string {
  return /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}/.test(s) ? s.slice(5, 16) : s;
}

/** データの取得日時と古さ。今のコンサルKPI の鮮度の帯 (setFresh) と同じ線引き。いつのものか分からないときは分からないと書く */
export function Fresh({ meta }: { meta: FreshMeta }) {
  const src = meta.source_as_of ?? '';
  const gen = meta.generated_at ?? '';
  const age = meta.source_age_days ?? null;
  const when = src === '' ? gen : src;
  if (when === '') {
    return (
      <div className="cv2-fresh cv2-fresh-bad" role="status">
        <b>このデータがいつのものか分かりません。</b>
        {meta.today ? <span className="cv2-dim"> 計算の基準日 {meta.today}</span> : null}
      </div>
    );
  }
  const tone = age === null ? 'warn' : age <= 1 ? 'ok' : age <= 3 ? 'warn' : 'bad';
  const word = age === null ? '' : age <= 0 ? '今日' : age === 1 ? '昨日' : `${String(age)}日前`;
  const stamp = when.replace(/^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}):\d{2}$/, '$1');
  return (
    <div className={`cv2-fresh cv2-fresh-${tone}`} role="status">
      {tone === 'ok' ? (
        <>
          <b>{word} {shortWhen(when)}</b> 時点のデータ<span className="cv2-dim">{src === '' ? '（シートを作った時刻）' : '（HubSpot から落とした時刻）'}</span>
        </>
      ) : (
        <b>{word === '' ? '取得日が分からない' : `${word}の`}データです（{stamp}）。</b>
      )}
      <details className="cv2-more">
        <summary>ほかの時刻</summary>
        <p className="cv2-dim">
          {src === '' ? 'シートを作ったのは' : '元データを落としたのは'} {when}。
          {gen !== '' && src !== '' && gen.slice(0, 10) !== src.slice(0, 10) ? `シートを作り直したのは ${gen}（元データはそれより前のもの）。` : ''}
          {meta.today ? `計算の基準日は ${meta.today}（データの新しさとは別物）。` : ''}
          {meta.update_schedule ? `自動更新: ${meta.update_schedule}。` : ''}
          {tone === 'bad' ? '元データが4日以上前なので、この案内を赤くしています。' : '更新が止まると、この案内が赤くなります（元データが4日以上前）。'}
        </p>
      </details>
    </div>
  );
}
