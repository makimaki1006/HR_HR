// 結果セクションの表示値を fixture の具体値で検証する (renderToStaticMarkup、node 環境)。
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { ApiResult } from '../../api/client';
import fixtures from '../../generated/jobgen/fixtures.json';
import type { JobgenPath, PostFn } from './api';
import { JobgenView } from './JobgenScreen';
import { buildMobileView } from './mobileView';
import { createPipelineController } from './pipeline';
import { AbSection } from './sections/AbSection';
import { AnalyzeSection } from './sections/AnalyzeSection';
import { CopySection } from './sections/CopySection';
import { ExtractSection } from './sections/ExtractSection';
import { HrhackerSection } from './sections/HrhackerSection';
import { ImagesSection } from './sections/ImagesSection';
import { MobileSection } from './sections/MobileSection';
import { PersonasSection } from './sections/PersonasSection';
import { initialState, type PipelineState } from './state';
import { createStore } from './store';

const R = fixtures.responses;
const noop = () => undefined;

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

/** fixture で①〜⑧まで走らせた後の状態 (store も返すので、その後の操作結果も読める)。 */
async function fullState() {
  const post: PostFn = (path) =>
    Promise.resolve({ ok: true, data: FIXTURE_BY_PATH[path] } as ApiResult<never>);
  const store = createStore<PipelineState>(initialState());
  const ctl = createPipelineController({ store, post });
  await ctl.normalize({ kind: 'free_text', text: fixtures.source_text });
  ctl.setJobTitle(fixtures.job_title);
  ctl.setJobTitleConfirmed(true);
  await ctl.runAll();
  return { state: store.get(), ctl, store };
}

function count(html: string, needle: string): number {
  return html.split(needle).length - 1;
}

describe('① ExtractSection', () => {
  it('引用照合の件数と各行の値・状態', () => {
    const html = renderToStaticMarkup(
      <ExtractSection facts={R.extract.facts} confirmed={false} onConfirm={noop} />,
    );
    expect(html).toContain('<span class="gate bad">引用照合：確認済み 6／要確認 1／未取得 1</span>');
    expect(html).toContain(
      '<td class="fkey">給与</td><td>月給192,000円〜195,000円</td><td class="fquote">「月給192,000円〜195,000円」</td><td><span class="fst verified">検証済</span></td>',
    );
    expect(html).toContain(
      '<tr class="row-rejected"><td class="fkey">保険</td><td>未取得</td><td class="fquote">「雇用保険、労災保険、健康保険、厚生年金、退職金制度」</td><td><span class="fst rejected">要確認</span></td>',
    );
    expect(html).toContain(
      '<tr class="row-missing"><td class="fkey">必須資格</td><td>未取得</td><td class="fquote">—</td><td><span class="fst missing">未取得</span></td>',
    );
    expect(count(html, '<tr')).toBe(9); // ヘッダ 1 + 8 項目
    expect(html).toContain('この工程の生成物を目視で確認しました（コンサル確認済みにする）');
    expect(html).not.toContain('confirmbox on');
  });
});

describe('② AnalyzeSection', () => {
  it('職種知識の注入表示と 3 カラム', () => {
    const html = renderToStaticMarkup(
      <AnalyzeSection
        analysis={R.analyze.analysis}
        category={R.analyze.category}
        knowledgeUsed={R.analyze.knowledge_used}
        confirmed={true}
        onConfirm={noop}
      />,
    );
    expect(html).toContain('<span class="gate ok">職種知識注入：介護職の知識を使用</span>');
    expect(html).not.toContain('knowledge-warn');
    expect(html).toContain('<h3>表面の強み</h3><ul><li>月給192,000円〜195,000円で資格手当・処遇改善手当を含む</li>');
    expect(html).toContain('<h3>ボトルネック</h3><ul><li>夜勤の有無・回数が原文で明確でない</li>');
    expect(html).toContain('class="confirmbox on"');
  });

  it('知識が無い職種は汎用注記を出す', () => {
    const html = renderToStaticMarkup(
      <AnalyzeSection
        analysis={R.analyze.analysis}
        category="その他"
        knowledgeUsed={false}
        confirmed={false}
        onConfirm={noop}
      />,
    );
    expect(html).toContain('<span class="gate warn">職種知識注入：その他（該当知識なし・汎用）</span>');
    expect(html).toContain('工程②だけ再実行');
  });
});

