// CRM 書き込み API の型 (契約: crm_write_contract.md)。
// バックエンドが ts-rs で src/generated/ に同じ型を出したら、そちらへ差し替える (名前は合わせてある)。

export type WriteObject = 'deal' | 'contact' | 'company';

export interface EditOption { value: string; label: string }

/** 書き換えてよい項目 1 件 */
export interface EditableProp {
  object: WriteObject;
  /** HubSpot の内部名 (画面には出さない) */
  name: string;
  label: string;
  /** HubSpot の type (string / number / date / datetime / enumeration / bool) */
  type: string;
  /** HubSpot の fieldType (text / textarea / select / radio / checkbox / booleancheckbox / phonenumber / date ...) */
  field_type: string;
  options: EditOption[];
  max_length?: number | null;
}

/** ステージごとの規則: 移すときに画面へ出す項目 (shown) と、空にできない項目 (required) */
export interface StageRule {
  id: string;
  label: string;
  required: string[];
  shown: string[];
}

export interface PipelineChoice { id: string; label: string; stages: { id: string; label: string }[] }

/**
 * GET /api/crm/edit-schema?deal_id=
 * 書ける項目の全部を `editable` で返す形のほか、`editable_all: true` と `read_only` (書けない項目の内部名) で返す形もある。両方に対応する
 */
export interface EditSchema {
  deal_id: string;
  pipeline_id: string | null;
  stage_id: string | null;
  editable: EditableProp[];
  editable_all?: boolean;
  read_only?: string[];
  stages: StageRule[];
  pipelines: PipelineChoice[];
  writes_enabled: boolean;
}

export type PropValues = Record<string, string | null>;

export interface ObjectPatch { id: string; base: PropValues; set: PropValues }

export interface PatchRequest {
  operation_id: string;
  base: PropValues;
  set: PropValues;
  stage?: { pipeline_id: string; stage_id: string };
  objects?: { contact?: ObjectPatch; company?: ObjectPatch };
}

export interface PatchSaved { status: 'saved'; values: PropValues; fetched_at: string }
export interface PatchQueued { status: 'queued'; operation_id: string }
export interface PatchConflict { status: 'conflict'; current: PropValues; changed_by_hubspot: string[] }
export interface PatchInvalid { status: 'invalid'; errors: Record<string, string>; missing_required: string[] }
/** 2xx で返るもの (200 saved / 202 queued) */
export type PatchResponse = PatchSaved | PatchQueued;
/** 4xx で返るもの (409 / 422) */
export type PatchFailure = PatchConflict | PatchInvalid;

export type OperationState = 'pending' | 'retrying' | 'saved' | 'failed';
export interface OperationStatus {
  operation_id: string;
  status: OperationState;
  attempts: number;
  last_error_code?: string | null;
  next_retry_at?: string | null;
}
