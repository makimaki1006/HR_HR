import { Component } from 'react';
import type { ErrorInfo, ReactNode } from 'react';

/**
 * CRM の画面ファイルを読み込めなかった (通信が切れた・新しい版を配信した直後で古いファイルが無い等) とき、
 * 白い画面のままにせず、何が起きたかと再読み込みのボタンを出す。
 */
export class ScreenLoadBoundary extends Component<{ children: ReactNode; onReload?: () => void }, { failed: boolean }> {
  override state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('[crm] screen failed to load:', error, info.componentStack);
  }

  override render(): ReactNode {
    if (!this.state.failed) return this.props.children;
    const reload = this.props.onReload ?? (() => { window.location.reload(); });
    return <div role="alert" style={{ padding: 16 }} data-testid="crm-load-error">
      <p style={{ margin: '0 0 8px' }}>画面を読み込めませんでした。通信の状態を確かめて、再読み込みしてください。</p>
      <p style={{ margin: '0 0 12px', fontSize: 12, color: '#5b6773' }}>入力中の架電結果は、このタブを閉じなければ再読み込みの後も残ります。</p>
      <button type="button" onClick={reload}>再読み込み</button>
    </div>;
  }
}
