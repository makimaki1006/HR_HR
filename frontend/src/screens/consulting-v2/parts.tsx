import type { ReactNode } from 'react';
import { caseManagementHref, flagTone, shortFlag } from './format';
import type { Loaded } from './useApi';

export function Kpi({
  label,
  value,
  unit,
  note,
  onClick,
}: {
  label: string;
  value: string;
  unit?: string;
  note?: string;
  onClick?: () => void;
}) {
  const body = (
    <>
      <span className="cv2-kpi-label">{label}</span>
      <strong>
        {value}
        {unit !== undefined && <span className="cv2-kpi-unit">{unit}</span>}
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
