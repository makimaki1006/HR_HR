// 求人票生成パイプライン画面 (/app/jobgen)。旧 static/jobgen.html の React 移植。
// Shell (ヘッダー・ナビ) には依存しない。要素 id は旧画面と同じにしてある
// (旧新比較の Playwright と、外部の自動操作が同じセレクタで動くように)。
import { type ChangeEvent, type ReactNode, useMemo, useSyncExternalStore } from 'react';
import type { NormalizeRequest } from '../../generated/NormalizeRequest';
import { postJobgen, type PostFn } from './api';
import './jobgen.css';
import { createPipelineController, type PipelineController } from './pipeline';
import { FailBody, StaleBar } from './parts';
import { AbSection } from './sections/AbSection';
import { AnalyzeSection } from './sections/AnalyzeSection';
import { CopySection } from './sections/CopySection';
import { ExtractSection } from './sections/ExtractSection';
import { HrhackerSection } from './sections/HrhackerSection';
import { ImagesSection } from './sections/ImagesSection';
import { MobileSection } from './sections/MobileSection';
import { PersonasSection } from './sections/PersonasSection';
import {
  canRunAll,
  initialState,
  type InputKind,
  type PipelineState,
  type StepKey,
  STEPS,
  stepView,
} from './state';
import { createStore, type Store } from './store';

const KINDS: { kind: InputKind; label: string }[] = [
  { kind: 'free_text', label: '自由テキスト' },
  { kind: 'url', label: 'URL' },
  { kind: 'csv', label: 'CSV' },
  { kind: 'excel', label: 'Excel' },
  { kind: 'pdf', label: 'PDF' },
  { kind: 'html', label: 'HTML' },
];

const FILE_ACCEPT: Partial<Record<InputKind, string>> = {
  csv: '.csv,text/csv',
  excel: '.xlsx,.xls',
  pdf: '.pdf,application/pdf',
  html: '.html,.htm,text/html',
};
const FILE_HINT: Partial<Record<InputKind, string>> = {
  csv: 'CSVファイル（1行1求人）。または下のテキスト欄に貼り付け。',
  excel: 'Excelファイル（.xlsx / .xls・1行1求人）。base64でサーバへ送ります。',
  pdf: 'PDFの求人票。base64でサーバへ送りテキスト抽出します。',
  html: '求人ページのHTML。または下のテキスト欄に貼り付け。',
};

function isFileKind(k: InputKind): boolean {
  return k === 'csv' || k === 'excel' || k === 'pdf' || k === 'html';
}

// ── ファイル読み込みヘルパ (旧 readText / readBase64) ──
function readText(file: File): Promise<string> {
  return new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => {
      res(typeof r.result === 'string' ? r.result : '');
    };
    r.onerror = () => {
      rej(new Error('ファイル読込に失敗'));
    };
    r.readAsText(file);
  });
}

function readBase64(file: File): Promise<string> {
  return new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => {
      const buf = r.result;
      if (!(buf instanceof ArrayBuffer)) {
        res('');
        return;
      }
      let bin = '';
      const bytes = new Uint8Array(buf);
      const chunk = 0x8000;
      for (let i = 0; i < bytes.length; i += chunk) {
        bin += String.fromCharCode(...bytes.subarray(i, i + chunk));
      }
      res(btoa(bin));
    };
    r.onerror = () => {
      rej(new Error('ファイル読込に失敗'));
    };
    r.readAsArrayBuffer(file);
  });
}

export interface NormalizeInputs {
  kind: InputKind;
  freeText: string;
  url: string;
  file: File | null;
  pasteText: string;
}

/**
 * 取り込みボタンの入力検査と body 組み立て (旧 normBtn.onclick の前半)。
 * 失敗はメッセージを返す (旧と同じ文言)。
 */
export async function buildNormalizeRequest(
  inp: NormalizeInputs,
): Promise<{ ok: true; body: NormalizeRequest } | { ok: false; message: string }> {
  const k = inp.kind;
  if (k === 'free_text') {
    const t = inp.freeText.trim();
    if (!t) return { ok: false, message: '求人原文を入力してください。' };
    return { ok: true, body: { kind: k, text: t } };
  }
  if (k === 'url') {
    const u = inp.url.trim();
    if (!u) return { ok: false, message: 'URLを入力してください。' };
    return { ok: true, body: { kind: k, url: u } };
  }
  if (k === 'csv' || k === 'html') {
    if (inp.file) return { ok: true, body: { kind: k, text: await readText(inp.file) } };
    const t = inp.pasteText.trim();
    if (!t) return { ok: false, message: 'ファイルを選ぶかテキストを貼り付けてください。' };
    return { ok: true, body: { kind: k, text: t } };
  }
  if (!inp.file) return { ok: false, message: 'ファイルを選択してください。' };
  return { ok: true, body: { kind: k, data_base64: await readBase64(inp.file) } };
}

