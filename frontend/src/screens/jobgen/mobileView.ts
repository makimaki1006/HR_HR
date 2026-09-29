// ⑥ スマホ原稿の「求人ページ風 作成例」の表示モデル (旧 jobPostCard / ipopPush の純粋部分)。
// ⑥スマホ原稿を本文に、①事実／②強み／④コピー／⑤画像案の生成済みデータだけを流し込む。
// データが無いブロックは出力しない。意図ポップアップは値が無い行を捨て、1 行も無ければ付けない
// (実物らしく見せるために意図を創作しない)。
import type { Persona } from '../../generated/Persona';
import { FKEY_JA, type MobileResult, type PipelineState, PV_FACT_KEYS } from './state';

export interface IntentPopup {
  head: string;
  rows: [string, string][];
  foot: string;
}

export interface FactBadge {
  key: string;
  label: string;
  cls: string;
  value: string;
  popup: number | null;
}

export interface ReqRow {
  key: string;
  label: string;
  value: string;
  popup: number | null;
}

export interface JobPostModel {
  label: string;
  photoPopup: number | null;
  photoDirection: string;
  catchCopy: { style: string; text: string; popup: number | null } | null;
  title: string;
  badges: FactBadge[];
  chips: string[];
  chipsPopup: number | null;
  bodyPopup: number | null;
  lines: string[];
  reqRows: ReqRow[];
}

export interface MobileView {
  popups: IntentPopup[];
  cards: JobPostModel[];
}

const PV_FACT_SRC = '①事実抽出で原文と一字一句照合した項目';

function personaRows(p: Persona | null): [string, string][] {
  return p
    ? [
        ['ターゲット', p.label || ''],
        ['人物像', p.profile || ''],
        ['現職の不満', p.dissatisfaction || ''],
        ['痛み', p.pain || ''],
        ['生活環境', p.environment || ''],
      ]
    : [];
}

/** 作成例に出す職種名 (職種名欄の値、無ければ先頭行候補)。 */
export function previewTitle(s: PipelineState): string {
  return (s.jobTitle || s.titleHint || '').trim();
}

export function buildMobileView(s: PipelineState, mobile: MobileResult[]): MobileView {
  const popups: IntentPopup[] = [];
  const push = (head: string, rows: [string, string][], foot: string): number | null => {
    const rs = rows.filter((r) => r[1].trim());
    if (!rs.length) return null;
    popups.push({ head, rows: rs, foot });
    return popups.length - 1;
  };
  const fact = (k: string): string => {
    const f = s.facts?.[k];
    return f?.status === 'verified' && f.value ? f.value.trim() : '';
  };
  const quote = (k: string): string => s.facts?.[k]?.evidence_quote.trim() ?? '';
  const title = previewTitle(s);
  const chips = s.analysis
    ? s.analysis.surface_strengths.filter((x) => x.trim()).slice(0, 4)
    : [];

  const cards = mobile
    .filter((m) => m.error === undefined)
    .map((m): JobPostModel => {
      const label = m.label;
      const persona = s.personas.find((p) => (p.label || '') === label) ?? null;
      const prows = personaRows(persona);
      const cp = s.copies.find((c) => c.label === label)?.copies.find((x) => x.text.trim()) ?? null;
      const dir = s.images.find((x) => (x.persona_label || '') === label);
      const dirTxt = dir ? dir.direction.trim() : '';
      const ip = s.imagePrompts.find((x) => (x.persona_label || '') === label);
      const appeal = ip ? ip.appeal_core.trim() : '';
      const photoPopup = push(
        '写真の意図',
        [['画像の狙い', appeal], ...prows],
        appeal ? '出所: ⑤画像案・生成プロンプト／③ペルソナ設計' : '出所: ③ペルソナ設計',
      );
      const catchPopup = cp
        ? push('キャッチコピーの意図', [...prows, ['コピーの型', cp.style]], '出所: ③ペルソナ設計／④キャッチコピー')
        : null;
      const badgeDefs: [string, string, string][] = [
        ['salary', '給与', ' pay'],
        ['work_location', '勤務地', ''],
        ['employment_type', '雇用形態', ''],
      ];
      const badges: FactBadge[] = [];
      for (const [key, label2, cls] of badgeDefs) {
        const v = fact(key);
        if (!v) continue;
        const popup = push(label2 + 'の出所', [['原文の引用', quote(key)]], PV_FACT_SRC);
        badges.push({ key, label: label2, cls, value: v, popup });
      }
      const chipsPopup = chips.length
        ? push('特徴タグの出所', [['出所', '②市場分析で抽出した表面の強み']], '')
        : null;
      const bodyPopup = push(
        'この本文の意図',
        prows,
        '出所: ③ペルソナ設計（このペルソナ向けに⑥スマホ原稿を生成）',
      );
      const reqRows: ReqRow[] = [];
      for (const k of PV_FACT_KEYS) {
        const v = fact(k);
        if (!v) continue;
        const ja = FKEY_JA[k] ?? k;
        const popup = push(ja + 'の出所', [['原文の引用', quote(k)]], PV_FACT_SRC);
        reqRows.push({ key: k, label: ja, value: v, popup });
      }
      return {
        label,
        photoPopup,
        photoDirection: dirTxt,
        catchCopy: cp ? { style: cp.style, text: cp.text, popup: catchPopup } : null,
        title,
        badges,
        chips,
        chipsPopup,
        bodyPopup,
        lines: m.lines,
        reqRows,
      };
    });
  return { popups, cards };
}
