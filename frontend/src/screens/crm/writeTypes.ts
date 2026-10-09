// CRM 書き込み API の型。正本はバックエンドが ts-rs で書き出す src/generated/ (src/crm/write.rs)。
// ここは生成された型を、画面が分岐しやすいよう文字列のリテラル型へ狭めて再公開するだけで、
// 項目の増減・改名は生成型との交差 / Omit を通るので、バックエンドの変更でコンパイルが落ちる。
import type { CrmAdminOperation } from '../../generated/CrmAdminOperation';
import type { CrmAdminOperationsResponse } from '../../generated/CrmAdminOperationsResponse';
import type { CrmDealPatchRequest } from '../../generated/CrmDealPatchRequest';
import type { CrmEditPipeline } from '../../generated/CrmEditPipeline';
import type { CrmEditSchemaResponse } from '../../generated/CrmEditSchemaResponse';
import type { CrmEditStage } from '../../generated/CrmEditStage';
import type { CrmEditableProperty } from '../../generated/CrmEditableProperty';
import type { CrmOperationStatus } from '../../generated/CrmOperationStatus';
import type { CrmPatchConflict } from '../../generated/CrmPatchConflict';
import type { CrmPatchInvalid } from '../../generated/CrmPatchInvalid';
import type { CrmPatchObject } from '../../generated/CrmPatchObject';
import type { CrmPatchPartial } from '../../generated/CrmPatchPartial';
import type { CrmPatchQueued } from '../../generated/CrmPatchQueued';
import type { CrmPatchSaved } from '../../generated/CrmPatchSaved';
import type { CrmPropertyOption } from '../../generated/CrmPropertyOption';

export type WriteObject = 'deal' | 'contact' | 'company';

/** 選択肢 (value と label。hidden は画面で使わない) */
export type EditOption = Pick<CrmPropertyOption, 'value' | 'label'>;

/** 書き換えてよい項目 1 件 */
export type EditableProp = Omit<CrmEditableProperty, 'object' | 'options' | 'max_length'> & {
  object: WriteObject;
  options: EditOption[];
  max_length?: CrmEditableProperty['max_length'];
};

/** ステージごとの規則: 移すときに画面へ出す項目 (shown) と、空にできない項目 (required) */
export type StageRule = CrmEditStage;
export type PipelineChoice = CrmEditPipeline;

/**
 * GET /api/crm/edit-schema?deal_id=
 * バックエンドは書ける項目の全部を `editable` で返す。`editable_all` / `read_only` は架空サンプル (fakeWrite.ts) だけが使う
 */
export type EditSchema = Omit<CrmEditSchemaResponse, 'editable'> & {
  editable: EditableProp[];
  editable_all?: boolean;
  read_only?: string[];
};

export type PropValues = CrmPatchObject['base'];

export type ObjectPatch = CrmPatchObject;

export type PatchRequest = Omit<CrmDealPatchRequest, 'stage' | 'objects'> & {
  stage?: NonNullable<CrmDealPatchRequest['stage']>;
  objects?: { contact?: CrmPatchObject; company?: CrmPatchObject };
};

/** 200: 案件の保存後の値 (values) と、担当者・会社を同時に変えたときの値 (objects_values) */
export type PatchSaved = Omit<CrmPatchSaved, 'status'> & { status: 'saved' };
export type PatchQueued = Omit<CrmPatchQueued, 'status'> & { status: 'queued' };
/** 409: どのオブジェクトの現在値が違ったか (object) を持つ */
export type PatchConflict = Omit<CrmPatchConflict, 'status' | 'object'> & { status: 'conflict'; object: WriteObject };
export type PatchInvalid = Omit<CrmPatchInvalid, 'status'> & { status: 'invalid' };
/** 複数オブジェクトの保存が途中で止まったとき、すでに HubSpot に書けた分 */
export type PatchPartial = CrmPatchPartial;
/** 2xx で返るもの (200 saved / 202 queued) */
export type PatchResponse = PatchSaved | PatchQueued;
/** 4xx で返るもの (409 / 422) */
export type PatchFailure = PatchConflict | PatchInvalid;

export type OperationState = 'pending' | 'retrying' | 'saved' | 'failed';
export type OperationStatus = Omit<CrmOperationStatus, 'status'> & { status: OperationState };

export type { CrmAdminOperation, CrmAdminOperationsResponse };
