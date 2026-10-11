"""定義の作成は冪等、衝突時は作成前に停止。求人レコードは触らない。"""
import importlib.util
import io
import json
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('definitions', ROOT/'scripts/ensure_jobgen_draft_properties.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
PROPERTIES = json.loads((ROOT/'src/job_gen/draft_properties.json').read_text())

class Definitions(unittest.TestCase):
    def run_main(self, existing=(), archived=(), apply=True):
        calls=[]
        def request(key, method, suffix='', body=None):
            calls.append((method,suffix,body))
            if method=='POST':return body
            if suffix.startswith('/groups/'):return {'name':'tab_p_1_kyuujinjouhou'}
            return {'results':list(archived if suffix else existing)}
        with patch.object(module,'request',side_effect=request), patch.object(module,'token',return_value='mock-token'), patch('sys.argv',['definitions']+(['--apply'] if apply else [])), patch('sys.stdout',io.StringIO()):
            module.main()
        return calls
    def test_only_missing_five_definitions_are_created(self):
        calls=self.run_main()
        creates=[body for method,_,body in calls if method=='POST']
        self.assertEqual([p['name'] for p in creates],[p['name'] for p in PROPERTIES])
        self.assertTrue(all(p['groupName']=='tab_p_1_kyuujinjouhou' and p['description']=='求人票作成の画面が書く。手で編集しない' for p in creates))
        self.assertEqual(creates[2]['options'][0]['label'],'確認待ち')
        self.assertEqual(creates[3]['options'][2]['value'],'excel')
    def test_matching_existing_and_dry_run_never_post(self):
        for calls in [self.run_main(existing=PROPERTIES),self.run_main(apply=False)]:
            self.assertTrue(all(method=='GET' for method,_,_ in calls))
    def test_collision_or_archived_name_stops_before_any_creation(self):
        wrong={**PROPERTIES[-1],'type':'number'}
        for existing,archived in [([wrong],[]),([],[PROPERTIES[-1]])]:
            calls=[]
            def request(key,method,suffix='',body=None):
                calls.append(method)
                if suffix.startswith('/groups/'):return {'name':'tab_p_1_kyuujinjouhou'}
                return {'results':archived if suffix else existing}
            with patch.object(module,'request',side_effect=request),patch.object(module,'token',return_value='mock'),patch('sys.argv',['definitions','--apply']):
                with self.assertRaises(RuntimeError):module.main()
            self.assertNotIn('POST',calls)
    def test_the_api_is_metadata_only(self):
        self.assertEqual(module.API,'https://api.hubapi.com/crm/v3/properties/0-420')

if __name__=='__main__':unittest.main()
