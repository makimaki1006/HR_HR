// パイプライン制御の検証。API はモックし、送った body (旧 JS と同じキー) と状態遷移を具体値で見る。
// fixture は cargo test (src/job_gen/contract.rs) が本物の応答組み立てコードから書き出したもの。
import { describe, expect, it } from 'vitest';
import { ApiDataError, type ApiResult } from '../../api/client';
import fixtures from '../../generated/jobgen/fixtures.json';
import type { JobgenEndpoints, JobgenPath, PostFn } from './api';
import { createPipelineController } from './pipeline';
import { canRunAll, CTL_HINT_READY, initialState, type PipelineState, type StepKey } from './state';
import { createStore } from './store';

const SRC = fixtures.source_text;
const R = fixtures.responses;

interface Call {
  path: JobgenPath;
  body: unknown;
}

type Responder = <P extends JobgenPath>(
  path: P,
  body: JobgenEndpoints[P]['req'],
  n: number,
) => ApiResult<JobgenEndpoints[P]['res']> | null;

const FIXTURE_BY_PATH: Record<JobgenPath, unknown> = {
  '/api/jobgen/normalize': R.normalize,
  '/api/jobgen/extract': R.extract,
  '/api/jobgen/analyze': R.analyze,
  '/api/jobgen/personas': R.personas,
  '/api/jobgen/copy': R.copy,
  '/api/jobgen/images': R.images,
  '/api/jobgen/image_prompts': R.image_prompts,
  '/api/jobgen/mobile': R.mobile,
  '/api/jobgen/hrhacker': R.hrhacker,
  '/api/jobgen/ab': R.ab,
};

/** fixture を返すモック。`override` で特定の呼び出しだけ差し替えられる。 */
function setup(override?: Responder) {
  const calls: Call[] = [];
  const post: PostFn = (path, body) => {
    // 旧 JS と同じく JSON 化して送る想定なので、body は JSON で固定して記録する。
    calls.push({ path, body: JSON.parse(JSON.stringify(body)) });
    const n = calls.filter((c) => c.path === path).length;
    const o = override?.(path, body, n);
    if (o) return Promise.resolve(o);
    return Promise.resolve({ ok: true, data: FIXTURE_BY_PATH[path] } as ApiResult<never>);
  };
  const store = createStore<PipelineState>(initialState());
  const ctl = createPipelineController({ store, post, now: () => new Date('2026-09-30T00:00:00Z') });
  return { calls, store, ctl };
}

async function importAndConfirm(t: ReturnType<typeof setup>) {
  await t.ctl.normalize({ kind: 'free_text', text: SRC });
  t.ctl.setJobTitle(fixtures.job_title);
  t.ctl.setJobTitleConfirmed(true);
}

const P0 = R.personas.personas[0];
const STRATEGY_HINT =
  '月給192,000円〜195,000円で資格手当・処遇改善手当を含む、年間休日110日・週休2日制、高尾駅から徒歩10分、入職後1か月の研修と先輩職員の同行で未経験でも始めやすい、夜勤手当が1回ごとに明示されている';
const AB_SUMMARY = [
  '職種カテゴリ: 介護職',
  '表面の強み: 月給192,000円〜195,000円で資格手当・処遇改善手当を含む、年間休日110日・週休2日制、高尾駅から徒歩10分',
  '裏の強み: 入職後1か月の研修と先輩職員の同行で未経験でも始めやすい、夜勤手当が1回ごとに明示されている',
  'ボトルネック: 夜勤の有無・回数が原文で明確でない、給与レンジが狭く昇給の見通しが書かれていない',
  'ペルソナ: 子育て中の復職希望者、介護未経験の異業種転職者、処遇改善を求めるベテラン',
].join('\n');

