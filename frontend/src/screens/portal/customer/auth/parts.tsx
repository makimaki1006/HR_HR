import { useState } from 'react';
import { SUPPORT } from '../../fixtures';
import { Button } from '../../ui/kit';

export { BTN_LG, CheckRow, Fold, Heading, INPUT_LG, IconCopy, NETWORK_ERROR, PasswordInput, TOUCH } from './basics';

/** 通信中の表示を試す。押して少し待ってから done を呼ぶ。 */
export function useFakeSend() {
  const [sending, setSending] = useState(false);
  const run = (done: () => void) => {
    setSending(true);
    window.setTimeout(() => { setSending(false); done(); }, 700);
  };
  return { sending, run };
}

const telHref = `tel:${SUPPORT.tel.replace(/-/g, '')}`;
export const mailHref = (subject: string): string => `mailto:${SUPPORT.mail}?subject=${encodeURIComponent(subject)}`;

/** 連絡先。箱にせず 2 行の文字。 */
export function ContactLines({ subject }: { subject: string }) {
  return (
    <div className="space-y-0.5 text-[13px] text-pt-muted">
      <p>
        電話{' '}
        <a className="pt-num text-pt-primary hover:underline" href={telHref}>{SUPPORT.tel}</a>
        <span className="ml-1.5 whitespace-nowrap text-xs text-pt-faint">{SUPPORT.hours}</span>
      </p>
      <p>メール <a className="break-all text-pt-primary hover:underline" href={mailHref(subject)}>{SUPPORT.mail}</a></p>
    </div>
  );
}

export { Button };
