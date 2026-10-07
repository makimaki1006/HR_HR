import type { ReactNode } from 'react';

/**
 * 取り込み操作 (課金CSV など) をまとめる「データ取込」。主作業の邪魔にならないよう閉じた状態で置く。
 * 計画 PR3 で HubSpot取込・媒体JSON・外部文面もこの中へ移す。
 */
export function JobCopyDataImport({ children }: { children: ReactNode }) {
  return <details className="jc-data-import"><summary>データ取込</summary>
    <div className="jc-data-import-body">{children}</div>
  </details>;
}
