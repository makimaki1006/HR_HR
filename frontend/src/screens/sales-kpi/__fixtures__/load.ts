import type { SalesKpiData } from '../types';
import { FIXTURE_PAYLOAD_TEXT } from './payload_2026-09-04';
import { FIXTURE_NEGTYPE_PAYLOAD_TEXT } from './payload_negtype_2026-09-04';

/** fixture の JSON を毎回新しいオブジェクトで返す (テスト間で書き換えが漏れないように)。 */
export function loadFixture(): SalesKpiData {
  return JSON.parse(FIXTURE_PAYLOAD_TEXT) as SalesKpiData;
}

/** 商談種別の列がある fixture (`negotiation_type_available: true`)。 */
export function loadNegtypeFixture(): SalesKpiData {
  return JSON.parse(FIXTURE_NEGTYPE_PAYLOAD_TEXT) as SalesKpiData;
}