describe('③ PersonasSection', () => {
  it('3 枚のカードに仮説バッジと各項目', () => {
    const html = renderToStaticMarkup(
      <PersonasSection personas={R.personas.personas} confirmed={false} onConfirm={noop} />,
    );
    expect(count(html, 'class="pcard"')).toBe(3);
    expect(html).toContain('<div class="plabel">子育て中の復職希望者<span class="pbadge">仮説</span></div>');
    expect(html).toContain('<span class="pk">現職の不満</span>以前の職場は夜勤が月8回あり家庭と両立できなかった');
    expect(html).toContain('実在人物の情報として扱わないでください。');
  });
});

describe('④ CopySection', () => {
  it('ゲート集計と違反明細', async () => {
    const { state } = await fullState();
    const html = renderToStaticMarkup(
      <CopySection copies={state.copies} confirmed={false} onConfirm={noop} />,
    );
    // 3 ペルソナ × 同じ fixture 応答 = 違反 3 件ずつ
    expect(html).toContain('<span class="gate bad">法令NGワード：違反 3件</span>');
    expect(html).toContain('<span class="gate bad">数値照合：原文にない数値 3件</span>');
    expect(html).toContain('<span class="gate warn">表現レビュー：要確認 3件</span>');
    expect(html).toContain('<div class="chip"><span class="cstyle">常識破壊</span>夜勤なしでも、介護福祉士の資格はちゃんと評価される。</div>');
    expect(html).toContain('<li>女性歓迎（女性×歓迎）: 性別差別表現</li>');
    expect(html).toContain('<li>年間休日120日。前の職場より土日が増えたと先輩が言った。　→ 数値: <b>120 / 120日</b></li>');
    expect(html).toContain('<li>夜勤なし（清掃業務/クレーム対応/夜勤/残業/雑務/電話対応/オンコール対応×なし）（要確認）: 要出典確認表現(業務内容の断定)</li>');
    expect(count(html, '<span class="gstat review">要確認</span>')).toBe(3);
    expect(html).not.toContain('数値照合: 未実施');
  });
});

describe('⑤ ImagesSection', () => {
  it('ディレクションと生成プロンプト', () => {
    const html = renderToStaticMarkup(
      <ImagesSection
        images={R.images.directions}
        imagePrompts={R.image_prompts.prompts}
        imagePromptsError=""
        imagesNv={[]}
        imagesNumberCheck="checked"
        confirmed={false}
        onConfirm={noop}
      />,
    );
    expect(count(html, 'class="dircard"')).toBe(3);
    expect(html).toContain('<div class="dl">介護未経験の異業種転職者</div><div class="dt">先輩職員が新人に記録の書き方を教えている場面。二人の距離感を近く。</div>');
    expect(html).toContain('<div class="pappeal">🎯 この画像の狙い: 研修と同行で未経験から始められる</div>');
    expect(html).toContain('<div class="pbox">被写体: 先輩職員と新人職員の2人。場所: 記録用のデスク。構図: 肩越しに手元を見せる。</div>');
    expect(html).toContain('<div class="pmeta">推奨アスペクト比: <b>4:5</b></div>');
    expect(html).not.toContain('gate bad');
  });

  it('プロンプト化の失敗と変換中', () => {
    const failed = renderToStaticMarkup(
      <ImagesSection
        images={R.images.directions}
        imagePrompts={[]}
        imagePromptsError="HTTP 502"
        imagesNv={[]}
        imagesNumberCheck=""
        confirmed={false}
        onConfirm={noop}
      />,
    );
    expect(failed).toContain('プロンプト化に失敗: HTTP 502（工程⑤を再実行すると再試行します）');
    const pending = renderToStaticMarkup(
      <ImagesSection
        images={R.images.directions}
        imagePrompts={[]}
        imagePromptsError=""
        imagesNv={[]}
        imagesNumberCheck=""
        confirmed={false}
        onConfirm={noop}
      />,
    );
    expect(count(pending, '生成AI用プロンプトへ変換中…')).toBe(3);
  });
});

