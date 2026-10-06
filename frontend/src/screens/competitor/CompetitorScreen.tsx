import { useCallback, useEffect, useRef, useState, type SyntheticEvent, type ReactNode } from 'react';
import type { UploadProgress } from '../../api/client';
import type { CompetitorOptions } from '../../generated/CompetitorOptions';
import type { CompetitorReportResponse } from '../../generated/CompetitorReportResponse';
import { createReport, describeFailure, downloadPdf, fetchOptions, type Failure } from './api';
import {
  DEFAULT_FORM,
  buildFormData,
  validateFile,
  validateForm,
  type FormErrors,
  type FormValues,
  type SourceType,
  type WageMode,
} from './form';
import { fmtInt } from './format';
import { ReportView, isTabId, type TabId } from './ReportView';
import './competitor.css';

const STORAGE_KEY = 'competitor.form.v1';
const LOGIN_URL = '/login';

type OptionsState =
  | { status: 'loading' }
  | { status: 'failed' }
  | { status: 'ok'; options: CompetitorOptions };

type PdfState =
  | { status: 'idle' }
  | { status: 'busy' }
  | { status: 'done'; filename: string; url: string }
  | { status: 'error'; failure: Failure };

// ---------------------------------------------------------------- 入力の保存 (CSV 以外)

type Persisted = Omit<FormValues, 'file'>;

function loadValues(): FormValues {
  try {
    const raw = sessionStorage.getItem(STORAGE_KEY);
    if (raw === null) return DEFAULT_FORM;
    const p = JSON.parse(raw) as Partial<Persisted>;
    const str = (v: unknown, d: string): string => (typeof v === 'string' ? v : d);
    return {
      ...DEFAULT_FORM,
      surveyTitle: str(p.surveyTitle, ''),
      marketTitle: str(p.marketTitle, ''),
      prefecture: str(p.prefecture, ''),
      searchKeyword: str(p.searchKeyword, ''),
      includeGoogle: typeof p.includeGoogle === 'boolean' ? p.includeGoogle : true,
      sourceType: p.sourceType === 'indeed' ? 'indeed' : 'indeed_sp',
      wageMode: p.wageMode === 'hourly' ? 'hourly' : 'monthly',
      topN: str(p.topN, DEFAULT_FORM.topN),
    };
  } catch {
    return DEFAULT_FORM;
  }
}

function saveValues(values: FormValues): void {
  try {
    const persisted: Persisted = {
      surveyTitle: values.surveyTitle,
      marketTitle: values.marketTitle,
      prefecture: values.prefecture,
      searchKeyword: values.searchKeyword,
      includeGoogle: values.includeGoogle,
      sourceType: values.sourceType,
      wageMode: values.wageMode,
      topN: values.topN,
    };
    sessionStorage.setItem(STORAGE_KEY, JSON.stringify(persisted));
  } catch {
    // 保存できなくても画面は動く
  }
}

function withoutKey(errors: FormErrors, key: string): FormErrors {
  return Object.fromEntries(Object.entries(errors).filter(([k]) => k !== key));
}

// ---------------------------------------------------------------- ?tab=

function readTab(): TabId {
  const t = new URLSearchParams(window.location.search).get('tab');
  return isTabId(t) ? t : 'excel';
}

function writeTab(tab: TabId): void {
  try {
    const url = new URL(window.location.href);
    url.searchParams.set('tab', tab);
    window.history.replaceState(null, '', url);
  } catch {
    // URL を書き換えられなくても切り替えは効く
  }
}

// ---------------------------------------------------------------- 画面