function toggleTheme(): void {
  const root = document.documentElement;
  const cur =
    root.getAttribute('data-theme') ??
    (matchMedia('(prefers-color-scheme:dark)').matches ? 'dark' : 'light');
  root.setAttribute('data-theme', cur === 'dark' ? 'light' : 'dark');
}

/** 入力パネル (自由テキスト / URL / ファイル)。入力欄の値は DOM が持つ (旧と同じ非制御)。 */
function InputPanel({ s, ctl }: { s: PipelineState; ctl: PipelineController }) {
  const k = s.kind;
  const isFile = isFileKind(k);
  const onNormalize = (): void => {
    const q = (id: string): HTMLInputElement | HTMLTextAreaElement | null =>
      document.getElementById(id) as HTMLInputElement | HTMLTextAreaElement | null;
    const fileEl = document.getElementById('fileInput') as HTMLInputElement | null;
    const inputs: NormalizeInputs = {
      kind: k,
      freeText: q('freeText')?.value ?? '',
      url: q('urlInput')?.value ?? '',
      file: fileEl?.files?.[0] ?? null,
      pasteText: q('pasteArea')?.value ?? '',
    };
    void (async () => {
      let built: Awaited<ReturnType<typeof buildNormalizeRequest>>;
      try {
        built = await buildNormalizeRequest(inputs);
      } catch (e) {
        ctl.errStatus(e instanceof Error ? e.message : String(e));
        return;
      }
      if (!built.ok) {
        ctl.errStatus(built.message);
        return;
      }
      await ctl.normalize(built.body);
    })();
  };
  return (
    <div className="panel">
      <h2>求人原文を取り込む</h2>
      <div className="tabs" id="tabs">
        {KINDS.map((t) => (
          <button
            key={t.kind}
            type="button"
            className={`tab${k === t.kind ? ' on' : ''}`}
            data-kind={t.kind}
            onClick={() => {
              const fileEl = document.getElementById('fileInput') as HTMLInputElement | null;
              if (fileEl) fileEl.value = '';
              ctl.setKind(t.kind);
            }}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className={`in-block${k === 'free_text' ? '' : ' hide'}`} id="in-text">
        <label htmlFor="freeText">求人原文（貼り付け）</label>
        <textarea
          id="freeText"
          placeholder={
            '求人票の本文をそのまま貼り付けてください。\n例: 職種／給与／勤務時間／休日／勤務地／雇用形態／保険／手当／必須資格 など'
          }
        />
      </div>
      <div className={`in-block${k === 'url' ? '' : ' hide'}`} id="in-url">
        <label htmlFor="urlInput">他媒体の掲載ページURL</label>
        <input type="url" id="urlInput" placeholder="https://…" />
        <div className="hint">サーバ側でHTTP取得し本文を抽出します。</div>
      </div>
      <div className={`in-block${isFile ? '' : ' hide'}`} id="in-file">
        <label id="fileLabel" htmlFor="fileInput">
          ファイルを選択
        </label>
        <input type="file" id="fileInput" accept={FILE_ACCEPT[k] ?? ''} />
        <div className="hint" id="fileHint">
          {FILE_HINT[k] ?? ''}
        </div>
        {/* テキスト貼付は CSV/HTML のみ許可 (Excel/PDF はバイナリ) */}
        <div
          id="pasteWrap"
          style={{ marginTop: '10px', display: k === 'csv' || k === 'html' ? undefined : 'none' }}
        >
          <label htmlFor="pasteArea">またはテキストを貼り付け</label>
          <textarea id="pasteArea" placeholder="CSV／HTMLのテキストを直接貼り付けても構いません。" />
        </div>
      </div>
      <div style={{ marginTop: '8px' }}>
        <button type="button" className="btn" id="normBtn" disabled={s.normalizing} onClick={onNormalize}>
          正規化して取り込む
        </button>
      </div>
      <div className="note">
        CSV／Excelは1行1求人として複数の求人に分かれることがあります。取り込み後に対象求人を選んでください。
      </div>
      <div id="jobPick">
        {s.jobs.length > 1 ? (
          <>
            <div className="joblist">
              {s.jobs.map((j, i) => {
                const on = s.sourceText === j.source_text && s.titleHint === (j.title_hint || '');
                return (
                  <div
                    key={i}
                    className={`jobitem${on ? ' on' : ''}`}
                    data-i={i}
                    onClick={() => {
                      ctl.pickJob(i);
                    }}
                  >
                    <span className="jt">{j.title_hint || '求人 ' + String(i + 1)}</span>
                    <span className="jp">{(j.source_text || '').slice(0, 90)}</span>
                  </div>
                );
              })}
            </div>
            <div className="note">
              複数の求人が見つかりました。1件を選ぶとパイプラインを実行できます。
            </div>
          </>
        ) : null}
      </div>
      <div id="pickedInfo">
        {s.sourceText ? (
          <>
            <div className="picked">
              取り込み済み: <b>{s.titleHint || '（先頭行を職種名候補にしています）'}</b>（原文{' '}
              {s.sourceText.length.toLocaleString('ja-JP')}字）
            </div>
            <div className="in-block" style={{ marginTop: '12px' }}>
              <label htmlFor="jobTitle">職種名（②市場分析で使用）</label>
              <input
                type="text"
                id="jobTitle"
                placeholder="例: 介護職 / 保育士 / 営業"
                value={s.jobTitle}
                onChange={(e: ChangeEvent<HTMLInputElement>) => {
                  ctl.setJobTitle(e.currentTarget.value);
                }}
              />
              <div className="hint">
                職種名で該当職種の知識を引きます（例:
                介護職、保育士、営業）。正しい職種名に直すと分析の質が上がります。
              </div>
            </div>
            <label className={`jobconfirm${s.jobTitleConfirmed ? ' on' : ''}`} id="jobConfirm">
              <input
                type="checkbox"
                id="jobConfirmChk"
                checked={s.jobTitleConfirmed}
                onChange={(e) => {
                  ctl.setJobTitleConfirmed(e.currentTarget.checked);
                }}
              />
              <span>
                この職種名で<b>市場分析（工程②）</b>を行います。職種名が正しいことを確認しました。
                <br />
                （確認するまで<b>一括実行</b>は押せません。工程を1つずつ実行する場合は不要です。）
              </span>
            </label>
          </>
        ) : null}
      </div>
    </div>
  );
}

function StepsPanel({ s, ctl }: { s: PipelineState; ctl: PipelineController }) {
  const ready = !!s.sourceText;
  return (
    <div className="panel">
      <h2>
        生成パイプライン <span className="tag">工程①〜⑧</span>
      </h2>
      <div className="ctl">
        <button
          type="button"
          className="btn"
          id="runAllBtn"
          disabled={!canRunAll(s)}
          onClick={() => {
            void ctl.runAll();
          }}
        >
          一括実行
        </button>
        <label className="cnt">
          ペルソナ人数
          <select
            id="personaCount"
            value={String(s.personaCount)}
            disabled={s.running}
            onChange={(e) => {
              ctl.setPersonaCount(parseInt(e.currentTarget.value, 10) || 5);
            }}
          >
            <option value="3">3案</option>
            <option value="4">4案</option>
            <option value="5">5案</option>
          </select>
        </label>
        <span className="hint" id="ctlHint">
          {s.ctlHint}
        </span>
      </div>
      <div className="steps" id="steps">
        {STEPS.map((st) => {
          const v = stepView(st.key, s.status[st.key], s.confirmed[st.key]);
          return (
            <div key={st.key} className={`step${s.curStep === st.key ? ' cur' : ''}`}>
              <div className="num">{st.num}</div>
              <div className="sinfo">
                <div className="sname">{st.name}</div>
                <div className="sgate">検証: {st.gate}</div>
              </div>
              <span className={`st ${v.cls}`}>{v.label}</span>
              <button
                type="button"
                className="btn2 btn-sm"
                data-rerun={st.key}
                disabled={!ready || s.running}
                onClick={() => {
                  void ctl.runOne(st.key);
                }}
              >
                再実行
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ResultSection({
  s,
  stepKey,
  children,
}: {
  s: PipelineState;
  stepKey: StepKey;
  children: ReactNode;
}) {
  const failure = s.failures[stepKey];
  const hidden = failure === undefined && children === null;
  const staleSrc = s.status[stepKey] === 'stale' ? s.staleSource[stepKey] : undefined;
  return (
    <section className="res" id={`res-${stepKey}`} hidden={hidden}>
      {!hidden && staleSrc !== undefined ? <StaleBar source={staleSrc} /> : null}
      {failure !== undefined ? <FailBody stepKey={stepKey} message={failure} /> : children}
    </section>
  );
}

export function JobgenView({ s, ctl }: { s: PipelineState; ctl: PipelineController }) {
  const onConfirm = ctl.toggleConfirm;
  const msg = s.statusMessage;
  return (
    <div className="jobgen">
      <button
        type="button"
        className="theme"
        id="themeBtn"
        title="テーマ切替"
        onClick={() => {
          toggleTheme();
        }}
      >
        ◐
      </button>
      <div className="wrap">
        <header>
          <div className="eyebrow">求人媒体選定エンジン · 生成パイプライン</div>
          <h1>求人票生成パイプライン</h1>
          <p className="sub">
            顧客の求人原文（自由テキスト／URL／CSV／Excel／PDF／HTML）から、工程①〜⑧を順に走らせて
            <b>戦略提案</b>と<b>HRハッカー84列原稿</b>
            を生成します。各工程はコードによる検証ゲート（引用照合・数値照合・NGワード・文字数）を通し、通らない項目は空欄＋人間レビュー行きにします。工程を分割しているので、失敗した工程だけ再実行できます。
          </p>
          <div className="legend" id="legend">
            <span className="lg">
              <span className="dot done" />
              生成完了・自動検証済み
            </span>
            <span className="lg">
              <span className="dot plain" />
              生成完了（自動検証なし）
            </span>
            <span className="lg">
              <span className="dot review" />
              警告あり（要確認）
            </span>
            <span className="lg">
              <span className="dot confirmed" />
              コンサル確認済み
            </span>
            <span className="lg">
              <span className="dot stale" />
              要再実行（前工程が更新済み）
            </span>
          </div>
        </header>

        <InputPanel s={s} ctl={ctl} />
        <StepsPanel s={s} ctl={ctl} />

        <div id="status">
          {msg ? <div className={msg.kind === 'err' ? 'err' : 'loading'}>{msg.text}</div> : null}
        </div>

        <ResultSection s={s} stepKey="extract">
          {s.facts ? (
            <ExtractSection facts={s.facts} confirmed={s.confirmed.extract} onConfirm={onConfirm} />
          ) : null}
        </ResultSection>
        <ResultSection s={s} stepKey="analyze">
          {s.analysis ? (
            <AnalyzeSection
              analysis={s.analysis}
              category={s.category}
              knowledgeUsed={s.knowledgeUsed}
              confirmed={s.confirmed.analyze}
              onConfirm={onConfirm}
            />
          ) : null}
        </ResultSection>
        <ResultSection s={s} stepKey="personas">
          {s.resultReady.personas ? (
            <PersonasSection
              personas={s.personas}
              confirmed={s.confirmed.personas}
              onConfirm={onConfirm}
            />
          ) : null}
        </ResultSection>
        <ResultSection s={s} stepKey="copy">
          {s.copies.length ? (
            <CopySection copies={s.copies} confirmed={s.confirmed.copy} onConfirm={onConfirm} />
          ) : null}
        </ResultSection>
        <ResultSection s={s} stepKey="images">
          {s.resultReady.images ? (
            <ImagesSection
              images={s.images}
              imagePrompts={s.imagePrompts}
              imagePromptsError={s.imagePromptsError}
              imagesNv={s.imagesNv}
              imagesNumberCheck={s.imagesNumberCheck}
              confirmed={s.confirmed.images}
              onConfirm={onConfirm}
            />
          ) : null}
        </ResultSection>
        <ResultSection s={s} stepKey="mobile">
          {s.mobile.length ? (
            <MobileSection
              state={s}
              mobile={s.mobile}
              confirmed={s.confirmed.mobile}
              onConfirm={onConfirm}
            />
          ) : null}
        </ResultSection>
        <ResultSection s={s} stepKey="hrhacker">
          {s.hrhacker ? (
            <HrhackerSection h={s.hrhacker} confirmed={s.confirmed.hrhacker} onConfirm={onConfirm} />
          ) : null}
        </ResultSection>
        <ResultSection s={s} stepKey="ab">
          {s.ab ? <AbSection ab={s.ab} confirmed={s.confirmed.ab} onConfirm={onConfirm} /> : null}
        </ResultSection>

        <p className="foot">
          検証はすべてコード側で実施（LLMには検証させない）。戦略成果物②〜⑥⑧は人間向け提案、①⑦は機械データ（84列CSV）。設計正本:
          docs/job_creation_media_engine_generation_pipeline_v1_2026-07-24.md ／
          job_media_engine_rs。最終レビューはコンサルが担保します。
        </p>
      </div>
    </div>
  );
}

/** 画面本体。ストアとコントローラを 1 度だけ作り、状態変化で再描画する。 */
export function JobgenScreen({ post = postJobgen }: { post?: PostFn }) {
  const store: Store<PipelineState> = useMemo(() => createStore(initialState()), []);
  const ctl = useMemo(() => createPipelineController({ store, post }), [store, post]);
  const s = useSyncExternalStore(store.subscribe, store.get, store.get);
  return <JobgenView s={s} ctl={ctl} />;
}
