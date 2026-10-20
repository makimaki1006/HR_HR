// 顧客向け応募者画面（/portal）の UI 試作で使う型。
// サーバーはまだ無い。docs/architecture/customer-applicant-portal-plan.html の 9 章の表に合わせ、
// 後で Rust の型（ts-rs）に置き換える前提で、画面が必要とする形だけを置く。

/** 顧客ごとの移行の段階。 */
export type CustomerStage = 'sheet_only' | 'both' | 'portal_only';

export interface Customer {
  id: string;
  /** 社内での表示名（架空）。 */
  name: string;
  stage: CustomerStage;
  disabled: boolean;
  /** 最後に誰かが画面を使った日時（ISO）。 */
  lastUsedAt: string | null;
}

export type ColumnGroup = 'summary' | 'customer' | 'staff' | 'other';
export type ColumnKind = 'text' | 'date' | 'datetime' | 'phone' | 'email' | 'choice';

/** 列の設定。シートの見出しの列ごと。 */
export interface ColumnConfig {
  /** シート上の列の見出し（元の名前）。 */
  header: string;
  /** 画面での表示名（空なら header）。 */
  label: string;
  visible: boolean;
  writable: boolean;
  /** プルダウン（入力規則）がある列の選択肢。 */
  options: string[] | null;
  /** 長い文章の列（複数行で入力）。 */
  multiline: boolean;
  /** 詳細でのまとまり。見出し=詳細の上部に出す基本情報。未設定は、書ける列は「貴社で記入」、書けない列は「そのほか」。 */
  group?: ColumnGroup;
  /** 値の種類。入力欄と表示の形が変わる。未設定は文字。 */
  kind?: ColumnKind;
  /** 一覧（PC の表・スマホのカード）に出す列。詳細だけに出す列は false。 */
  inList: boolean;
}

export interface SheetConfig {
  id: string;
  customerId: string;
  /** 顧客に見せるタブの名前（例: 「応募者一覧」「2025年度」）。 */
  tabName: string;
  headerRow: number;
  /** 一覧のカードと表で名前として太く出す列（見出しの名前）。未設定は最初の見せる列。 */
  titleColumn?: string;
  /** 選考状況のバッジ・絞り込みに使う列。未設定は出さない。 */
  statusColumn?: string;
  /** 新しい順に並べる日付の列。未設定は並べ替えない。 */
  sortColumn?: string;
  columns: ColumnConfig[];
  /** 接続の状態。 */
  connection: 'ok' | 'not_shared' | 'not_found' | 'header_changed';
  /** 選考状況の選択肢ごとの段階（数字の集計に使う）。キーは選択肢の文字。 */
  stageMap?: Record<string, FunnelStage>;
  /** 顧客が項目・プルダウンを変えてよいか（社内が決める）。 */
  customerCanEditColumns?: boolean;
}

/** 応募から採用までの段階。数字（面接移行率・採用率など）の集計に使う。 */
export type FunnelStage = 'applied' | 'screening' | 'interview_set' | 'interviewed' | 'offer' | 'hired' | 'rejected' | 'declined';

/** シートの 1 行。値は見出し（header）をキーにした文字列。 */
export interface ApplicantRow {
  /** シートの行番号（表示しない。保存の照合用）。 */
  rowNumber: number;
  values: Record<string, string>;
  /** 前回この人が見たあとに増えた・変わった行。 */
  changedSinceLastView: 'new' | 'updated' | null;
}

export interface SheetSnapshot {
  sheetId: string;
  /** シートから読んだ時刻（ISO）。「○時○分時点」に使う。 */
  readAt: string;
  rows: ApplicantRow[];
}

export type AccountStatus = 'invited' | 'active' | 'locked' | 'disabled';

export interface PortalAccount {
  id: string;
  customerId: string;
  email: string;
  /** 本人の表示名（任意）。 */
  displayName: string;
  status: AccountStatus;
  /** 確認コードの追加の送り先。 */
  extraCodeEmail: string | null;
  lastLoginAt: string | null;
  invitedAt: string;
  /** 招待リンクの期限（status = invited のとき）。 */
  inviteExpiresAt: string | null;
  /** status = locked のとき、自動で解除される時刻。 */
  lockedUntil?: string | null;
}

export interface TrustedDevice {
  id: string;
  accountId: string;
  /** 「Windows の Chrome」「iPhone の Safari」のような説明。 */
  label: string;
  lastUsedAt: string;
  expiresAt: string;
  current: boolean;
}

export type ActivityKind =
  | 'login' | 'login_failed' | 'code_sent' | 'view' | 'save' | 'save_conflict' | 'invite' | 'disable' | 'device_removed'
  /** シート側（社内の人がシートで直接）で変わった値。 */
  | 'sheet_change'
  /** 項目・プルダウン・段階の設定の変更。 */
  | 'column_change'
  | 'export';

export interface ActivityEntry {
  id: string;
  at: string;
  accountEmail: string;
  customerId: string;
  kind: ActivityKind;
  /** 「田中 太郎さんの行の『面接日』を 10/24 に変更」のような説明。 */
  detail: string;
  /** 誰が（表示名）。社内の人は「弊社 山口」の形。 */
  actorName?: string;
  /** どこからの操作か。画面・シートで直接・社内の管理画面。 */
  source?: 'portal' | 'sheet' | 'admin';
  sheetId?: string;
  rowNumber?: number;
  /** 変わった項目（見出しの名前）。 */
  column?: string;
  before?: string;
  after?: string;
}

/** 1 マスの保存の結果。 */
export type CellSaveResult =
  | { kind: 'saved'; value: string }
  | { kind: 'conflict'; latest: string; by: string; at: string }
  | { kind: 'failed'; message: string };

/** 画面を見ている人。顧客の担当者か、社内の人（顧客を切り替えて見られる）。 */
export type Viewer = { kind: 'customer'; accountId: string } | { kind: 'staff'; name: string };

/** 保存の結果。 */
export type SaveResult =
  | { kind: 'saved'; row: ApplicantRow }
  | { kind: 'conflict'; latest: ApplicantRow }
  | { kind: 'failed'; message: string };
