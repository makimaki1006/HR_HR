"""実測の定義・関連の集め方・読み取り制限を架空データで検査する。"""
import json
from pathlib import Path
import unittest
from unittest.mock import patch
from measure_sentence_overlap import Reader, measure, overlap, samples, summary, publication


class MeasurementTests(unittest.TestCase):
    def test_same_sentence_contract_as_rust(self):
        fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/rikurozi_sentence_overlap.json"
        for case in json.loads(fixture.read_text()):
            with self.subTest(case=case):
                self.assertEqual(overlap(case["generated"], case["original"], case["minimum"]),
                                 (case["ratio"], case["whole"]))

    def test_publication_matches_verified_normalized_and_media_values(self):
        fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/rikurozi_publication.json"
        for case in json.loads(fixture.read_text()):
            normalized = case["normalized"] if case["air_only"] else None
            self.assertEqual(publication(case["raw"], normalized), case["expected"])

    def test_related_reads_are_cached_only_in_memory(self):
        reader = Reader("mock-token")
        calls = []
        def request(path, body):
            calls.append(body)
            return {"results": [{"from": {"id": item["id"]}, "to": [{"toObjectId": 8}]} for item in body["inputs"]]}
        reader.request = request
        first = reader.associations("0-420", ["1", "2"], "deals")
        second = reader.associations("0-420", ["2", "3"], "deals")
        self.assertEqual(first, {"1": {"8"}, "2": {"8"}})
        self.assertEqual(second, {"2": {"8"}, "3": {"8"}})
        self.assertEqual([len(call["inputs"]) for call in calls], [2, 1])

    def test_company_candidates_start_with_known_published_airwork(self):
        reader = Reader("mock-token")
        requests = []
        def request(path, body):
            requests.append(body)
            return {"results": [{"id": "1"}, {"id": "2"}]}
        reader.request = request
        reader.associations = lambda source, ids, target: ({"1": {"deal1"}, "2": {"deal2"}} if source == "0-420" else {"deal1": {"company1"}, "deal2": {"company2"}})
        self.assertEqual(reader.companies(2), ["company1", "company2"])
        filters = requests[0]["filterGroups"][0]["filters"]
        self.assertIn({"propertyName": "kyuujin_status", "operator": "EQ", "value": "公開中"}, filters)
        self.assertIn({"propertyName": "id_airwork", "operator": "HAS_PROPERTY"}, filters)
        self.assertIn({"propertyName": "id_hrhakkaa", "operator": "NOT_HAS_PROPERTY"}, filters)

    def test_percentiles_and_tenth_bins_preserve_unavailable(self):
        result = summary([0.0, .2, .5, 1.0], 1, 2)
        self.assertAlmostEqual(result["p10"], .06)
        self.assertAlmostEqual(result["p50"], .35)
        self.assertAlmostEqual(result["p95"], .925)
        self.assertEqual(result["histogram"], [1, 0, 1, 0, 0, 1, 0, 0, 0, 1])
        self.assertEqual(result["unmeasurable_count"], 2)
        self.assertIsNone(summary([], 0, 1)["p50"])

    def test_same_company_all_pairs_and_only_known_published_airwork(self):
        class MockReader:
            def companies(self, maximum):
                return ["company1", "company2"]
            def associations(self, source, ids, target):
                if source == "companies":
                    return {"company1": {"deal1", "deal2"}, "company2": {"deal3"}}
                return {"deal1": {"hrh", "air1"}, "deal2": {"air1", "air2", "unknown", "closed"}, "deal3": {"other"}}
            def batch(self, ids):
                return {
                    "hrh": {"id_hrhakkaa": "mock", "hrh_kyuujinhyou_honbun": "共通の文です。基準の文です。"},
                    "air1": {"id_airwork": "mock", "shigotonaiyou": "共通の文です。相手の文です。", "baitai_genjoukyou_airwork": "公開"},
                    "air2": {"id_airwork": "mock", "shigotonaiyou": "別の求人です。", "baitai_genjoukyou_airwork": "掲載中"},
                    "unknown": {"id_airwork": "mock", "shigotonaiyou": "保存禁止の本文"},
                    "closed": {"id_airwork": "mock", "shigotonaiyou": "保存禁止の本文", "baitai_genjoukyou_airwork": "公開終了"},
                    "other": {"id_airwork": "mock", "shigotonaiyou": "保存禁止の別会社", "baitai_genjoukyou_airwork": "公開"},
                }
        result = measure(MockReader(), 200)
        self.assertEqual(result["company_count"], 1)
        self.assertEqual(result["hrh_job_count"], 1)
        self.assertEqual(result["airwork_job_count"], 2)
        self.assertEqual(result["measurements"]["hrh_airwork_0"]["count"], 2)
        self.assertEqual(result["measurements"]["airwork_airwork_0"]["count"], 2)
        self.assertNotIn("保存禁止", json.dumps(result, ensure_ascii=False))

    def test_missing_body_is_unmeasurable_instead_of_zero(self):
        hrh, air = samples([{"id_hrhakkaa": "mock"}, {"id_airwork": "mock", "baitai_genjoukyou_airwork": "掲載中"}])
        self.assertEqual(hrh, [""])
        self.assertEqual(air, [""])
        self.assertEqual(overlap(hrh[0], air[0]), (None, False))

    def test_batch_uses_100_record_chunks_and_minimal_properties(self):
        reader = Reader("mock-token")
        calls = []
        def request(path, body):
            calls.append((path, body))
            return {"results": [{"id": item["id"], "properties": {}} for item in body["inputs"]]}
        reader.request = request
        self.assertEqual(len(reader.batch(map(str, range(201)))), 201)
        self.assertEqual([len(body["inputs"]) for path, body in calls], [100, 100, 1])
        self.assertTrue(all(path.endswith("/batch/read") for path, _ in calls))
        self.assertTrue(all("hs_name" not in body["properties"] for _, body in calls))

    def test_missing_batch_association_is_verified_individually(self):
        reader = Reader("mock-token")
        reader.request = lambda path, body: {"results": [{"from": {"id": "1"}, "to": [{"toObjectId": 8}]}]}
        reader.all_associations = lambda source, key, target: {"9"} if key == "2" else set()
        self.assertEqual(reader.associations("0-420", ["1", "2"], "deals"), {"1": {"8"}, "2": {"9"}})

    def test_partial_body_read_stops_measurement(self):
        reader = Reader("mock-token")
        reader.request = lambda path, body: {"results": []}
        with self.assertRaises(RuntimeError):
            reader.batch(["1"])

    def test_writes_are_rejected_before_network(self):
        reader = Reader("mock-token")
        with patch("urllib.request.urlopen") as network:
            for path in ["/crm/v3/objects/0-420", "/crm/v3/objects/0-420/batch/update", "/crm/v3/objects/0-420/batch/create"]:
                with self.assertRaises(RuntimeError):
                    reader.request(path, {"inputs": []})
            network.assert_not_called()

    def test_request_pacing_also_limits_search_to_one_per_second(self):
        reader = Reader("mock-token")
        reader.last = 10.0
        with patch("time.monotonic", return_value=10.5), patch("time.sleep") as sleep, patch("urllib.request.urlopen") as network:
            network.return_value.__enter__.return_value.read.return_value = b'{"results": []}'
            reader.request("/crm/v3/objects/0-420/search", {})
            self.assertAlmostEqual(sleep.call_args.args[0], .51)


if __name__ == "__main__":
    unittest.main()
