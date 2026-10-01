import { useState } from 'react';
import type { CrmMetadataResponse } from '../../generated/CrmMetadataResponse';
import { apiGet, ApiHttpError, AuthRequiredError } from '../../api/client';
import './metadata.css';

export function CrmMetadataPanel({ metadata, onLoaded, disabled }: {
  metadata: CrmMetadataResponse | null; onLoaded: (metadata: CrmMetadataResponse) => void; disabled: boolean;
}) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [elapsed, setElapsed] = useState<number | null>(null);
  const [pipelineId, setPipelineId] = useState('');
  const [stageId, setStageId] = useState('');
  const pipeline = metadata?.pipelines.find(item => item.id === pipelineId);

  async function load(refresh: boolean) {
    if (loading) return;
    setLoading(true); setError('');
    const started = performance.now();
    const result = await apiGet<CrmMetadataResponse>(`/api/crm/metadata${refresh ? '?refresh=true' : ''}`, { timeoutMs: 35_000 });
    setElapsed(Math.round(performance.now() - started));
    setLoading(false);
    if (!result.ok) {
      setError(result.error instanceof AuthRequiredError || (result.error instanceof ApiHttpError && result.error.status === 401)
        ? 'Google Workspaceでログインしてから取得してください。'
        : result.error instanceof ApiHttpError && result.error.status === 403
          ? 'このアカウントにはCRM定義の読み取り権限がありません。'
          : 'HubSpot定義を取得できませんでした。接続設定と権限を確認して再試行してください。');
      return;
    }
    onLoaded(result.data);
    setPipelineId(''); setStageId('');
  }

  return <details className="crm-metadata-panel"><summary>HubSpot連携・速度確認 <span>{loading ? '取得中…' : metadata ? 'API取得済み' : '固定定義を使用中'}</span></summary>
    <div className="crm-metadata-content">
      <p>顧客・案件の値は架空です。ボタンで実HubSpotの項目定義・パイプライン・ステージを読み取り、入力欄の項目名と選択肢に反映します。発信・保存はデモです。</p>
      <div className="crm-metadata-actions"><button disabled={loading || disabled} onClick={() => { void load(false); }}>HubSpot定義を取得</button>
        <button disabled={loading || disabled} onClick={() => { void load(true); }}>キャッシュを使わず再取得</button></div>
      {error && <p role="alert">{error}{metadata ? ' 前回取得した定義を表示しています。' : ' 固定定義の表示を継続しています。'}</p>}
      {elapsed !== null && <p role="status">ブラウザ往復: {elapsed} ms{metadata && !error && <> · Rust取得処理: {metadata.total_ms} ms · HubSpot取得: {metadata.hubspot_ms} ms · {metadata.cache_hit ? '60秒キャッシュ利用（HubSpot通信なし）' : 'HubSpotから取得'}</>}</p>}
      {metadata && <><small>取得日時: {metadata.fetched_at} · プロパティ {metadata.properties.length}件 · パイプライン {metadata.pipelines.length}件</small>
        <div className="crm-metadata-selectors"><label>HubSpotパイプライン（定義の確認）<select value={pipelineId} onChange={event => { setPipelineId(event.target.value); setStageId(''); }}>
          <option value="">選択してください</option>{metadata.pipelines.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}</select></label>
        <label>HubSpotステージ（定義の確認）<select value={stageId} disabled={!pipeline} onChange={event => { setStageId(event.target.value); }}>
          <option value="">選択してください</option>{pipeline?.stages.map(stage => <option key={stage.id} value={stage.id}>{stage.label}</option>)}</select></label></div>
        <small>この選択は定義の確認用です。架空の案件にパイプラインやステージを設定しません。入力済みの旧選択肢は「既存値」として保持し、デモ記録時に確認します。</small></>}
      <small>「1件ずつの表示」「基準のCRM画面」は固定データの操作デモです。API定義の反映はこの連続架電画面が対象です。</small>
    </div>
  </details>;
}
