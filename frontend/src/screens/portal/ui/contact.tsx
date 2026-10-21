// 困ったときの連絡先。ログイン前・一覧が読めないとき・設定の画面で同じものを出す。
import { SUPPORT } from '../fixtures';

const telHref = `tel:${SUPPORT.tel.replace(/-/g, '')}`;

/** 連絡先の一覧とスマホ用の大きなボタン。 */
export function ContactBox({ mailSubject = 'ログインについて' }: { mailSubject?: string }) {
  const mailHref = `mailto:${SUPPORT.mail}?subject=${encodeURIComponent(mailSubject)}`;
  return (
    <div>
      <dl className="rounded-md border border-pt-line bg-pt-bg px-4 py-3 text-sm">
        <div className="flex gap-3 py-0.5"><dt className="w-14 shrink-0 text-pt-muted">担当</dt><dd>{SUPPORT.name}</dd></div>
        <div className="flex gap-3 py-0.5"><dt className="w-14 shrink-0 text-pt-muted">電話</dt><dd><a className="text-pt-primary underline" href={telHref}>{SUPPORT.tel}</a>（{SUPPORT.hours}）</dd></div>
        <div className="flex gap-3 py-0.5"><dt className="w-14 shrink-0 text-pt-muted">メール</dt><dd className="break-all"><a className="text-pt-primary underline" href={mailHref}>{SUPPORT.mail}</a></dd></div>
      </dl>
      <div className="mt-3 grid grid-cols-2 gap-2 sm:hidden">
        <a href={telHref} className="flex h-11 items-center justify-center rounded-md bg-pt-primary text-sm font-medium text-white">電話する</a>
        <a href={mailHref} className="flex h-11 items-center justify-center rounded-md border border-pt-line bg-pt-surface text-sm font-medium text-pt-ink">メールで頼む</a>
      </div>
      <p className="mt-3 text-xs leading-relaxed text-pt-muted">時間外のメールも受け付けています。翌営業日の午前中に対応します。</p>
    </div>
  );
}

/** 1 行の連絡先。電話とメールがリンク。 */
export function ContactLine({ prefix = '' }: { prefix?: string }) {
  return (
    <span>
      {prefix}
      {SUPPORT.name}{' '}
      <a className="text-pt-primary underline" href={telHref}>{SUPPORT.tel}</a>
      （{SUPPORT.hours}）{' '}
      <a className="break-all text-pt-primary underline" href={`mailto:${SUPPORT.mail}`}>{SUPPORT.mail}</a>
    </span>
  );
}
