import type { ReactNode } from 'react';

/**
 * 取り込み操作（HubSpot・媒体で取得した本文・課金CSV・外部文面）をまとめる「データ取込」。
 * 主作業（一覧とタイムライン）の流れの外に置き、画面上部の「データ取込」ボタンで開く。
 * 閉じても中身は残す（読み込み途中の課金CSVなどを失わないため、hidden で隠すだけにする）。
 */
export function JobCopyDataImport({ open = true, onClose, children }: { open?: boolean; onClose?: () => void; children: ReactNode }) {
  return <section className="jc-data-import" id="job-copy-data-import" aria-labelledby="job-copy-data-import-heading" hidden={!open}>
    <div className="jc-panel-heading"><h2 id="job-copy-data-import-heading" tabIndex={-1}>データ取込</h2>{onClose && <button type="button" className="jc-button" onClick={onClose}>閉じる</button>}</div>
    <p className="jc-muted">取り込んだ内容はこの画面の中だけで使います。HubSpot や媒体には書き込みません。</p>
    <div className="jc-data-import-body">{children}</div>
  </section>;
}
