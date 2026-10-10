// パイプラインの実行制御 (旧 static/jobgen.html の runExtract〜runAb / runOne / 一括実行 /
// 正規化 / pickJob を移植)。DOM は触らず、状態はストア経由。送信関数 `post` は差し替え可能
// (Vitest はモックを渡し、送った body と状態遷移を検証する)。
//
// 旧と同じ順で同じ API を同じ body で呼ぶこと (旧 JS のキーと突き合わせ済み):
//   extract       {source_text}
//   analyze       {source_text, job_title}
//   personas      {source_text, analysis, count}
//   copy (×人数)  {persona, analysis, source_text}
//   images        {personas, source_text}
//   image_prompts {directions:{directions}, personas, source_text}
//   mobile (×人数){persona, facts_text, source_text}
//   hrhacker      {source_text, facts, strategy_hint}
//   ab            {summary, source_text}
import type { Analysis } from '../../generated/Analysis';
import type { NormalizeRequest } from '../../generated/NormalizeRequest';
import type { Persona } from '../../generated/Persona';
import type { PostFn } from './api';
import { saveSourceHandoff } from './handoff';
import {
  abSummary,
  CTL_HINT_READY,
  type InputKind,
  markStaleAfter,
  type PipelineState,
  resetResults,
  type StepKey,
  type StepStatus,
  STEPS,
  stepDef,
  strategyHint,
} from './state';
import type { Store } from './store';

export interface PipelineController {
  setKind: (kind: InputKind) => void;
  setPersonaCount: (n: number) => void;
  setJobTitle: (v: string) => void;
  setJobTitleConfirmed: (b: boolean) => void;
  toggleConfirm: (key: StepKey, checked: boolean) => void;
  /** 取り込み。body は画面側で組む (ファイル読込は DOM 側)。 */
  normalize: (body: NormalizeRequest) => Promise<void>;
  pickJob: (index: number) => void;
  runOne: (key: StepKey) => Promise<void>;
  runAll: () => Promise<void>;
  errStatus: (message: string) => void;
}

type Runner = () => Promise<StepStatus>;

/** ラベルが空なら「ペルソナN」(旧: p.label||("ペルソナ"+(i+1)))。 */
function personaLabel(p: Persona | undefined, i: number): string {
  return (p?.label ?? '') || 'ペルソナ' + String(i + 1);
}

/** 工程キーを 1 つ除いた複製 (delete を使わない)。 */
function omitStep<T>(obj: Partial<Record<StepKey, T>>, key: StepKey): Partial<Record<StepKey, T>> {
  const out: Partial<Record<StepKey, T>> = {};
  for (const [k, v] of Object.entries(obj) as [StepKey, T][]) {
    if (k !== key) out[k] = v;
  }
  return out;
}

const EMPTY_ANALYSIS: Analysis = { surface_strengths: [], hidden_strengths: [], bottlenecks: [] };

export interface ControllerDeps {
  store: Store<PipelineState>;
  post: PostFn;
  /** 引き継ぎ保存の時刻 (テストで固定できるように注入)。 */
  now?: () => Date;
}

