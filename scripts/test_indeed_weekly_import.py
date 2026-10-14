"""Synthetic checks for the Indeed weekly importer. No network; fixtures are made up."""
import csv
import datetime
import json
import tempfile
import unittest
from pathlib import Path

import indeed_weekly_import as imp

HEADER = ['求人', 'キャンペーン', '参照番号', '表示回数', 'クリック数', '応募数', '費用']


def write_week(directory, start, end, rows):
    with open(Path(directory) / f'JobsCampaigns_{start}_{end}.csv', 'w', encoding='utf-8-sig', newline='') as handle:
        writer = csv.writer(handle)
        writer.writerow(HEADER)
        for row in rows:
            writer.writerow(row)


def row(start, cost, apps=0):
    end = (datetime.date.fromisoformat(start) + datetime.timedelta(days=6)).isoformat()
    return {'period_start': start, 'period_end': end, 'impressions': 10, 'clicks': 1, 'cost_yen': cost, 'applications': apps}


class Parsing(unittest.TestCase):
    def test_numbers(self):
        self.assertIsNone(imp.parse_number('-'))
        self.assertIsNone(imp.parse_number(''))
        self.assertEqual(imp.parse_number('¥1,234'), 1234)
        self.assertEqual(imp.parse_number('12'), 12)
        self.assertEqual(imp.parse_number('0'), 0)

    def test_sum_across_campaigns_and_filter(self):
        with tempfile.TemporaryDirectory() as d:
            write_week(d, '20260105', '20260111', [
                ['a', 'c1', '1234567', '10', '1', '-', '¥1,000'],
                ['a', 'c2', '1234567', '5', '-', '1', '¥500'],
                ['b', 'c1', '7654321', '0', '0', '0', '¥0'],
                ['c', 'c1', '9999999', '-', '-', '-', '-'],
            ])
            weeks, files = imp.load_dir(d)
        self.assertEqual(files, 1)
        self.assertEqual(sorted(weeks), ['1234567'])
        self.assertEqual(weeks['1234567']['2026-01-05'], {'period_start': '2026-01-05', 'period_end': '2026-01-11',
                                                          'impressions': 15, 'clicks': 1, 'cost_yen': 1500, 'applications': 1})


class MergeAndSummary(unittest.TestCase):
    def test_merge_replaces_same_week_and_sorts(self):
        existing = {'rows': [row('2026-01-12', 100), row('2026-01-05', 50)]}
        merged = imp.merge_rows(existing, [row('2026-01-12', 999), row('2026-01-19', 7)])
        self.assertEqual([(r['period_start'], r['cost_yen']) for r in merged],
                         [('2026-01-05', 50), ('2026-01-12', 999), ('2026-01-19', 7)])

    def test_last4_weeks_end_at_latest_week(self):
        rows = [row('2026-01-05', 100, 1), row('2026-01-12', 200, 1), row('2026-01-19', 300, 0),
                row('2026-01-26', 400, 2), row('2026-02-02', 500, 0)]
        result = imp.summarize(rows)
        self.assertEqual(result['indeed_last_week_start'], '2026-02-02')
        self.assertEqual(result['indeed_cost_last_week'], 500)
        self.assertEqual(result['indeed_cost_last4w'], 200 + 300 + 400 + 500)
        self.assertEqual(result['indeed_applications_last4w'], 3)
        self.assertEqual(result['indeed_cpa_last4w'], round(1400 / 3))
        self.assertEqual(result['indeed_cost_total'], 1500)

    def test_gap_weeks_count_as_zero(self):
        result = imp.summarize([row('2026-01-05', 100, 1), row('2026-03-02', 40, 1)])
        self.assertEqual(result['indeed_cost_last4w'], 40)
        self.assertEqual(result['indeed_cost_total'], 140)

    def test_cpa_empty_when_no_applications(self):
        self.assertEqual(imp.summarize([row('2026-01-05', 100, 0)])['indeed_cpa_last4w'], '')

    def test_plan_skips_when_unchanged_and_keeps_captured_at(self):
        wanted, _ = imp.plan_listing('55', '1234567', {}, [row('2026-01-05', 100, 1)], '2026-01-20T00:00:00Z')
        self.assertIn('listing=55', wanted['jobcopy_app_url'])
        again, _ = imp.plan_listing('55', '1234567', dict(wanted), [row('2026-01-05', 100, 1)], '2026-02-20T00:00:00Z')
        self.assertIsNone(again)
        changed, _ = imp.plan_listing('55', '1234567', dict(wanted), [row('2026-01-12', 30, 0)], '2026-02-20T00:00:00Z')
        data = json.loads(changed[imp.PERF])
        self.assertEqual([r['period_start'] for r in data['rows']], ['2026-01-05', '2026-01-12'])
        self.assertEqual(data['job_id'], '1234567')

    def test_broken_existing_json_is_rebuilt(self):
        wanted, rows = imp.plan_listing('9', '1234567', {imp.PERF: 'not json'}, [row('2026-01-05', 10)], '2026-01-20T00:00:00Z')
        self.assertEqual(len(rows), 1)
        self.assertIsNotNone(wanted)

    def test_norm_dates_and_numbers(self):
        ms = int(datetime.datetime(2026, 1, 5, tzinfo=datetime.timezone.utc).timestamp() * 1000)
        self.assertEqual(imp.norm(str(ms)), '2026-01-05')
        self.assertEqual(imp.norm('2026-01-05T00:00:00Z'), '2026-01-05')
        self.assertEqual(imp.norm('1500.0'), '1500')


class FakeHubSpot:
    def __init__(self, records):
        self.records, self.updates = records, []

    def find_by_refs(self, refs):
        return [r for r in self.records if r['properties']['id_hrhakkaa'] in refs]

    def batch_update(self, inputs):
        self.updates.extend(inputs)
        return True


class Run(unittest.TestCase):
    def test_counts_and_limit(self):
        with tempfile.TemporaryDirectory() as d:
            write_week(d, '20260105', '20260111', [['a', 'c', '1111111', '10', '1', '0', '¥100'],
                                                   ['a', 'c', '2222222', '10', '1', '1', '¥200'],
                                                   ['a', 'c', '3333333', '10', '1', '1', '¥300']])
            hub = FakeHubSpot([{'id': '1', 'properties': {'id_hrhakkaa': '1111111'}},
                               {'id': '2', 'properties': {'id_hrhakkaa': '2222222'}}])
            dry = imp.run(hub, d)
            self.assertEqual((dry['refs'], dry['matched'], dry['unmatched'], dry['to_write'], dry['written']), (3, 2, 1, 2, 0))
            self.assertEqual(hub.updates, [])
            applied = imp.run(hub, d, apply=True, limit=1)
            self.assertEqual((applied['to_write'], applied['written']), (1, 1))
            self.assertEqual(hub.updates[0]['id'], '1')


if __name__ == '__main__':
    unittest.main()
