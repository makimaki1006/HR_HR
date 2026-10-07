import { useEffect, useState } from 'react';
import { ZOOM_EMBED_SRC } from './smartEmbed';
import type { ZoomPhone } from './useZoomPhone';
import { formatPhoneForDisplay } from './phone';
import { clock } from './workspaceModel';

const RESULT_LABELS: Record<string, string> = { ended: '通話が終了しました', missed: '応答がありませんでした', rejected: '拒否されました' };

function CallStatus({ zoom }: { zoom: ZoomPhone }) {
  const { call } = zoom;
  const [now, setNow] = useState(() => Date.now());
  const connectedAt = call.phase === 'connected' ? call.connectedAt : null;
  useEffect(() => {
    if (connectedAt === null) return;
    const t = window.setInterval(() => { setNow(Date.now()); }, 1000);
    return () => { window.clearInterval(t); };
  }, [connectedAt]);
  const who = formatPhoneForDisplay(call.number) ?? zoom.pending?.number ?? null;
  if (call.phase === 'idle') {
    return <p className="zp-status" role="status">{zoom.pending
      ? `${formatPhoneForDisplay(zoom.pending.number) ?? zoom.pending.number} への発信を依頼しました。呼び出しの開始を待っています…`
      : '通話していません'}</p>;
  }
  return <div className={`zp-status zp-${call.phase}`} role="status">
    <strong>{call.phase === 'ringing' ? '呼び出し中' : call.phase === 'connected' ? '通話中' : (call.result ? RESULT_LABELS[call.result] : '通話が終了しました')}</strong>
    {who && <span>{who}</span>}
    {call.phase === 'connected' && connectedAt !== null && <span className="zp-timer">{clock((now - connectedAt) / 1000)}</span>}
    {call.phase === 'ended' && call.talkSeconds !== null && <span>通話時間 {clock(call.talkSeconds)}(画面で受けたイベントの時刻差)</span>}
    {call.callId && <small>通話ID {call.callId}</small>}
    {call.phase === 'ended' && <small>通話の結果は中央下の「架電結果」に下書きとして入力できます。HubSpot にはまだ保存されません。</small>}
  </div>;
}

/** 右側に常駐する Zoom Phone Smart Embed。案件を切り替えても iframe は作り直さない */
export function ZoomPhonePanel({ zoom, iframeRef }: { zoom: ZoomPhone; iframeRef: React.RefObject<HTMLIFrameElement | null> }) {
  const unavailable = zoom.embed === 'timeout';
  return <aside className="zp-panel" aria-label="Zoom Phone">
    <div className="zp-head"><span className="crm-eyebrow">ZOOM PHONE</span><h2>電話</h2></div>
    <CallStatus zoom={zoom} />
    {zoom.stalled && zoom.call.phase === 'idle' && <div className="cq-notice cq-warn" role="alert">
      <strong>発信が始まりません</strong>
      <ul>
        <li>下の枠で Zoom にサインインしているか確認してください。</li>
        <li>枠が空白・エラーのときは、管理者の設定(下記)が未了の可能性があります。</li>
        <li>その間は「番号をコピー」または電話番号のリンク(tel:)から発信できます。</li>
      </ul></div>}
    {zoom.embed === 'disabled' && <div className="cq-notice zp-disabled">
      <strong>架空サンプルでは発信できません</strong>
      <p>実データに切り替えると、ここに Zoom Phone が表示されます。</p></div>}
    {zoom.embed !== 'disabled' && <>
      {unavailable && <div className="cq-notice cq-warn" role="alert"><strong>Zoom Phone を読み込めません</strong>
        <p>ネットワーク、または Zoom 側の設定(許可ドメインへの登録・サードパーティからの発信の許可)が未了の可能性があります。</p>
        <p>それまでは、各電話番号の「番号をコピー」または電話番号のリンク(tel:)で発信してください。</p></div>}
      <iframe ref={iframeRef} className="zp-frame" title="Zoom Phone" src={ZOOM_EMBED_SRC}
        allow="microphone; clipboard-read; clipboard-write" onLoad={zoom.onLoad} />
      {zoom.embed === 'loading' && <p className="zp-hint">Zoom Phone を読み込み中…</p>}
      <details className="zp-setup"><summary>初めて使うときの設定(管理者)</summary>
        <ol>
          <li>Zoom Marketplace の「Zoom Phone Smart Embed」(公式アプリ)をアカウントにインストールする。</li>
          <li>インストール時の許可ドメイン(approved domains)に、このアプリのドメインを登録する。</li>
          <li>Zoom 管理画面 アカウント設定 &gt; Zoom Phone タブ の「Automatically Call From Third Party Apps」を有効にする。</li>
          <li>利用者は上の枠で Zoom の資格情報でサインインする。</li>
        </ol>
        <p className="zp-hint">出典: Zoom Developer Docs「Zoom Phone Smart Embed guide」。ブラウザのサードパーティ Cookie の扱いにより、サインインが保持されない場合があります(公式に記載なし・未確認)。</p>
      </details>
    </>}
  </aside>;
}
