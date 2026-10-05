import copy
import unittest
from job_copy_attach_hrh_performance import attach


class PerformanceTests(unittest.TestCase):
    def setUp(self):
        self.snapshot = {'schemaVersion': 1, 'capture_bundle': {'jobs': [
            {'media': 'HRハッカー', 'mediaJobId': '01234567', 'hubspotListingId': '30'}]},
            'results': [{'listing_id': '30'}]}
        self.metrics = {'schema_version': 1, 'source': 'hrhacker', 'job_id': '01234567',
            'captured_at': '2026-10-05T01:00:00Z', 'rows': [
                {'period_start': '2026-09-01', 'period_end': '2026-09-10',
                 'impressions': 1000, 'clicks': 50, 'cost_yen': 10000, 'applications': 5}]}

    def test_exact_join_preserves_original_and_leading_zero(self):
        before = copy.deepcopy(self.snapshot)
        result = attach(self.snapshot, [self.metrics])
        self.assertEqual(result['results'][0]['hrh_performance']['job_id'], '01234567')
        self.assertEqual(self.snapshot, before)

    def test_no_cross_company_or_media_or_numeric_coercion(self):
        for bad_id in ['1234567', 1234567, '87654321']:
            with self.assertRaises(ValueError):
                attach(self.snapshot, [{**self.metrics, 'job_id': bad_id}])
        self.snapshot['capture_bundle']['jobs'].append(copy.deepcopy(self.snapshot['capture_bundle']['jobs'][0]))
        with self.assertRaises(ValueError):
            attach(self.snapshot, [self.metrics])
        self.snapshot['capture_bundle']['jobs'] = [{**self.snapshot['capture_bundle']['jobs'][0], 'media': 'Airワーク'}]
        with self.assertRaises(ValueError):
            attach(self.snapshot, [self.metrics])

    def test_period_and_metric_reverse_proof(self):
        original = self.metrics['rows'][0]
        for delta in [{'period_start': '2026-02-30'}, {'clicks': 1001}, {'cost_yen': -1},
                      {'applications': True}, {'clicks': 1.5}, {'cost_yen': float('nan')}, {'email': 'synthetic'}]:
            with self.assertRaises(ValueError):
                attach(self.snapshot, [{**self.metrics, 'rows': [{**original, **delta}]}])
        with self.assertRaises(ValueError):
            attach(self.snapshot, [{**self.metrics, 'rows': [original, original]}])

    def test_missing_is_null_not_zero_and_repeat_is_rejected(self):
        data = {**self.metrics, 'rows': [{**self.metrics['rows'][0], 'impressions': None, 'cost_yen': None}]}
        result = attach(self.snapshot, [data])
        self.assertIsNone(result['results'][0]['hrh_performance']['rows'][0]['cost_yen'])
        with self.assertRaises(ValueError):
            attach(result, [data])
        with self.assertRaises(ValueError):
            attach(self.snapshot, [data, data])


if __name__ == '__main__':
    unittest.main()
