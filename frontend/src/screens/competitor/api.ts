// 競合調査 API の呼び出し (GET options / POST report / POST pdf) とエラーの文言。
// 型は ts-rs の生成物 (src/generated)。応答の中身はサーバを信頼して検証しない (他の画面と同じ)。
import {
  ApiAbortedError,
  ApiHttpError,
  ApiNetworkError,
  ApiTimeoutError,
  AuthRequiredError,
  apiGet,
  apiUpload,
  type ApiError,
  type ApiResult,
  type UploadProgress,
} from '../../api/client';
import type { CompetitorErrorCode } from '../../generated/CompetitorErrorCode';
import type { CompetitorOptions } from '../../generated/CompetitorOptions';
import type { CompetitorReportResponse } from '../../generated/CompetitorReportResponse';

/** レポート作成。アップロード + 集計 + Google 待ち (サーバ側 45 秒) を含めて 120 秒より長く待つ。 */
export const REPORT_TIMEOUT_MS = 150_000;
/** PDF 作成。サーバ側の最悪待ち (混雑待ち 60 秒 + 描画 45 秒) を超える。 */
export const PDF_TIMEOUT_MS = 180_000;

const OPTIONS_PATH = '/api/competitor/options';
const REPORT_PATH = '/api/competitor/report';
const PDF_PATH = '/api/competitor/pdf';
const DEFAULT_PDF_NAME = 'competitor-report.pdf';

export function fetchOptions(signal?: AbortSignal): Promise<ApiResult<CompetitorOptions>> {
  return apiGet<CompetitorOptions>(OPTIONS_PATH, signal ? { signal } : {});
}

export interface CreateOptions {
  signal?: AbortSignal;
  onProgress?: (progress: UploadProgress) => void;
}

export function createReport(
  form: FormData,
  options: CreateOptions = {},
): Promise<ApiResult<CompetitorReportResponse>> {
  return apiUpload<CompetitorReportResponse>(REPORT_PATH, form, {
    timeoutMs: REPORT_TIMEOUT_MS,
    ...(options.signal ? { signal: options.signal } : {}),
    ...(options.onProgress ? { onProgress: options.onProgress } : {}),
  });
}

// ---------------------------------------------------------------- エラーの文言

export type FailureKind =
  | 'auth' // 401: ログイン切れ (入力は残す)
  | 'expired' // 404: report_id の期限切れ・存在しない
  | 'busy' // PDF 作成が混み合っている
  | 'in_progress' // 前のレポート/PDF を作成中 (429)
  | 'timeout'
  | 'aborted'
  | 'network'
  | 'incomplete' // PDF が不完全 / PDF ではない
  | 'input' // CSV・入力の問題 (コード付き)
  | 'server';

export interface Failure {
  kind: FailureKind;
  code?: CompetitorErrorCode;
  message: string;
}

type PdfFailure = { ok: false } & Failure;
export type PdfResult = { ok: true; blob: Blob; filename: string } | PdfFailure;

type Context = 'report' | 'pdf';

const KNOWN_CODES: readonly CompetitorErrorCode[] = [
  'csv_missing',
  'csv_unreadable',
  'csv_too_large',
  'csv_too_many_rows',
  'csv_parse_failed',
  'no_indeed_jobs',
  'field_too_long',
  'invalid_source_type',
  'invalid_wage_mode',
  'invalid_prefecture',
  'report_in_progress',
  'report_not_found',
  'invalid_request',
  'pdf_busy',
  'pdf_timeout',
  'pdf_failed',
];

/** サーバが本文を返さなかった・読めなかったときの固定文 (コード別)。 */
const CODE_MESSAGE: Record<CompetitorErrorCode, string> = {
  csv_missing: '求人一覧CSVを選択してください。',
  csv_unreadable: 'CSVを読み込めませんでした。ファイルサイズと形式を確認してください。',
  csv_too_large: 'CSVのサイズが上限(20MB)を超えています。',
  csv_too_many_rows: 'CSVの行数が上限を超えています。分割して作成してください。',
  csv_parse_failed: 'CSVを解析できませんでした。列名と形式を確認してください。',
  no_indeed_jobs: '分析できるIndeed求人がありません。CSVの列と内容を確認してください。',
  field_too_long: '入力が長すぎます。調査名・検索語を短くしてください。',
  invalid_source_type: 'CSVの媒体の選択が正しくありません。',
  invalid_wage_mode: '給与の表示単位の選択が正しくありません。',
  invalid_prefecture: '対象都道府県の選択が正しくありません。',
  report_in_progress: '前のレポートを作成中です。完了してからもう一度お試しください。',
  report_not_found: 'このレポートは保持期限が切れたか見つかりません。もう一度作成してください。',
  invalid_request: 'リクエストを読み取れませんでした。',
  pdf_busy: 'PDF作成が混み合っています。少し時間をおいて再度お試しください。',
  pdf_timeout: 'PDFの作成に時間がかかっています。もう一度お試しください。',
  pdf_failed: 'PDFを作成できませんでした。時間をおいてもう一度お試しください。',
};

