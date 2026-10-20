import { useState, type ReactNode } from 'react';
import { Button, TextInput } from '../../ui/kit';

export function PasswordInput({ id, name, value, onChange, autoComplete, invalid }: {
  id: string;
  name: string;
  value: string;
  onChange: (v: string) => void;
  autoComplete: string;
  invalid?: boolean;
}) {
  const [shown, setShown] = useState(false);
  return (
    <div className="relative">
      <TextInput
        id={id}
        name={name}
        type={shown ? 'text' : 'password'}
        value={value}
        autoComplete={autoComplete}
        aria-invalid={invalid}
        className="pr-20"
        onChange={(e) => { onChange(e.target.value); }}
      />
      <button
        type="button"
        aria-pressed={shown}
        className="absolute right-0 top-0 h-full w-18 rounded-r-md px-3 text-sm text-pt-primary hover:bg-pt-bg"
        onClick={() => { setShown((s) => !s); }}
      >
        {shown ? '隠す' : '表示'}
      </button>
    </div>
  );
}

/** スマホでは高さ 44px、PC では小さめ。 */
export const TOUCH = 'h-11 sm:h-8';

export function Heading({ title, lead }: { title: string; lead?: ReactNode }) {
  return (
    <div className="mb-5">
      <h1 className="text-xl font-semibold text-pt-ink">{title}</h1>
      {lead && <div className="mt-2 text-sm leading-relaxed text-pt-muted">{lead}</div>}
    </div>
  );
}

/** 通信中の表示を試す。押して少し待ってから done を呼ぶ。 */
export function useFakeSend() {
  const [sending, setSending] = useState(false);
  const run = (done: () => void) => {
    setSending(true);
    window.setTimeout(() => { setSending(false); done(); }, 700);
  };
  return { sending, run };
}

export const NETWORK_ERROR = '通信できませんでした。電波の良い場所でもう一度お試しください';

export { ContactBox } from '../../ui/contact';

export { Button };