describe('⑥ MobileSection', () => {
  it('作成例カードは検証済みの事実だけを出し、意図ポップアップの元データを持つ', async () => {
    const { state } = await fullState();
    const view = buildMobileView(state, state.mobile);
    expect(view.cards).toHaveLength(3);
    const c0 = view.cards[0];
    expect(c0?.title).toBe('介護職');
    expect(c0?.badges.map((b) => [b.label, b.value])).toEqual([
      ['給与', '月給192,000円〜195,000円'],
      ['勤務地', '東京都八王子市高尾町1-2-3 ○○苑'],
      ['雇用形態', '正社員'],
    ]);
    expect(c0?.chips).toEqual([
      '月給192,000円〜195,000円で資格手当・処遇改善手当を含む',
      '年間休日110日・週休2日制',
      '高尾駅から徒歩10分',
    ]);
    expect(c0?.reqRows.map((r) => r.label)).toEqual(['給与', '勤務時間', '休日', '勤務地', '雇用形態', '手当']);
    expect(c0?.catchCopy?.text).toBe('夜勤なしでも、介護福祉士の資格はちゃんと評価される。');
    expect(c0?.photoDirection).toBe('午後の明るい談話室。利用者と笑顔で話す職員を斜め前から。制服は清潔なポロシャツ。');
    const photo = c0?.photoPopup !== null && c0?.photoPopup !== undefined ? view.popups[c0.photoPopup] : undefined;
    expect(photo?.head).toBe('写真の意図');
    expect(photo?.rows[0]).toEqual(['画像の狙い', '家庭と両立できる働き方']);
    expect(photo?.rows[1]).toEqual(['ターゲット', '子育て中の復職希望者']);
    expect(photo?.foot).toBe('出所: ⑤画像案・生成プロンプト／③ペルソナ設計');
    // 各カード: 写真 1 + コピー 1 + バッジ 3 + タグ 1 + 本文 1 + 要項 6 = 13
    expect(view.popups).toHaveLength(39);

    const html = renderToStaticMarkup(
      <MobileSection state={state} mobile={state.mobile} confirmed={false} onConfirm={noop} />,
    );
    expect(html).toContain('<span class="gate ok">文字数・法令NGワード：通過</span>');
    expect(html).toContain('<span class="gate ok">数値照合：通過</span>');
    expect(html).toContain('求人ページ風の作成例（イメージ）');
    expect(count(html, '<article class="jpost">')).toBe(3);
    expect(html).toContain('<h3 class="jpost-title">介護職</h3>');
    expect(html).toContain('<span class="jbadge pay" data-ipop="2" tabindex="0"><span class="jbk">給与</span>月給192,000円〜195,000円</span>');
    expect(html).toContain('<div class="jrow" data-ipop="7" tabindex="0"><div class="jdt">給与</div><div class="jdd">月給192,000円〜195,000円</div></div>');
    expect(html).not.toContain('<div class="jdt">保険</div>'); // rejected は出さない
    expect(html).not.toContain('<div class="jdt">必須資格</div>'); // missing は出さない
    expect(count(html, '<div class="ml-line">高尾駅から徒歩10分。</div>')).toBe(6); // カード本文 + プレーン原稿 × 3
    expect(count(html, '<div class="ml-empty"></div>')).toBe(6);
    expect(count(html, '<button class="japply" type="button" disabled="">応募する（イメージ）</button>')).toBe(3);
    expect(count(html, '※この作成例はあくまでイメージです。')).toBe(3);
    expect(html).toContain('<summary>原稿テキスト（プレーン）</summary>');
    expect(html).toContain('原稿テキストをコピー');
    expect(html).not.toContain('id="intentPop"');
  });

  it('失敗したペルソナはエラーだけを出す', () => {
    const s = initialState();
    const mobile = [
      {
        label: 'X',
        lines: [],
        ng_violations: [],
        expression_warnings: [],
        number_violations: [],
        number_check: '',
        review_required: false,
        error: 'HTTP 500',
      },
    ];
    const html = renderToStaticMarkup(
      <MobileSection state={s} mobile={mobile} confirmed={false} onConfirm={noop} />,
    );
    expect(html).toContain('<div class="moblabel">X</div><div class="err">HTTP 500</div>');
    expect(html).not.toContain('jpost');
    expect(html).not.toContain('ihint');
  });
});