const SERVER_MESSAGE: Record<Context, string> = {
  report: 'レポートを作成できませんでした。時間をおいてもう一度お試しください。',
  pdf: 'PDFを作成できませんでした。時間をおいてもう一度お試しください。',
};

const EXPIRED_MESSAGE =
  'このレポートは保持期限(30分)が切れたか、見つかりません。もう一度作成してください。';

const AUTH_MESSAGE =
  'ログインの有効期限が切れました。入力した内容はこの画面に残しています。別のタブでログインし直してから、もう一度お試しください。';

function kindOfCode(code: CompetitorErrorCode): FailureKind {
  switch (code) {
    case 'report_in_progress':
      return 'in_progress';
    case 'report_not_found':
      return 'expired';
    case 'pdf_busy':
      return 'busy';
    case 'pdf_timeout':
      return 'timeout';
    case 'pdf_failed':
    case 'invalid_request':
      return 'server';
    default:
      return 'input';
  }
}

function parseErrorBody(body: unknown): { code?: CompetitorErrorCode; message?: string } {
  if (typeof body !== 'object' || body === null) return {};
  const raw = body as { error?: unknown; message?: unknown };
  const code = KNOWN_CODES.find((c) => c === raw.error);
  const message = typeof raw.message === 'string' && raw.message !== '' ? raw.message : undefined;
  return {
    ...(code ? { code } : {}),
    ...(code && message ? { message } : {}), // 知らないコードの文言は信用しない
  };
}

function timeoutMessage(ctx: Context): string {
  const secs = (ctx === 'pdf' ? PDF_TIMEOUT_MS : REPORT_TIMEOUT_MS) / 1000;
  const what = ctx === 'pdf' ? 'PDF' : 'レポート';
  return `${what}の作成に時間がかかり、${String(secs)}秒で中断しました。Googleの応答待ちが長引いている場合があります。もう一度お試しください。`;
}

function failureFromHttp(status: number, body: unknown, ctx: Context): Failure {
  const { code, message } = parseErrorBody(body);
  if (code) {
    const kind = kindOfCode(code);
    // 期限切れの案内は「もう一度作成してください」で統一する (サーバの文言は使わない)。
    if (kind === 'expired') return { kind, code, message: EXPIRED_MESSAGE };
    return { kind, code, message: message ?? CODE_MESSAGE[code] };
  }
  if (status === 429) return { kind: 'in_progress', message: CODE_MESSAGE.report_in_progress };
  if (status === 404 && ctx === 'pdf') return { kind: 'expired', message: EXPIRED_MESSAGE };
  return { kind: 'server', message: SERVER_MESSAGE[ctx] };
}

/** apiUpload / apiGet が返す ApiError を、画面に出す形にする。 */
export function describeFailure(error: ApiError, ctx: Context): Failure {
  if (error instanceof AuthRequiredError) return { kind: 'auth', message: AUTH_MESSAGE };
  if (error instanceof ApiTimeoutError) return { kind: 'timeout', message: timeoutMessage(ctx) };
  if (error instanceof ApiAbortedError) return { kind: 'aborted', message: 'キャンセルしました。' };
  if (error instanceof ApiNetworkError) {
    return { kind: 'network', message: '通信に失敗しました。接続を確認して、もう一度お試しください。' };
  }
  if (error instanceof ApiHttpError) return failureFromHttp(error.status, error.body, ctx);
  return { kind: 'server', message: SERVER_MESSAGE[ctx] };
}

// ---------------------------------------------------------------- PDF

