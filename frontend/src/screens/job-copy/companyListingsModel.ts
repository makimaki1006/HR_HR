import type { CompanyDeal } from './companyDeals';
import { dealInputs } from './dealSummaryLoad';
import type { DealData } from './dealSummaryLoad';
import { companyRollup, dealCutoff, listingNumbers } from './dealSummaryModel';
import type { Billing } from './DealSummary';
import { publicationLabel } from './hubspotListings';
import type { HubSpotListing } from './hubspotListings';
import { todayJst } from './timelineModel';

/** 求人は顧客内で1回。公開状況は kyuujin_status から正規化済みの索引の値を使う。 */
export function companyListingsModel(rows: readonly HubSpotListing[], datas: readonly DealData[], deals: readonly CompanyDeal[], billing: Billing) {
  const inputs = companyRollup(datas.map(data => ({ id: data.id, name: data.name, inputs: dealInputs(data, billing), overlap: data.overlap }))).inputs;
  const asOf = dealCutoff(inputs) ?? todayJst();
  const items = [...new Map(rows.map(row => [row.id, row])).values()].map(row => {
    const input = inputs.find(input => input.id === row.id);
    const changedAt = row.summary?.body_changed_at ?? datas.map(data => data.bodyDates[row.id]).find(Boolean) ?? null;
    return { row, changedAt, live: publicationLabel(row.publication_status) === '掲載中', numbers: input ? listingNumbers(input, asOf) : null,
      deals: deals.filter(deal => row.deal_ids?.includes(deal.id) === true || datas.some(data => data.id === deal.id && data.rows.some(item => item.id === row.id))) };
  }).sort((a, b) => (b.changedAt ?? '').localeCompare(a.changedAt ?? '') || (a.row.title ?? '').localeCompare(b.row.title ?? '', 'ja') || a.row.id.localeCompare(b.row.id));
  return { live: items.filter(item => item.live), past: items.filter(item => !item.live), asOf };
}
