import { useRef, useState } from 'react';
import type { ChangeEvent } from 'react';
import type { JobCopyRecord } from './data';
import { MAX_CAPTURE_FILE_BYTES, parseMediaCapture } from './mediaCaptureParser';

export function MediaCaptureImport({ onImport }: { onImport: (records: JobCopyRecord[]) => void }) {
  const [records, setRecords] = useState<JobCopyRecord[] | null>(null);
  const [reading, setReading] = useState(false);
  const [error, setError] = useState('');
  const [displayed, setDisplayed] = useState(false);
  const panel = useRef<HTMLDetailsElement>(null);

  async function readCapture(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    setRecords(null); setError(''); setDisplayed(false); setReading(true);
    try {
      if (!file.name.toLowerCase().endsWith('.json') || file.size > MAX_CAPTURE_FILE_BYTES) {
        setError('32MB以下の媒体取得JSONファイルを選んでください。');
        return;
      }
      const input = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer());
      setRecords(parseMediaCapture(input));
    } catch {
      setError('媒体取得データを読み込めませんでした。UTF-8・schemaVersion 1の形式と件数を確認してください。');
    } finally {
      setReading(false); event.target.value = '';
    }
  }

  return <details ref={panel} className="jc-media-import"><summary>媒体で取得した求人本文・画像を確認{displayed && records ? `（${String(records.length)}件表示中）` : ''}</summary>
    <p>個別に取得したJSONを選び、ブラウザのメモリ上に表示します。この操作からAPIへ送信せず、リポジトリやサーバーにも保存しません。再読み込みで消えます。</p>
    <p>取得した本文・画像と、ファイルに含まれる過去のCSV観測版を確認します。掲載更新日時や応募数は補完しません。過去画像の参照を現在取得した表示は、その旨を注記します。</p>
    <label>媒体取得データを読み込む<input type="file" accept=".json,application/json" disabled={reading} onChange={event => { void readCapture(event); }} /></label>
    <p className="jc-muted">32MBまで・1〜59件・求人ごとに画像3点まで・画像はファイル内のデータのみ</p>
    {reading && <p role="status">読み込み中…</p>}
    {error && <p role="alert" className="jc-error">{error}</p>}
    {records && <div><p role="status">{String(records.length)}件の取得データを読み込みました。{displayed ? '画面に表示しています。' : '表示するまで一覧は切り替わりません。'}</p>
      <button className="jc-button" onClick={() => { onImport(records); setDisplayed(true); if (panel.current) { panel.current.open = false; panel.current.querySelector('summary')?.focus(); } }}>取得データを表示</button></div>}
  </details>;
}