export function CompetitorScreen() {
  const [values, setValues] = useState<FormValues>(loadValues);
  const [errors, setErrors] = useState<FormErrors>({});
  const [optionsState, setOptionsState] = useState<OptionsState>({ status: 'loading' });
  const [submitting, setSubmitting] = useState(false);
  const [progress, setProgress] = useState<UploadProgress | null>(null);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [result, setResult] = useState<CompetitorReportResponse | null>(null);
  const [tab, setTab] = useState<TabId>(readTab);
  const [pdf, setPdf] = useState<PdfState>({ status: 'idle' });

  // 二重送信の抑止。state は再描画まで古いので、送信の瞬間は ref で見る。
  const busyRef = useRef(false);
  const abortRef = useRef<AbortController | null>(null);
  const blobUrlRef = useRef<string | null>(null);

  useEffect(() => {
    saveValues(values);
  }, [values]);

  useEffect(() => {
    const controller = new AbortController();
    void fetchOptions(controller.signal).then((res) => {
      if (controller.signal.aborted) return;
      setOptionsState(res.ok ? { status: 'ok', options: res.data } : { status: 'failed' });
    });
    return () => {
      controller.abort();
    };
  }, []);

  useEffect(
    () => () => {
      abortRef.current?.abort();
      if (blobUrlRef.current !== null) URL.revokeObjectURL(blobUrlRef.current);
    },
    [],
  );

  const options = optionsState.status === 'ok' ? optionsState.options : null;
  const prefectures = options?.prefectures ?? [];

  const update = useCallback(<K extends keyof FormValues>(key: K, value: FormValues[K]): void => {
    setValues((v) => ({ ...v, [key]: value }));
    setErrors((e) => withoutKey(e, key));
  }, []);

  const onFile = (file: File | null): void => {
    setValues((v) => ({ ...v, file }));
    const fileError = file === null ? undefined : validateFile(file);
    setErrors((e) => {
      const rest = withoutKey(e, 'file');
      return fileError === undefined ? rest : { ...rest, file: fileError };
    });
  };

  const onSubmit = (event: SyntheticEvent): void => {
    event.preventDefault();
    if (busyRef.current) return;
    const found = validateForm(values, prefectures);
    setErrors(found);
    if (Object.keys(found).length > 0) {
      setFailure(null);
      return;
    }
    busyRef.current = true;
    const controller = new AbortController();
    abortRef.current = controller;
    setSubmitting(true);
    setFailure(null);
    setProgress(null);
    void createReport(buildFormData(values), {
      signal: controller.signal,
      onProgress: setProgress,
    }).then((res) => {
      if (abortRef.current !== controller) return; // 画面が閉じられた
      busyRef.current = false;
      abortRef.current = null;
      setSubmitting(false);
      setProgress(null);
      if (res.ok) {
        setResult(res.data);
        setPdf({ status: 'idle' });
        setFailure(null);
      } else {
        setFailure(describeFailure(res.error, 'report'));
      }
    });
  };

  const onCancel = (): void => {
    abortRef.current?.abort();
  };

  const onPdf = (): void => {
    if (busyRef.current || result === null) return;
    busyRef.current = true;
    setPdf({ status: 'busy' });
    void downloadPdf(result.report_id).then((res) => {
      busyRef.current = false;
      if (!res.ok) {
        setPdf({ status: 'error', failure: res });
        return;
      }
      if (blobUrlRef.current !== null) URL.revokeObjectURL(blobUrlRef.current);
      const url = URL.createObjectURL(res.blob);
      blobUrlRef.current = url;
      const link = document.createElement('a');
      link.href = url;
      link.download = res.filename;
      document.body.append(link);
      link.click();
      link.remove();
      setPdf({ status: 'done', filename: res.filename, url });
    });
  };

  const backToForm = (): void => {
    if (busyRef.current) return;
    setResult(null);
    setPdf({ status: 'idle' });
  };

  const changeTab = (next: TabId): void => {
    setTab(next);
    writeTab(next);
  };

  return (
    <main className="competitor">
      <nav className="cmp-nav" aria-label="機能選択">
        <a href="/">媒体分析</a>
        <strong aria-current="page">競合調査</strong>
        <a href="/?tab=%2Ftab%2Findeed">採用市場</a>
      </nav>
      <h1>競合調査</h1>
      {result === null ? (
        <FormView
          values={values}
          errors={errors}
          optionsState={optionsState}
          submitting={submitting}
          progress={progress}
          failure={failure}
          update={update}
          onFile={onFile}
          onSubmit={onSubmit}
          onCancel={onCancel}
        />
      ) : (
        <ResultView
          data={result}
          tab={tab}
          pdf={pdf}
          onTab={changeTab}
          onPdf={onPdf}
          onBack={backToForm}
        />
      )}
    </main>
  );
}

