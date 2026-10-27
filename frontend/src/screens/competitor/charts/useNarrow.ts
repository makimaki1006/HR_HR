import { useEffect, useState } from 'react';

const QUERY = '(max-width: 640px)';

const read = (): boolean =>
  typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia(QUERY).matches;

/** 画面幅が狭い (スマホ) とき true。狭いときは図の縦横比を変えて、縮んでも文字とプロットが重ならないようにする。 */
export function useNarrow(): boolean {
  const [narrow, setNarrow] = useState(read);
  useEffect(() => {
    if (typeof window.matchMedia !== 'function') return undefined;
    const mq = window.matchMedia(QUERY);
    const on = (): void => { setNarrow(mq.matches); };
    on();
    mq.addEventListener('change', on);
    return () => { mq.removeEventListener('change', on); };
  }, []);
  return narrow;
}
