import { memo, useEffect, useState } from 'react';
import type { RefObject } from 'react';
import { ZOOM_EMBED_SRC } from './smartEmbed';
import type { ZoomPhone } from './useZoomPhone';
import { formatPhoneForDisplay } from './phone';
import { clock } from './workspaceModel';

export const RESULT_LABELS: Record<string, string> = { ended: '通話が終了しました', missed: '応答がありませんでした', rejected: '拒否されました' };

/** `active` の間だけ毎秒描き直し、そのときの時刻を返す (通話中の経過時間の表示用) */
export function useTicking(active: boolean, now: () => number): number {
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!active) return;
    const id = window.setInterval(() => { setTick(n => n + 1); }, 1000);
    return () => { window.clearInterval(id); };
  }, [active]);
  return now();
}

/**
 * 終わった通話と入力欄の関係。
 * selected: 選んでいる架電先の画面から発信した / recorded: 発信した架電先には記録済み / none: どちらでもない
 */
export type CallLink = 'selected' | 'recorded' | 'none';

const LINK_HINTS: Record<CallLink, string> = {
  selected: '通話の結果は中央下の「架電結果」に下書きとして入力できます。HubSpot にはまだ保存されません。',
  recorded: 'この通話の結果は、発信した架電先に記録済みです(HubSpot には未送信)。',
  none: 'この通話は選んでいる架電先と結び付いていません。架電先を選んでから「架電結果」に入力してください。',
};

function CallStatus({ zoom, link }: { zoom: ZoomPhone; link: CallLink }) {
  const { call } = zoom;
  const connectedAt = call.phase === 'connected' ? call.connectedAt : null;
  const now = useTicking(connectedAt !== null, zoom.now);
  const who = formatPhoneForDisplay(call.number) ?? zoom.pending?.number ?? null;
  if (call.phase === 'idle') {
    return <p className="zp-status" role="status">{zoom.pending
      ? `${formatPhoneForDisplay(zoom.pending.number) ?? zoom.pending.number} への発信を依頼しました。呼び出しの開始を待っています…`
      : '通話していません'}</p>;
  }
  return <div className={`zp-status zp-${call.phase}`} role="status">
    <strong>{call.phase === 'ringing' ? '呼び出し中' : call.phase === 'connected' ? '通話中' : (call.result ? RESULT_LABELS[call.result] : '通話が終了しました')}</strong>
    {who && <span>{who}</span>}
    {/* 毎秒変わる時計は読み上げない (状態の変化だけを読み上げる) */}
    {call.phase === 'connected' && connectedAt !== null && <span className="zp-timer" aria-hidden="true">{clock((now - connectedAt) / 1000)}</span>}
    {call.phase === 'ended' && call.talkSeconds !== null && <span data-testid="zp-talk"
      title={`Zoom の画面で見た通話の開始から終了までの目安です${call.callId ? `(Zoom の通話 ID: ${call.callId})` : ''}`}>通話時間 約 {clock(call.talkSeconds)}</span>}
    {call.phase === 'ended' && <small data-testid="zp-result-hint"
      title={link === 'none' ? 'この画面の「架ける番号」から発信した通話だけが、選んだ架電先の入力欄に結び付きます' : undefined}>{LINK_HINTS[link]}</small>}
  </div>;
}

/**
 * 右から開く Zoom Phone Smart Embed の中身。案件を切り替えても、閉じても iframe は作り直さない
 * (閉じている間も画面の外に同じ大きさで置いたままにし、発信の依頼を送れるようにする)
 */
function ZoomPhonePanelImpl({ zoom, iframeRef, link = 'none', onClose }: {
  zoom: ZoomPhone; iframeRef: RefObject<HTMLIFrameElement | null>;
  /** 終わった通話と入力欄の関係 */
  link?: CallLink;
  /** 枠を閉じる (閉じるボタンを出す) */
  onClose?: (() => void) | undefined;
}) {
  const unavailable = zoom.embed === 'timeout';
  return <aside className="zp-panel" aria-label="Zoom Phone">
    <div className="zp-head"><span className="crm-eyebrow">ZOOM PHONE</span><h2>電話</h2>
      {onClose && <button type="button" className="zp-close" onClick={onClose} aria-label="Zoom の枠を閉じる">閉じる</button>}</div>
    <CallStatus zoom={zoom} link={link} />
    {zoom.stalled && zoom.call.phase === 'idle' && <div className="cq-notice cq-warn" role="alert">
      <strong>Zoom が応答しません</strong>
      <ul>
        <li>このパソコンで Zoom アプリが起動していて、サインインしているか確認してください。</li>
        <li>下の枠に「Sign In」が出ているときは、押してサインインしてください。</li>
        <li>下の枠に「No available device」が出ているときは、Zoom アプリにサインインしてから、この画面を再読み込みしてください。</li>
        <li>その間は「番号をコピー」または「端末の電話で発信」から発信できます。</li>
      </ul></div>}
    {zoom.embed === 'disabled' && <div className="cq-notice zp-disabled">
      <strong>架空サンプルでは発信できません</strong>
      <p>実データに切り替えると、ここに Zoom Phone が表示されます。</p></div>}
    {zoom.embed !== 'disabled' && <>
      {unavailable && <div className="cq-notice cq-warn" role="alert"><strong>Zoom Phone を読み込めません</strong>
        <p>ネットワーク、または Zoom 側の設定(許可ドメインへの登録・サードパーティからの発信の許可)が未了の可能性があります。</p>
        <p>それまでは、各電話番号の「番号をコピー」または「端末の電話で発信」で発信してください。</p></div>}
      <iframe ref={iframeRef} className="zp-frame" title="Zoom Phone" src={ZOOM_EMBED_SRC}
        allow="microphone; clipboard-read; clipboard-write" onLoad={zoom.onLoad} />
      {zoom.embed === 'loading' && <p className="zp-hint">Zoom Phone を読み込み中…</p>}
      <details className="zp-setup"><summary>初めて使うときの設定</summary>
        <p className="zp-setup-head">架電する人(パソコンごと)</p>
        <ol>
          <li>このパソコンで Zoom アプリを起動し、架電に使う Zoom のアカウントでサインインしておく。</li>
          <li>初めてのときは、上の枠の中でも同じアカウントでサインインする(「Sign In」を押すと Zoom の画面が開きます)。</li>
          <li>Zoom アプリが起動していないと、上の枠に「No available device」と出て発信できません。</li>
        </ol>
        <p className="zp-setup-head">管理者(最初に 1 回)</p>
        <ol>
          <li>Zoom Marketplace の「Zoom Phone Smart Embed」(公式アプリ)をアカウントにインストールする。</li>
          <li>インストール時の許可ドメインに、このアプリのドメインを登録する。</li>
          <li>Zoom 管理画面 アカウント設定 &gt; Zoom Phone タブ の「Automatically Call From Third Party Apps」を有効にする。</li>
        </ol>
        <p className="zp-hint" title="手順の出典: Zoom の開発者向け資料「Zoom Phone Smart Embed guide」">ブラウザの設定によっては、サインインが保たれないことがあります。</p>
      </details>
    </>}
  </aside>;
}

/** 親 (架電画面) が架電結果の入力のたびに描き直しても、props が同じなら描き直さない */
export const ZoomPhonePanel = memo(ZoomPhonePanelImpl);
