// `GET /api/sales-kpi/data` の JSON の型。
//
// 正本は Rust (src/handlers/sales_kpi/payload.rs) で、ts-rs が `frontend/src/generated/SalesKpi*.ts`
// に書き出したものを、ここで画面向けの短い名前に付け替えて再エクスポートする。
// 画面側は必ずこのファイル経由で型を引く (generated を直接 import しない)。
// Rust 側のフィールドを変えると生成物が変わり、`npm run typecheck` が落ちる (CI の contract-types)。
import type { SalesKpiCallPeriods } from '../../generated/SalesKpiCallPeriods';
import type { SalesKpiKetteiCells } from '../../generated/SalesKpiKetteiCells';
import type { SalesKpiKetteiRow } from '../../generated/SalesKpiKetteiRow';

export type { SalesKpiData } from '../../generated/SalesKpiData';
export type { SalesKpiPerson as Person } from '../../generated/SalesKpiPerson';
export type { SalesKpiDateSpan as DateSpan } from '../../generated/SalesKpiDateSpan';
export type { SalesKpiDealRow as DealRow } from '../../generated/SalesKpiDealRow';
export type { SalesKpiKadenPeriod as KadenPeriod } from '../../generated/SalesKpiKadenPeriod';
export type { SalesKpiCallsDaily as CallsDaily } from '../../generated/SalesKpiCallsDaily';
export type { SalesKpiCalls as Calls } from '../../generated/SalesKpiCalls';
export type { SalesKpiKadenComposition as KadenComposition } from '../../generated/SalesKpiKadenComposition';
export type { SalesKpiUnassignedPerson as UnassignedPerson } from '../../generated/SalesKpiUnassignedPerson';
export type { SalesKpiKadenBaseTrend as KadenBaseTrend } from '../../generated/SalesKpiKadenBaseTrend';
export type { SalesKpiKaden as Kaden } from '../../generated/SalesKpiKaden';
export type { SalesKpiKettei as Kettei } from '../../generated/SalesKpiKettei';
export type { SalesKpiStockGroup as StockGroup } from '../../generated/SalesKpiStockGroup';
export type { SalesKpiStockList as StockList } from '../../generated/SalesKpiStockList';
export type { SalesKpiStockTrendList as StockTrendList } from '../../generated/SalesKpiStockTrendList';
export type { SalesKpiStockTrend as StockTrend } from '../../generated/SalesKpiStockTrend';
export type { SalesKpiListStock as ListStock } from '../../generated/SalesKpiListStock';
export type { SalesKpiSnapshot as Snapshot } from '../../generated/SalesKpiSnapshot';
export type { SalesKpiSnapshotTotals as SnapshotTotals } from '../../generated/SalesKpiSnapshotTotals';
export type { SalesKpiSnapshotWeekTotals as SnapshotWeekTotals } from '../../generated/SalesKpiSnapshotWeekTotals';

/** 数え上げ。キーは画面がそのまま出す日本語ラベルや `apo` `pool` 等 (Rust `Counts`)。 */
export type Counts = Record<string, number>;

/** 架電の期間。Rust `CallPeriods` のフィールド名がそのままキー。 */
export type CallPeriodKey = keyof SalesKpiCallPeriods;

/** 決定者・決裁者の 1 行 (担当者あり)。列の値は `ketteiCell()` で引く。 */
export type KetteiRow = SalesKpiKetteiRow;
/** 担当者が入っていない取引ぶん。人ではないので owner 等は無い。 */
export type KetteiNoOwner = SalesKpiKetteiCells;