describe('⑦ HrhackerSection', () => {
  it('転記充足・未照合数値・未転記ヒント・生成 5 列・84 列テーブル', async () => {
    const { state } = await fullState();
    if (!state.hrhacker) throw new Error('hrhacker 未設定');
    const html = renderToStaticMarkup(
      <HrhackerSection h={state.hrhacker} confirmed={false} onConfirm={noop} />,
    );
    expect(html).toContain('<span class="gate bad">数値照合：未照合 1件</span>');
    expect(html).toContain('<span class="gate warn">文字数・NGワード：レビュー要あり</span>');
    expect(html).toContain('<span class="gate warn">生成列：要レビュー 1列</span>');
    expect(html).toContain('<div class="fillstat">転記充足: <b>11/84</b> 列（原文に対応する列 <b>11/13</b>）</div>');
    expect(html).toContain('<div class="badnums"><b>元の資料で確認できない数値があります。</b> 該当する生成項目を確認してください。</div>');
    expect(html).toContain('📋 原文に記載があるのに未転記の可能性がある項目（2件）');
    expect(html).toContain('<td class="colcell">最寄り駅</td>');
    expect(html).toContain('<td class="colcell">試用・研修の有無</td>');
    expect(html).toContain('<div class="gcol">メリット <span class="gstat review">要確認</span></div><div class="gval empty">（空欄・レビュー行き）</div><div class="gissues">課題:<ul><li>原文で確認できない数値があります。元の資料を確認してください。</li></ul></div>');
    expect(html).toContain('<div class="gcol">求人の見出し <span class="gstat verified">検証済</span></div><div class="gval">介護職員（特養）／高尾駅徒歩10分・年間休日110日</div>');
    expect(count(html, 'class="gencard ')).toBe(5);
    expect(html).toContain('出力内容の確認（84項目）');
    expect(count(html, 'class="row-gen"')).toBe(5);
    expect(html).toContain('<tr class=""><td class="colcell">求人の管理番号</td><td><div class="valwrap">未取得</div></td><td class="colcell">未取得</td></tr>');
    expect(html).toContain('<td class="colcell">雇用形態</td><td><div class="valwrap">正社員</div></td>');
    expect(html).toContain('<td class="colcell">休日・休暇</td><td><div class="valwrap">週休2日制（シフト制）、年間休日110日</div></td>');
    expect(html).toContain('id="csvBtn"');
  });
});

describe('⑧ AbSection', () => {
  it('助言 3 行と表現レビュー', async () => {
    const { state } = await fullState();
    if (!state.ab) throw new Error('ab 未設定');
    const html = renderToStaticMarkup(<AbSection ab={state.ab} confirmed={false} onConfirm={noop} />);
    expect(html).toContain('<span class="gate ok">法令NGワード：通過</span>');
    expect(html).toContain('<span class="gate ok">数値照合：通過</span>');
    expect(html).toContain('<span class="gate warn">表現レビュー：要確認 1件</span>');
    expect(count(html, 'class="abrow"')).toBe(3);
    expect(html).toContain('<div class="abm">CTR（クリック率）</div><div>キャッチコピーA（夜勤なし訴求）とB（資格評価訴求）を同一画像で並走し、CTRが低い方を差し替える</div>');
  });
});

