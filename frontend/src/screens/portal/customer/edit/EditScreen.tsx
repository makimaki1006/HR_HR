import { useEffect } from 'react';
import { actions, useData } from '../../data';
import { ApplicantList } from '../list/ApplicantList';

// 本物の一覧の上に、s1 の 3 番目の応募者の詳細を開いた状態で見せる。
export function EditScreen({ next }: { next: 'ok' | 'conflict' | 'failed' }) {
  const d = useData();
  useEffect(() => { actions.setNextSave(next); }, [next]);
  const row = d.rows.s1?.[2];
  return <ApplicantList openAtStart={row?.rowNumber} />;
}