describe('取り込み (normalize)', () => {
  it('単一求人の生成後に複数求人を取り込むと、選択前の原文・結果・職種確認を消す', async () => {
    const t = setup();
    await t.ctl.normalize({ kind: 'free_text', text: SRC });
    t.ctl.setJobTitleConfirmed(true);
    await t.ctl.runAll();
    const s = t.store.get();
    const two = { status: 'ok', jobs: [{ title_hint: '倉庫', source_text: '月給250000円' }, { title_hint: '配送', source_text: '月給300000円' }] };
    const ctl = createPipelineController({ store: t.store, post: () => Promise.resolve({ ok: true, data: two } as ApiResult<never>) });
    expect(s.hrhacker).not.toBeNull();
    await ctl.normalize({ kind: 'csv', text: 'dummy' });
    expect(t.store.get().sourceText).toBe('');
    expect(t.store.get().hrhacker).toBeNull();
    expect(t.store.get().jobTitleConfirmed).toBe(false);
    expect(canRunAll(t.store.get())).toBe(false);
  });
  it('free_text は {kind, text} だけを送り、1 件なら自動で選択して職種名欄を先頭行候補で埋める', async () => {
    const t = setup();
    await t.ctl.normalize({ kind: 'free_text', text: SRC });
    expect(t.calls).toEqual([{ path: '/api/jobgen/normalize', body: { kind: 'free_text', text: SRC } }]);
    const s = t.store.get();
    expect(s.jobs).toHaveLength(1);
    expect(s.sourceText).toBe(SRC);
    expect(s.titleHint).toBe('【職種】介護職員（特別養護老人ホーム）');
    expect(s.jobTitle).toBe('【職種】介護職員（特別養護老人ホーム）');
    expect(s.jobTitleConfirmed).toBe(false);
    expect(s.ctlHint).toBe(CTL_HINT_READY);
    expect(s.statusMessage).toBeNull();
    expect(s.normalizing).toBe(false);
    expect(Object.values(s.status).every((v) => v === 'wait')).toBe(true);
  });

  it('サーバの status:"error" は「正規化エラー: <message>」で出す', async () => {
    const t = setup((path) =>
      path === '/api/jobgen/normalize'
        ? { ok: false, error: new ApiDataError(R.error.message, R.error) }
        : null,
    );
    await t.ctl.normalize({ kind: 'url', url: 'https://example.invalid/x' });
    expect(t.calls[0]?.body).toEqual({ kind: 'url', url: 'https://example.invalid/x' });
    expect(t.store.get().statusMessage).toEqual({
      kind: 'err',
      text: '取り込めませんでした: source_text が必要です',
    });
    expect(t.store.get().sourceText).toBe('');
  });

  it('jobs が空なら「求人原文が取得できませんでした。」', async () => {
    const t = setup((path) =>
      path === '/api/jobgen/normalize' ? { ok: true, data: { status: 'ok', jobs: [] } } : null,
    );
    await t.ctl.normalize({ kind: 'csv', text: 'a,b' });
    expect(t.store.get().statusMessage?.text).toBe('求人原文が取得できませんでした。');
  });

  it('複数件なら選ぶまで取り込み済みにならない。選ぶと下流がリセットされる', async () => {
    const two = {
      status: 'ok',
      jobs: [
        { title_hint: '求人A', source_text: 'A の原文' },
        { title_hint: '求人B', source_text: 'B の原文' },
      ],
    };
    const t = setup((path) => (path === '/api/jobgen/normalize' ? { ok: true, data: two } : null));
    await t.ctl.normalize({ kind: 'csv', text: 'dummy' });
    expect(t.store.get().jobs).toHaveLength(2);
    expect(t.store.get().sourceText).toBe('');
    t.ctl.pickJob(1);
    expect(t.store.get().sourceText).toBe('B の原文');
    expect(t.store.get().titleHint).toBe('求人B');
  });
});