export function createPipelineController({ store, post, now }: ControllerDeps): PipelineController {
  const get = store.get;
  const set = store.set;
  const clock = now ?? (() => new Date());

  const errStatus = (message: string): void => {
    set({ statusMessage: { kind: 'err', text: message } });
  };

  // ── 各工程の実行 (旧 runExtract 等)。失敗は throw、成功は 'done' | 'review' ──
  const runExtract: Runner = async () => {
    const s = get();
    if (!s.sourceText) throw new Error('求人原文が未取り込みです。');
    const r = await post('/api/jobgen/extract', { source_text: s.sourceText });
    if (!r.ok) throw new Error(r.error.message);
    const facts = r.data.facts;
    set((s2) => ({ ...s2, facts, factsText: r.data.facts_text || '', resultReady: { ...s2.resultReady, extract: true } }));
    return Object.values(facts).some((f) => f.status === 'rejected') ? 'review' : 'done';
  };

  const runAnalyze: Runner = async () => {
    const s = get();
    if (!s.sourceText) throw new Error('求人原文が未取り込みです。');
    // 職種名はユーザーが編集できる職種名欄を正とする (先頭行が会社名のケース対策)。
    const jobTitle = s.jobTitle.trim() || s.titleHint || '';
    const r = await post('/api/jobgen/analyze', { source_text: s.sourceText, job_title: jobTitle });
    if (!r.ok) throw new Error(r.error.message);
    set((s2) => ({
      ...s2,
      category: r.data.category || '',
      analysis: r.data.analysis,
      knowledgeUsed: r.data.knowledge_used,
      resultReady: { ...s2.resultReady, analyze: true },
    }));
    return 'done';
  };

  const runPersonas: Runner = async () => {
    const s = get();
    if (!s.analysis) throw new Error('前工程（②市場分析）が未実行です。');
    const count = s.personaCount || 5;
    const r = await post('/api/jobgen/personas', {
      source_text: s.sourceText,
      analysis: s.analysis,
      count,
    });
    if (!r.ok) throw new Error(r.error.message);
    const personas = r.data.personas ?? [];
    set((s2) => ({ ...s2, personas, resultReady: { ...s2.resultReady, personas: true } }));
    return personas.length ? 'done' : 'review';
  };

  const runCopy: Runner = async () => {
    const s = get();
    if (!s.personas.length) throw new Error('前工程（③ペルソナ設計）が未実行です。');
    const analysis = s.analysis ?? EMPTY_ANALYSIS;
    const results = await Promise.all(
      s.personas.map((p) =>
        post('/api/jobgen/copy', { persona: p, analysis, source_text: s.sourceText }),
      ),
    );
    const copies = results.map((r, i) => {
      const label = personaLabel(s.personas[i], i);
      if (r.ok) {
        return {
          label,
          copies: r.data.copies ?? [],
          ng_violations: r.data.ng_violations,
          expression_warnings: r.data.expression_warnings,
          number_violations: r.data.number_violations,
          number_check: r.data.number_check || '',
          review_required: r.data.review_required,
        };
      }
      return {
        label,
        copies: [],
        ng_violations: [],
        expression_warnings: [],
        number_violations: [],
        number_check: '',
        review_required: false,
        error: r.error.message,
      };
    });
    set((s2) => ({ ...s2, copies, resultReady: { ...s2.resultReady, copy: true } }));
    const failed = copies.filter((c) => c.error !== undefined);
    if (failed.length) {
      throw new Error(
        '一部ペルソナのコピー生成に失敗: ' + failed.map((c) => c.label).join('、'),
      );
    }
    return copies.some(
      (c) =>
        c.review_required ||
        c.ng_violations.length ||
        c.expression_warnings.length ||
        c.number_violations.length,
    )
      ? 'review'
      : 'done';
  };

  const runImages: Runner = async () => {
    const s = get();
    if (!s.personas.length) throw new Error('前工程（③ペルソナ設計）が未実行です。');
    const r = await post('/api/jobgen/images', { personas: s.personas, source_text: s.sourceText });
    if (!r.ok) throw new Error(r.error.message);
    const images = r.data.directions ?? [];
    // まずディレクションを出す (プロンプト化の待ち時間中も見られるように)。
    set((s2) => ({
      ...s2,
      images,
      imagesNv: [...r.data.number_violations],
      imagesNumberCheck: r.data.number_check || '',
      imagePrompts: [],
      imagePromptsError: '',
      resultReady: { ...s2.resultReady, images: true },
    }));
    // ⑤b: 生成AIへ丸投げできる日本語プロンプトに変換 (全ペルソナ一括で +1 コール)。
    if (images.length) {
      const p = await post('/api/jobgen/image_prompts', {
        directions: { directions: images },
        personas: s.personas,
        source_text: s.sourceText,
      });
      if (p.ok) {
        const cur = get();
        set({
          imagePrompts: p.data.prompts ?? [],
          imagesNv: p.data.number_violations.length
            ? cur.imagesNv.concat(p.data.number_violations)
            : cur.imagesNv,
          imagesNumberCheck: p.data.number_check || cur.imagesNumberCheck,
        });
      } else {
        set({ imagePrompts: [], imagePromptsError: p.error.message });
      }
    }
    return get().imagesNv.length ? 'review' : 'done';
  };

  const runMobile: Runner = async () => {
    const s = get();
    if (!s.personas.length) throw new Error('前工程（③ペルソナ設計）が未実行です。');
    const results = await Promise.all(
      s.personas.map((p) =>
        post('/api/jobgen/mobile', {
          persona: p,
          facts_text: s.factsText,
          source_text: s.sourceText,
        }),
      ),
    );
    const mobile = results.map((r, i) => {
      const label = personaLabel(s.personas[i], i);
      if (r.ok) {
        return {
          label,
          lines: r.data.lines,
          ng_violations: r.data.ng_violations,
          expression_warnings: r.data.expression_warnings,
          number_violations: r.data.number_violations,
          number_check: r.data.number_check || '',
          review_required: r.data.review_required,
        };
      }
      return {
        label,
        lines: [],
        ng_violations: [],
        expression_warnings: [],
        number_violations: [],
        number_check: '',
        review_required: false,
        error: r.error.message,
      };
    });
    set((s2) => ({ ...s2, mobile, resultReady: { ...s2.resultReady, mobile: true } }));
    const failed = mobile.filter((m) => m.error !== undefined);
    if (failed.length) {
      throw new Error(
        '一部ペルソナのスマホ原稿生成に失敗: ' + failed.map((m) => m.label).join('、'),
      );
    }
    return mobile.some(
      (m) =>
        m.review_required ||
        m.ng_violations.length ||
        m.expression_warnings.length ||
        m.number_violations.length,
    )
      ? 'review'
      : 'done';
  };

  const runHrhacker: Runner = async () => {
    const s = get();
    if (!s.facts) throw new Error('前工程（①事実抽出）が未実行です。');
    const r = await post('/api/jobgen/hrhacker', {
      source_text: s.sourceText,
      facts: s.facts,
      strategy_hint: strategyHint(s),
    });
    if (!r.ok) throw new Error(r.error.message);
    const h = {
      row: r.data.row,
      generated_fields: r.data.generated_fields,
      review_required_fields: r.data.review_required_fields,
      unsupported_numbers: r.data.unsupported_numbers,
      fill_stats: r.data.fill_stats,
      unassigned_hints: r.data.unassigned_hints,
    };
    set((s2) => ({ ...s2, hrhacker: h, resultReady: { ...s2.resultReady, hrhacker: true } }));
    return h.review_required_fields.length ||
      h.unsupported_numbers.length ||
      Object.values(h.generated_fields).some((g) => g.status === 'review_required')
      ? 'review'
      : 'done';
  };

  const runAb: Runner = async () => {
    const s = get();
    const r = await post('/api/jobgen/ab', { summary: abSummary(s), source_text: s.sourceText });
    if (!r.ok) throw new Error(r.error.message);
    const ab = {
      steps: r.data.steps ?? [],
      ng_violations: r.data.ng_violations,
      expression_warnings: r.data.expression_warnings,
      number_violations: r.data.number_violations,
      number_check: r.data.number_check || '',
    };
    set((s2) => ({ ...s2, ab, resultReady: { ...s2.resultReady, ab: true } }));
    return ab.ng_violations.length || ab.expression_warnings.length || ab.number_violations.length
      ? 'review'
      : 'done';
  };

  const RUNNERS: Record<StepKey, Runner> = {
    extract: runExtract,
    analyze: runAnalyze,
    personas: runPersonas,
    copy: runCopy,
    images: runImages,
    mobile: runMobile,
    hrhacker: runHrhacker,
    ab: runAb,
  };

  /** 工程を「実行中」にする共通処理 (確認済みフラグと前回の失敗・古さの帯を外す)。 */
  const beginStep = (s: PipelineState, key: StepKey): PipelineState => {
    const failures = omitStep(s.failures, key);
    const staleSource = omitStep(s.staleSource, key);
    const { num, name } = stepDef(key);
    return {
      ...s,
      running: true,
      curStep: key,
      confirmed: { ...s.confirmed, [key]: false },
      status: { ...s.status, [key]: 'run' },
      failures,
      staleSource,
      statusMessage: { kind: 'loading', text: `${num} ${name} を実行中…` },
    };
  };

  const runOne = async (key: StepKey): Promise<void> => {
    if (get().running || get().normalizing) return;
    set((s) => beginStep(s, key));
    try {
      const st = await RUNNERS[key]();
      // 単独再実行では後続工程の結果が古いまま残るため、依存する工程に印を付ける。
      set((s) =>
        markStaleAfter({ ...s, status: { ...s.status, [key]: st }, statusMessage: null }, key),
      );
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      set((s) => ({
        ...s,
        status: { ...s.status, [key]: 'fail' },
        failures: { ...s.failures, [key]: message },
        statusMessage: null,
      }));
    } finally {
      set({ running: false, curStep: null });
    }
  };

  /** 一括実行 (①→⑧を順に。前工程結果をメモリ経由で次へ)。失敗した工程で停止。 */
  const runAll = async (): Promise<void> => {
    const s0 = get();
    if (s0.running || s0.normalizing || !s0.sourceText || !s0.jobTitleConfirmed) return;
    for (const step of STEPS) {
      set((s) => beginStep(s, step.key));
      try {
        const st = await RUNNERS[step.key]();
        set((s) => ({ ...s, status: { ...s.status, [step.key]: st } }));
      } catch (e) {
        const message = e instanceof Error ? e.message : String(e);
        set((s) => ({
          ...s,
          status: { ...s.status, [step.key]: 'fail' },
          failures: { ...s.failures, [step.key]: message },
          statusMessage: {
            kind: 'err',
            text: `${step.num} ${step.name} で停止しました。修正後、その工程だけ再実行できます。`,
          },
          running: false,
          curStep: null,
        }));
        return;
      }
    }
    set({ running: false, curStep: null, statusMessage: null });
  };

  const pickJob = (index: number): void => {
    const s = get();
    if (s.running || s.normalizing) return;
    if (s.selectedJobIndex === index) return;
    const j = s.jobs[index];
    if (!j) return;
    const titleHint = j.title_hint || '';
    const sourceText = j.source_text || '';
    saveSourceHandoff(sourceText, titleHint, clock());
    set((prev) => ({
      ...resetResults({ ...prev, titleHint, sourceText }),
      selectedJobIndex: index,
      jobTitle: titleHint,
      ctlHint: CTL_HINT_READY,
    }));
  };

  const normalize = async (body: NormalizeRequest): Promise<void> => {
    if (get().running || get().normalizing) return;
    set({ normalizing: true, statusMessage: { kind: 'loading', text: '求人の内容を取り込み中…' } });
    try {
      const r = await post('/api/jobgen/normalize', body);
      if (!r.ok) {
        errStatus('取り込めませんでした: ' + r.error.message);
        return;
      }
      const jobs = r.data.jobs.filter((j) => (j.source_text || '').trim());
      if (!jobs.length) {
        errStatus('求人原文が取得できませんでした。');
        return;
      }
      set((s) => ({ ...resetResults(s), jobs, titleHint: '', sourceText: '', jobTitle: '', normalizing: false, statusMessage: null }));
      if (jobs.length === 1) pickJob(0);
    } catch {
      errStatus('取り込みを完了できませんでした。通信状態を確認し、もう一度取り込んでください。');
    } finally {
      set({ normalizing: false });
    }
  };

  const setJobTitle = (v: string): void => {
    set((s) => {
      let next: PipelineState = { ...s, jobTitle: v };
      // 職種名を編集したら確認をやり直させる (誤った職種名のまま一括実行するのを防ぐ)。
      if (next.jobTitleConfirmed) next = { ...next, jobTitleConfirmed: false };
      // 職種名は②市場分析の入力なので、分析済みなら②以下も「要再実行」にする。
      if (next.analysis && next.status.analyze === 'done') {
        next = markStaleAfter(
          {
            ...next,
            status: { ...next.status, analyze: 'stale' },
            confirmed: { ...next.confirmed, analyze: false },
          },
          'analyze',
        );
      }
      return next;
    });
  };

  return {
    setKind: (kind) => {
      set({ kind });
    },
    setPersonaCount: (n) => {
      set({ personaCount: n });
    },
    setJobTitle,
    setJobTitleConfirmed: (b) => {
      set({ jobTitleConfirmed: b });
    },
    toggleConfirm: (key, checked) => {
      set((s) => ({ ...s, confirmed: { ...s.confirmed, [key]: checked } }));
    },
    normalize,
    pickJob,
    runOne,
    runAll,
    errStatus,
  };
}