describe('画面全体 (JobgenView)', () => {
  it('初期表示: 8 工程が待機、一括実行は無効、結果セクションは非表示', () => {
    const store = createStore<PipelineState>(initialState());
    const ctl = createPipelineController({ store, post: () => Promise.reject(new Error('unused')) });
    const html = renderToStaticMarkup(<JobgenView s={store.get()} ctl={ctl} />);
    expect(html).toContain('<h1>求人票作成</h1>');
    expect(count(html, '<span class="st wait">待機</span>')).toBe(8);
    expect(html).toContain('<button type="button" class="btn" id="runAllBtn" disabled="">一括実行</button>');
    expect(html).toContain('先に求人原文を取り込むと実行できます。');
    expect(count(html, '<section class="res" id="res-')).toBe(8);
    expect(count(html, 'hidden=""')).toBe(8);
    expect(html).toContain('<div class="sgate">検証: 数値照合・文字数・NGワード</div>');
    expect(html).toContain('<option value="5" selected="">5案</option>');
  });

  it('一括実行後: 工程ピルの文言、帯なし、8 セクション表示', async () => {
    const { state, ctl } = await fullState();
    const html = renderToStaticMarkup(<JobgenView s={state} ctl={ctl} />);
    expect(count(html, '<span class="st review">警告あり（要確認）</span>')).toBe(5); // ①④⑤⑦⑧
    expect(count(html, '<span class="st done">生成完了（自動検証なし）</span>')).toBe(2); // ②③
    expect(count(html, '<span class="st done">生成完了・自動検証済み</span>')).toBe(1); // ⑥
    expect(count(html, 'hidden=""')).toBe(0);
    expect(html).not.toContain('stalebar');
    expect(html).toContain('取り込み済み: <b>【職種】介護職員（特別養護老人ホーム）</b>（原文 ');
    expect(html).toContain('id="jobConfirm"');
    expect(html).toContain('class="jobconfirm on"');
  });

  it('②再実行後: ③〜⑧に要再実行の帯とピル、コンサル確認は外れる', async () => {
    const { ctl, store } = await fullState();
    ctl.toggleConfirm('extract', true);
    await ctl.runOne('analyze');
    const html = renderToStaticMarkup(<JobgenView s={store.get()} ctl={ctl} />);
    expect(count(html, '<span class="st stale">要再実行（前工程が更新済み）</span>')).toBe(6);
    expect(count(html, '<div class="stalebar">⚠ ② 市場分析 を実行し直したため、この結果は古い可能性があります。必要なら再実行してください。</div>')).toBe(6);
    expect(html).toContain('<span class="st confirmed">コンサル確認済み</span>');
    expect(count(html, 'class="confirmbox on"')).toBe(1);
  });

  it('失敗した工程のセクションは失敗の見出しとエラーだけ', () => {
    const s: PipelineState = {
      ...initialState(),
      sourceText: 'x',
      status: { ...initialState().status, extract: 'fail' },
      failures: { extract: 'GEMINI_API_KEY が未設定です' },
    };
    const store = createStore<PipelineState>(s);
    const ctl = createPipelineController({ store, post: () => Promise.reject(new Error('unused')) });
    const html = renderToStaticMarkup(<JobgenView s={s} ctl={ctl} />);
    expect(html).toContain('<h2>① 事実抽出</h2><div class="gates"><span class="gate bad">失敗</span></div><div class="err">GEMINI_API_KEY が未設定です</div>');
    expect(html).toContain('<span class="st fail">失敗</span>');
    expect(count(html, 'hidden=""')).toBe(7);
  });
});