describe('一括実行 (①→⑧)', () => {
  it('職種名を確認するまで一括実行は押せない', async () => {
    const t = setup();
    expect(canRunAll(t.store.get())).toBe(false);
    await t.ctl.normalize({ kind: 'free_text', text: SRC });
    expect(canRunAll(t.store.get())).toBe(false);
    t.ctl.setJobTitleConfirmed(true);
    expect(canRunAll(t.store.get())).toBe(true);
    // 職種名を編集すると確認が外れる
    t.ctl.setJobTitle('介護');
    expect(t.store.get().jobTitleConfirmed).toBe(false);
  });

  it('旧 JS と同じ順・同じ body で 14 回呼ぶ (copy/mobile はペルソナごと)', async () => {
    const t = setup();
    await importAndConfirm(t);
    t.calls.length = 0;
    await t.ctl.runAll();

    expect(t.calls.map((c) => c.path)).toEqual([
      '/api/jobgen/extract',
      '/api/jobgen/analyze',
      '/api/jobgen/personas',
      '/api/jobgen/copy',
      '/api/jobgen/copy',
      '/api/jobgen/copy',
      '/api/jobgen/images',
      '/api/jobgen/image_prompts',
      '/api/jobgen/mobile',
      '/api/jobgen/mobile',
      '/api/jobgen/mobile',
      '/api/jobgen/hrhacker',
      '/api/jobgen/ab',
    ]);
    expect(t.calls[0]?.body).toEqual({ source_text: SRC });
    expect(t.calls[1]?.body).toEqual({ source_text: SRC, job_title: '介護職' });
    expect(t.calls[2]?.body).toEqual({ source_text: SRC, analysis: R.analyze.analysis, count: 5 });
    expect(t.calls[3]?.body).toEqual({ persona: P0, analysis: R.analyze.analysis, source_text: SRC });
    expect(t.calls[5]?.body).toEqual({
      persona: R.personas.personas[2],
      analysis: R.analyze.analysis,
      source_text: SRC,
    });
    expect(t.calls[6]?.body).toEqual({ personas: R.personas.personas, source_text: SRC });
    expect(t.calls[7]?.body).toEqual({
      directions: { directions: R.images.directions },
      personas: R.personas.personas,
      source_text: SRC,
    });
    expect(t.calls[8]?.body).toEqual({
      persona: P0,
      facts_text: R.extract.facts_text,
      source_text: SRC,
    });
    expect(t.calls[11]?.body).toEqual({
      source_text: SRC,
      facts: R.extract.facts,
      strategy_hint: STRATEGY_HINT,
    });
    expect(t.calls[12]?.body).toEqual({ summary: AB_SUMMARY, source_text: SRC });
  });

  it('各工程の状態は検証ゲートの結果で決まる', async () => {
    const t = setup();
    await importAndConfirm(t);
    await t.ctl.runAll();
    const s = t.store.get();
    expect(s.status).toEqual({
      extract: 'review', // insurance が rejected
      analyze: 'done',
      personas: 'done',
      copy: 'review', // 女性歓迎 (法令NG) + 120日 (原文にない数値) + 夜勤なし (表現)
      images: 'review', // ⑤b のプロンプトに 30代/40代/1人/2人 (原文にない数値)
      mobile: 'done',
      hrhacker: 'review', // merit が review_required
      ab: 'review', // 夜勤なし (表現レビュー)
    });
    expect(s.running).toBe(false);
    expect(s.curStep).toBeNull();
    expect(s.statusMessage).toBeNull();
    expect(s.failures).toEqual({});
    expect(s.resultReady).toEqual({
      extract: true,
      analyze: true,
      personas: true,
      copy: true,
      images: true,
      mobile: true,
      hrhacker: true,
      ab: true,
    });

    expect(s.facts?.salary?.value).toBe('月給192,000円〜195,000円');
    expect(s.facts?.insurance?.status).toBe('rejected');
    expect(s.factsText).toBe(R.extract.facts_text);
    expect(s.category).toBe('介護職');
    expect(s.knowledgeUsed).toBe(true);
    expect(s.personas.map((p) => p.label)).toEqual([
      '子育て中の復職希望者',
      '介護未経験の異業種転職者',
      '処遇改善を求めるベテラン',
    ]);
    expect(s.copies).toHaveLength(3);
    expect(s.copies[0]?.label).toBe('子育て中の復職希望者');
    expect(s.copies[0]?.copies.map((c) => c.style)).toEqual(['常識破壊', '比較・リアルな声', '感情・共感']);
    expect(s.copies[0]?.ng_violations[0]?.matched).toBe('女性歓迎');
    expect(s.copies[0]?.number_violations[0]?.numbers).toEqual(['120', '120日']);
    expect(s.copies[0]?.expression_warnings[0]?.matched).toBe('夜勤なし');
    expect(s.images.map((d) => d.persona_label)).toEqual(s.personas.map((p) => p.label));
    expect(s.imagePrompts).toHaveLength(3);
    expect(s.imagePrompts[0]?.aspect_ratio).toBe('4:5');
    // ⑤ 本体は通過、⑤b で 3 件 → 統合リストは 3 件
    expect(s.imagesNv).toHaveLength(3);
    expect(s.imagesNv[0]?.numbers).toEqual(['1人', '30代']);
    expect(s.imagesNumberCheck).toBe('checked');
    expect(s.mobile[0]?.lines).toEqual(R.mobile.lines);
    expect(s.hrhacker?.fill_stats).toEqual({
      filled: 11,
      total: 84,
      fact_mapped_filled: 11,
      fact_mapped_total: 13,
    });
    expect(Object.keys(s.hrhacker?.row ?? {})).toHaveLength(84);
    expect(s.hrhacker?.review_required_fields).toEqual(['merit']);
    expect(s.ab?.steps.map((x) => x.metric)).toEqual(['CTR（クリック率）', 'CVR（応募転換率）', 'CPA（応募単価）']);
    expect(s.ab?.expression_warnings).toHaveLength(1);
  });

  it('途中で失敗した工程で止まり、その工程だけ失敗にする', async () => {
    const t = setup((path) =>
      path === '/api/jobgen/personas'
        ? { ok: false, error: new ApiDataError('GEMINI_API_KEY が未設定です', {}) }
        : null,
    );
    await importAndConfirm(t);
    await t.ctl.runAll();
    const s = t.store.get();
    expect(s.status.extract).toBe('review');
    expect(s.status.analyze).toBe('done');
    expect(s.status.personas).toBe('fail');
    expect(s.status.copy).toBe('wait');
    expect(s.failures.personas).toBe('GEMINI_API_KEY が未設定です');
    expect(s.statusMessage).toEqual({
      kind: 'err',
      text: '③ ペルソナ設計 で停止しました。修正後、その工程だけ再実行できます。',
    });
    expect(s.running).toBe(false);
    expect(t.calls.map((c) => c.path)).toEqual([
      '/api/jobgen/normalize',
      '/api/jobgen/extract',
      '/api/jobgen/analyze',
      '/api/jobgen/personas',
    ]);
  });
});

