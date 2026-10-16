import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import type { EditSchema } from '../crm/writeTypes';
import { CONTRACT_FIELDS, DISPLAY_FIELDS } from './dealContractModel';

const labels = ['契約プラン', '契約種別', '契約開始日', '契約満了日', '契約期間', '金額', '商品値引額', '応募数', '面接数', '内定数', '採用数', '売上額', '分割売上額'];
export const contractProps: CrmCatalogProperty[] = DISPLAY_FIELDS.map((name, index) => ({ name, label: labels[index] ?? name,
  property_type: index < 2 ? 'enumeration' : index < 4 ? 'date' : 'number', field_type: index < 2 ? 'select' : index < 4 ? 'date' : 'number', owner_ref: false,
  options: index < 2 ? [{ value: 'basic', label: '合成・基本', hidden: false }, { value: 'premium', label: '合成・充実', hidden: false }] : [],
}));
export const contractSchema: EditSchema = { deal_id: '901', pipeline_id: '21596025', stage_id: 'stage-demo', writes_enabled: true, stages: [], pipelines: [], editable: contractProps.filter(prop => CONTRACT_FIELDS.some(name => name === prop.name)).map(prop => ({ ...prop, object: 'deal', type: prop.property_type })) };
export const contractValues = { contract_plan: 'basic', contract_type: 'basic', contract_start_date: '2026-10-01T00:00:00Z', contract_expiration_date: '2027-09-30', contract_period: '12', amount: '240000', discount_amount: '10000', oubo: '20', mensetu: '8', naite: '3', syoudaku: '2', sales_amount: '230000', split_sales_amount: '19166.67' };
export const contractCatalog = { objects: [{ object_type: 'deals', groups: [{ name: 'contract', label: '契約', properties: contractProps }] }], max_selected_per_object: 50, fetched_at: '2026-10-16T00:00:00Z', cache_hit: false };
export function contractDetail(values = contractValues, pipeline = '21596025') {
  return { deal: { id: '901', pipeline_id: pipeline, stage_label: '合成・契約中', deep_link: '#synthetic-deal' }, selected: { deal: values, contact: {}, company: {} }, partial: [], fetched_at: '2026-10-16T00:00:00Z' };
}