/** 旧画面と同じ検査: 先頭が `%PDF-`、末尾 32 バイトに `%%EOF` (途中切断の検知)。 */
export function isCompletePdf(bytes: Uint8Array): boolean {
  if (bytes.length < 5) return false;
  const head = String.fromCharCode(...bytes.slice(0, 5));
  if (head !== '%PDF-') return false;
  const tail = String.fromCharCode(...bytes.slice(Math.max(0, bytes.length - 32)));
  return tail.includes('%%EOF');
}

function safeFilename(name: string): string {
  // eslint-disable-next-line no-control-regex
  const cleaned = name.replace(/[\u0000-\u001f\u007f\\/:*?"<>|]+/g, '_').replace(/^[._]+/, '');
  return cleaned === '' ? DEFAULT_PDF_NAME : cleaned;
}

/** `filename*=UTF-8''...` (RFC 5987) を優先し、無ければ `filename="..."`、それも無ければ既定名。 */
export function filenameFromDisposition(header: string | null): string {
  if (header === null) return DEFAULT_PDF_NAME;
  const star = /filename\*\s*=\s*UTF-8''([^;]+)/i.exec(header);
  if (star?.[1]) {
    try {
      return safeFilename(decodeURIComponent(star[1].trim()));
    } catch {
      // 壊れた % エスケープ: filename= に落とす
    }
  }
  const plain = /filename\s*=\s*"([^"]+)"/i.exec(header) ?? /filename\s*=\s*([^;\s]+)/i.exec(header);
  if (plain?.[1]) return safeFilename(plain[1]);
  return DEFAULT_PDF_NAME;
}

function pdfFailure(failure: Failure): PdfFailure {
  return { ok: false, ...failure };
}

/**
 * 保持中のレポート (`report_id`) から PDF を作る。Google は再取得しない (サーバがレポートを保持している)。
 * 例外は投げず、失敗は `{ ok: false, kind, message }` で返す。待ち時間の上限は 180 秒。
 */
export async function downloadPdf(reportId: string, signal?: AbortSignal): Promise<PdfResult> {
  const controller = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, PDF_TIMEOUT_MS);
  const onExternalAbort = (): void => {
    controller.abort();
  };
  if (signal?.aborted) controller.abort();
  else signal?.addEventListener('abort', onExternalAbort, { once: true });

  const interrupted = (): PdfFailure =>
    timedOut
      ? pdfFailure({ kind: 'timeout', message: timeoutMessage('pdf') })
      : signal?.aborted
        ? pdfFailure({ kind: 'aborted', message: 'キャンセルしました。' })
        : pdfFailure({
            kind: 'network',
            message: '通信に失敗しました。接続を確認して、もう一度お試しください。',
          });

  try {
    let res: Response;
    try {
      res = await fetch(PDF_PATH, {
        method: 'POST',
        credentials: 'same-origin',
        headers: {
          Accept: 'application/pdf, application/json',
          'Content-Type': 'application/json',
          'X-Requested-With': 'fetch',
        },
        body: JSON.stringify({ report_id: reportId }),
        signal: controller.signal,
      });
    } catch {
      return interrupted();
    }

    if (res.redirected && safePathname(res.url) === '/login') {
      return pdfFailure({ kind: 'auth', message: AUTH_MESSAGE });
    }
    if (res.status === 401) return pdfFailure({ kind: 'auth', message: AUTH_MESSAGE });

    if (!res.ok) {
      let body: unknown;
      try {
        body = JSON.parse(await res.text());
      } catch {
        body = undefined;
      }
      return pdfFailure(failureFromHttp(res.status, body, 'pdf'));
    }

    if (!/^application\/pdf\b/i.test((res.headers.get('content-type') ?? '').trim())) {
      return pdfFailure({
        kind: 'incomplete',
        message: 'PDFを取得できませんでした。再度お試しください。',
      });
    }
    let bytes: Uint8Array;
    try {
      bytes = new Uint8Array(await res.arrayBuffer());
    } catch {
      return interrupted();
    }
    if (!isCompletePdf(bytes)) {
      return pdfFailure({
        kind: 'incomplete',
        message: 'PDFファイルが不完全です。再度お試しください。',
      });
    }
    return {
      ok: true,
      blob: new Blob([bytes as BlobPart], { type: 'application/pdf' }),
      filename: filenameFromDisposition(res.headers.get('content-disposition')),
    };
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener('abort', onExternalAbort);
  }
}

function safePathname(url: string): string {
  try {
    return new URL(url).pathname;
  } catch {
    return '';
  }
}