describe('単工程の再実行と「要再実行」の伝播', () => {
  it('②を再実行すると③〜⑧が要再実行になり、①はそのまま', async () => {
    const t = setup();
    await importAndConfirm(t);
    await t.ctl.runAll();
    t.ctl.toggleConfirm('personas', true);
    expect(t.store.get().confirmed.personas).toBe(true);

    await t.ctl.runOne('analyze');
    const s = t.store.get();
    expect(s.status.extract).toBe('review');
    expect(s.status.analyze).toBe('done');
    for (const k of ['personas', 'copy', 'images', 'mobile', 'hrhacker', 'ab'] as StepKey[]) {
      expect(s.status[k], k).toBe('stale');
      expect(s.staleSource[k], k).toBe('analyze');
      expect(s.confirmed[k], k).toBe(false);
    }
    expect(s.staleSource.analyze).toBeUndefined();
    // 古い結果はそのまま見える (帯が付くだけ)
    expect(s.personas).toHaveLength(3);
  });

  it('①だけ再実行すると ⑥⑦ とその下流 (④⑤⑧は ③ 経由ではないので影響なし) が要再実行', async () => {
    const t = setup();
    await importAndConfirm(t);
    await t.ctl.runAll();
    await t.ctl.runOne('extract');
    const s = t.store.get();
    expect(s.status.mobile).toBe('stale');
    expect(s.status.hrhacker).toBe('stale');
    expect(s.status.analyze).toBe('done');
    expect(s.status.personas).toBe('done');
    expect(s.status.copy).toBe('review');
    expect(s.status.images).toBe('review');
    expect(s.status.ab).toBe('review');
  });

  it('古くなった工程を再実行すると帯が消える', async () => {
    const t = setup();
    await importAndConfirm(t);
    await t.ctl.runAll();
    await t.ctl.runOne('analyze');
    await t.ctl.runOne('personas');
    const s = t.store.get();
    expect(s.status.personas).toBe('done');
    expect(s.staleSource.personas).toBeUndefined();
    // ③を実行し直したので ④⑤⑥⑧ の帯の出所は「③」に置き換わらず、最初に古くした「②」のまま
    expect(s.staleSource.copy).toBe('analyze');
  });

  it('職種名を編集すると (②が完了済みなら) ②以下が要再実行になる', async () => {
    const t = setup();
    await importAndConfirm(t);
    await t.ctl.runAll();
    t.ctl.setJobTitle('介護');
    const s = t.store.get();
    expect(s.status.analyze).toBe('stale');
    expect(s.staleSource.analyze).toBeUndefined();
    expect(s.status.personas).toBe('stale');
    expect(s.staleSource.personas).toBe('analyze');
    expect(s.status.extract).toBe('review');
    expect(s.jobTitleConfirmed).toBe(false);
  });

  it('前工程未実行のときは API を呼ばずに失敗にする', async () => {
    const t = setup();
    await importAndConfirm(t);
    t.calls.length = 0;
    await t.ctl.runOne('personas');
    expect(t.calls).toEqual([]);
    expect(t.store.get().status.personas).toBe('fail');
    expect(t.store.get().failures.personas).toBe('前工程（②市場分析）が未実行です。');
  });

  it('④で一部ペルソナが失敗すると失敗ラベル付きで止まる', async () => {
    const t = setup((path, _body, n) =>
      path === '/api/jobgen/copy' && n === 2
        ? { ok: false, error: new ApiDataError('LLM の返りに必要なキーがありません', {}) }
        : null,
    );
    await importAndConfirm(t);
    await t.ctl.runOne('extract');
    await t.ctl.runOne('analyze');
    await t.ctl.runOne('personas');
    await t.ctl.runOne('copy');
    const s = t.store.get();
    expect(s.status.copy).toBe('fail');
    expect(s.failures.copy).toBe('一部ペルソナのコピー生成に失敗: 介護未経験の異業種転職者');
    expect(s.copies[1]?.error).toBe('LLM の返りに必要なキーがありません');
    expect(s.copies[0]?.copies).toHaveLength(3);
  });

  it('⑤b (プロンプト化) だけ失敗しても ⑤ は完了し、エラー文を持つ', async () => {
    const t = setup((path) =>
      path === '/api/jobgen/image_prompts'
        ? { ok: false, error: new ApiDataError('directions(工程⑤の出力)が必要です', {}) }
        : null,
    );
    await importAndConfirm(t);
    await t.ctl.runAll();
    const s = t.store.get();
    expect(s.status.images).toBe('done');
    expect(s.images).toHaveLength(3);
    expect(s.imagePrompts).toEqual([]);
    expect(s.imagePromptsError).toBe('directions(工程⑤の出力)が必要です');
  });
});