// ---------------------------------------------------------------- フォーム

interface FormViewProps {
  values: FormValues;
  errors: FormErrors;
  optionsState: OptionsState;
  submitting: boolean;
  progress: UploadProgress | null;
  failure: Failure | null;
  update: <K extends keyof FormValues>(key: K, value: FormValues[K]) => void;
  onFile: (file: File | null) => void;
  onSubmit: (event: SyntheticEvent) => void;
  onCancel: () => void;
}

function FormView(p: FormViewProps) {
  const { values, errors, optionsState, submitting, progress, failure } = p;
  const options = optionsState.status === 'ok' ? optionsState.options : null;
  const marketOff = !options?.market_available;

  return (
    <>
      <p className="cmp-lead">
        給与・待遇・検索需要・採用市場を、グラフで確認できます。
      </p>
      <form onSubmit={p.onSubmit} noValidate aria-busy={submitting}>
        <section className="cmp-card">
          <h2>1. 調査する職種・地域</h2>
          <Field label="調査名" htmlFor="survey-title" error={errors.surveyTitle}>
            <input
              id="survey-title"
              name="survey_title"
              maxLength={200}
              placeholder="例：大阪府・施設長・正社員"
              value={values.surveyTitle}
              onChange={(e) => {
                p.update('surveyTitle', e.target.value);
              }}
            />
          </Field>
          <div className="cmp-grid">
            <Field label="Indeed採用市場の職種" htmlFor="market-title" help="CSVの職種に対応する項目を選びます。">
              <select
                id="market-title"
                name="market_title"
                disabled={marketOff}
                value={values.marketTitle}
                onChange={(e) => {
                  p.update('marketTitle', e.target.value);
                }}
              >
                <option value="">使用しない</option>
                {options?.titles.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
            </Field>
            <Field
              label="対象都道府県"
              htmlFor="prefecture"
              error={errors.prefecture}
              help="外部データの対象地域です。CSVの求人は地域で絞り込みません。"
            >
              <select
                id="prefecture"
                name="prefecture"
                value={values.prefecture}
                onChange={(e) => {
                  p.update('prefecture', e.target.value);
                }}
              >
                <option value="">全国</option>
                {options?.prefectures.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
            </Field>
          </div>
          <p className="cmp-hint">
            {optionsState.status === 'loading' && '選択肢を読み込んでいます…'}
            {optionsState.status === 'failed' &&
              '選択肢を取得できませんでした。ページを再読み込みしてください。CSVの競合調査は利用できます。'}
            {options !== null &&
              (options.market_available
                ? 'Indeed採用市場データを利用できます。'
                : 'Indeed採用市場データは取得できません。CSVの競合調査は利用できます。')}
          </p>
          <Field
            label="Googleで調べる検索語"
            htmlFor="search-keyword"
            error={errors.searchKeyword}
            help="空欄の場合は選択したIndeed職種に「求人」を付けて検索します。カンマ・改行で区切ると複数語を調べます。"
          >
            <input
              id="search-keyword"
              name="search_keyword"
              maxLength={200}
              placeholder="例：施設長 求人"
              value={values.searchKeyword}
              onChange={(e) => {
                p.update('searchKeyword', e.target.value);
              }}
            />
          </Field>
          <label className="cmp-check">
            <input
              type="checkbox"
              name="include_google"
              checked={values.includeGoogle}
              onChange={(e) => {
                p.update('includeGoogle', e.target.checked);
              }}
            />
            Google広告APIの検索需要・関連キーワードを取得する
          </label>
        </section>

        <section className="cmp-card">
          <h2>2. 競合求人のCSV</h2>
          <p>
            調査対象の職種・地域・雇用形態で集めたCSVを使用します。検索結果の順序を保つと、先頭の求人と全体を比較できます。
          </p>
          <div className="cmp-grid">
            <Field label="CSVの媒体" htmlFor="source-type">
              <select
                id="source-type"
                name="source_type"
                value={values.sourceType}
                onChange={(e) => {
                  p.update('sourceType', e.target.value as SourceType);
                }}
              >
                <option value="indeed_sp">Indeed (SP・スマホ版)</option>
                <option value="indeed">Indeed (PC版)</option>
              </select>
            </Field>
            <Field
              label="給与の表示単位"
              htmlFor="wage-mode"
              help="CSVの給与が時給なら時給を選んでください。単位が違うと全指標が意味を持ちません。"
            >
              <select
                id="wage-mode"
                name="wage_mode"
                value={values.wageMode}
                onChange={(e) => {
                  p.update('wageMode', e.target.value as WageMode);
                }}
              >
                <option value="monthly">月給</option>
                <option value="hourly">時給</option>
              </select>
            </Field>
          </div>
          <Field
            label="先頭の求人を何件比較するか"
            htmlFor="top-n"
            error={errors.topN}
            help="1〜200 の整数。"
          >
            <input
              id="top-n"
              name="top_n"
              type="number"
              min={1}
              max={200}
              inputMode="numeric"
              value={values.topN}
              onChange={(e) => {
                p.update('topN', e.target.value);
              }}
            />
          </Field>
          <Field
            label="求人一覧CSV"
            htmlFor="csv-file"
            error={errors.file}
            help="UTF-8推奨。20MBまで、.csv / .txt。Excelブックは求人一覧シートをCSVに書き出してください。人気比較には人気・超人気タグのあるSP版CSVが必要です。"
          >
            <input
              id="csv-file"
              name="csv_file"
              type="file"
              accept=".csv,.txt"
              onChange={(e) => {
                p.onFile(e.target.files?.[0] ?? null);
              }}
            />
          </Field>
          {values.file !== null && (
            <p className="cmp-file">
              選択中: {values.file.name}（{fmtInt(values.file.size)} バイト）
            </p>
          )}

          <FormStatus submitting={submitting} progress={progress} failure={failure} />
          <div className="cmp-actions">
            <button type="submit" className="cmp-btn cmp-btn--primary" disabled={submitting}>
              レポートを作成
            </button>
            {submitting && (
              <button type="button" className="cmp-btn" onClick={p.onCancel}>
                キャンセル
              </button>
            )}
          </div>
        </section>
      </form>
      <p className="cmp-lead">
        給与・待遇、検索需要、採用市場、人口、採用のヒントをA3横のPDFで保存できます。
      </p>
    </>
  );
}

function FormStatus({
  submitting,
  progress,
  failure,
}: {
  submitting: boolean;
  progress: UploadProgress | null;
  failure: Failure | null;
}) {
  let status = '';
  if (submitting) {
    status = 'レポートを作成しています。Googleのデータ取得には時間がかかる場合があります。';
    if (progress !== null && progress.ratio < 1) {
      status += ` アップロード中 ${String(Math.round(progress.ratio * 100))}%`;
    }
  } else if (failure?.kind === 'aborted') {
    status = failure.message;
  }
  return (
    <>
      <p role="status" aria-live="polite" className="cmp-status">
        {status}
      </p>
      {failure !== null && failure.kind !== 'aborted' && !submitting && (
        <div role="alert" className="cmp-error">
          <p>{failure.message}</p>
          {failure.kind === 'auth' && <LoginLink />}
        </div>
      )}
    </>
  );
}

function LoginLink() {
  return (
    <p>
      <a href={LOGIN_URL} target="_blank" rel="noopener noreferrer">
        ログイン画面を別タブで開く
      </a>
      （ログイン後、この画面でもう一度「レポートを作成」を押してください）
    </p>
  );
}

function Field(props: {
  label: string;
  htmlFor: string;
  error?: string | undefined;
  help?: string;
  children: ReactNode;
}) {
  const errorId = `${props.htmlFor}-error`;
  return (
    <div className="cmp-field">
      <label htmlFor={props.htmlFor}>{props.label}</label>
      {props.children}
      {props.help !== undefined && <small>{props.help}</small>}
      {props.error !== undefined && (
        <p id={errorId} role="alert" className="cmp-field-error">
          {props.error}
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- 結果

interface ResultViewProps {
  data: CompetitorReportResponse;
  tab: TabId;
  pdf: PdfState;
  onTab: (tab: TabId) => void;
  onPdf: () => void;
  onBack: () => void;
}

function ResultView({ data, tab, pdf, onTab, onPdf, onBack }: ResultViewProps) {
  const { meta } = data.report;
  const busy = pdf.status === 'busy';
  return (
    <>
      <section className="cmp-card cmp-result-head">
        <div data-testid="result-summary" className="cmp-summary-line">
          <strong>{meta.title}</strong>
          <span>CSV重複排除後 {fmtInt(meta.total_count)} 件</span>
          <span>給与の単位: {meta.unit}</span>
          <span>{meta.is_hourly ? '時給' : '月給'}</span>
          {meta.prefecture !== null && <span>{meta.prefecture}</span>}
        </div>
        <div className="cmp-actions">
          <button type="button" className="cmp-btn cmp-btn--primary" disabled={busy} onClick={onPdf}>
            PDFをダウンロード
          </button>
          <button type="button" className="cmp-btn" disabled={busy} onClick={onBack}>
            条件を変えて再作成
          </button>
        </div>
        <PdfStatus pdf={pdf} />
        <p className="cmp-hint">
          このレポートは {String(Math.round(data.expires_in_secs / 60))}{' '}
          分間保持され、PDFの作成に使われます。ページを再読み込みすると結果は消えるので、もう一度作成してください。
        </p>
        <Warnings report={data} />
      </section>
      <ReportView report={data.report} tab={tab} onTabChange={onTab} />
    </>
  );
}

function PdfStatus({ pdf }: { pdf: PdfState }) {
  let text = '';
  if (pdf.status === 'busy') {
    text = 'PDFを作成しています。他のPDFを作成中の場合は順番待ちになります（最長3分ほど）。';
  } else if (pdf.status === 'done') {
    text = 'PDFをダウンロードしました。PDFを開いて印刷できます。';
  }
  return (
    <>
      <p role="status" aria-live="polite" className="cmp-status">
        {text}
        {pdf.status === 'done' && (
          <>
            {' '}
            <a href={pdf.url} download={pdf.filename}>
              ダウンロードが始まらない場合はこちら
            </a>
          </>
        )}
      </p>
      {pdf.status === 'error' && (
        <div role="alert" className="cmp-error">
          <p>{pdf.failure.message}</p>
          {pdf.failure.kind === 'auth' && <LoginLink />}
        </div>
      )}
    </>
  );
}

function Warnings({ report }: { report: CompetitorReportResponse }) {
  const { meta } = report.report;
  const items: string[] = [...meta.warnings];
  if (meta.salary_parsed_count === 0) {
    items.push(
      '給与を読み取れた求人が 0 件でした。CSVの給与列と、給与の表示単位（月給/時給）の選択を確認してください。',
    );
  } else if (meta.salary_missing_count > 0) {
    items.push(`給与の下限が読めなかった求人が ${fmtInt(meta.salary_missing_count)} 件あります。`);
  }
  if (items.length === 0) return null;
  return (
    <ul className="cmp-warnings" aria-label="確認してください">
      {items.map((t, i) => (
        <li key={i}>{t}</li>
      ))}
    </ul>
  );
}
