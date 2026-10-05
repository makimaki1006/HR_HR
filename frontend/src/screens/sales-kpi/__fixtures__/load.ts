import type { SalesKpiData } from '../types';
import { FIXTURE_PAYLOAD_TEXT } from './payload_2026-09-04';
import { FIXTURE_ATTR_PAYLOAD_TEXT } from './payload_attr_2026-09-04';

/** fixture の JSON を毎回新しいオブジェクトで返す (テスト間で書き換えが漏れないように)。 */
export function loadFixture(): SalesKpiData {
  return JSON.parse(FIXTURE_PAYLOAD_TEXT) as SalesKpiData;
}

/** 商談属性の列がある fixture (`deal_attr_available: true`)。 */
export function loadAttrFixture(): SalesKpiData {
  return JSON.parse(FIXTURE_ATTR_PAYLOAD_TEXT) as SalesKpiData;
}
