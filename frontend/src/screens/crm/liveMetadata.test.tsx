import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { CrmMetadataResponse } from '../../generated/CrmMetadataResponse';
import { CrmMetadataContext, metadataDealDefinitions } from './liveMetadata';
import { MocPropertyInput } from './ContactProperties';
import { CrmMetadataPanel } from './CrmMetadataPanel';
import { emptyBatchDraft, validateBatchDraft } from './batchModel';

const metadata: CrmMetadataResponse = {
  properties: [{ object_type: 'deals', name: 'bpo_42', label: '実アカウントの会話温度', property_type: 'enumeration', field_type: 'select',
    options: [{ label: '前向き（実ラベル）', value: 'positive_v2', hidden: false }] }],
  pipelines: [{ id: 'pipeline-17', label: 'BPO架電パイプライン', stages: [{ id: 'stage-23', label: '採用ヒアリング済み' }] }],
  fetched_at: '2026-10-01T00:00:00Z', hubspot_ms: 210, total_ms: 220, cache_hit: false,
};

describe('live CRM definitions', () => {
  it('uses API labels and option values, and validates against the same definitions', () => {
    const html = renderToStaticMarkup(<CrmMetadataContext value={metadata}><MocPropertyInput recordName="架空の担当者" definitionName="bpo_42" value="positive_v2" onChange={() => undefined} /></CrmMetadataContext>);
    expect(html).toContain('実アカウントの会話温度');
    expect(html).toContain('value="positive_v2" selected=""');
    expect(html).toContain('前向き（実ラベル）');
    expect(validateBatchDraft({ ...emptyBatchDraft(), result: 'connected', interest: 'positive_v2' }, metadataDealDefinitions(metadata))).toBeNull();
    expect(validateBatchDraft({ ...emptyBatchDraft(), result: 'connected', interest: '高（前向き）' }, metadataDealDefinitions(metadata))).toBe('選択肢を確認してください。');
  });
  it('reports absent API properties instead of falling back to bundled definitions', () => {
    expect(metadataDealDefinitions(metadata).bpo_49).toBeUndefined();
    const html = renderToStaticMarkup(<CrmMetadataContext value={metadata}><MocPropertyInput recordName="架空" definitionName="bpo_49" value="" onChange={() => undefined} /></CrmMetadataContext>);
    expect(html).toContain('取得したHubSpot定義にありません');
    expect(html).not.toContain('募集人数（アポ用）');
  });
  it('initially renders static-definition status and only offers explicit load actions', () => {
    const html = renderToStaticMarkup(<CrmMetadataPanel metadata={null} onLoaded={() => undefined} disabled={false} />);
    expect(html).toContain('固定定義を使用中');
    expect(html).toContain('HubSpot定義を取得');
    expect(html).not.toContain('ブラウザ往復');
    const loaded = renderToStaticMarkup(<CrmMetadataPanel metadata={metadata} onLoaded={() => undefined} disabled={false} />);
    expect(loaded).toContain('BPO架電パイプライン');
    expect(loaded).toContain('定義の確認用');
  });
});
