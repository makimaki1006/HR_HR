"""Offline execution tests: no API keys or external service calls."""
import argparse
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import job_copy_airwork_pipeline as pipeline
import job_copy_airwork_images as collector
from test_job_copy_airwork_images import Session, Response, html, png, SITE, PAGE, FIRST


class PipelineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.job = {'accountKey': 'synthetic-account', 'recruitSiteUrl': SITE, 'mediaJobId': '123',
                    'hubspotListingId': '30', 'companyIds': ['10'], 'kyuujin_status': '公開中'}
        self.input = self.root / 'input.json'
        self.input.write_text(json.dumps({'schemaVersion': 1, 'jobs': [self.job]}))
        self.args = argparse.Namespace(input=self.input, recruit_cache=None, output=self.root / 'out',
            phase='capture', apply=False, storage_config=None, service_key=None, link_bin=None, env_file=None)

    def run_pipeline(self):
        with contextlib.redirect_stdout(io.StringIO()):
            return pipeline.run(self.args)

    def test_exact_public_status_and_count_no_unknown_zero(self):
        rows = [self.job, {**self.job, 'kyuujin_status': '公開終了'}, {**self.job, 'kyuujin_status': '不明'}]
        jobs, excluded = pipeline.select({'schemaVersion': 1, 'jobs': rows})
        self.assertEqual((len(jobs), excluded), (1, 2))
        self.assertNotIn('expectedImages', jobs[0])
        with self.assertRaisesRegex(collector.CaptureError, 'publication_status_required'):
            pipeline.select({'schemaVersion': 1, 'jobs': [{**self.job, 'kyuujin_status': None}]})

    def test_thousand_jobs_are_seventeen_existing_bounded_batches(self):
        jobs = [{**self.job, 'mediaJobId': str(1000 + i), 'hubspotListingId': str(2000 + i)} for i in range(1000)]
        self.input.write_text(json.dumps({'schemaVersion': 1, 'jobs': jobs}))
        result = self.run_pipeline()
        self.assertEqual((result['eligibleJobs'], result['batches']), (1000, 17))
        with self.assertRaisesRegex(collector.CaptureError, 'duplicate_listing'):
            pipeline.select({'schemaVersion': 1, 'jobs': jobs + [jobs[0]]})

    def test_dry_run_no_transport_no_output_no_receipts(self):
        with patch.object(pipeline, 'capture_jobs') as capture, patch.object(pipeline, 'archive') as archive, patch.object(pipeline.subprocess, 'run') as link:
            self.assertEqual(self.run_pipeline()['dryRun'], True)
            capture.assert_not_called(); archive.assert_not_called(); link.assert_not_called()
        self.assertFalse(self.args.output.exists())

    def test_capture_resume_preserves_observation_and_checks_bytes(self):
        self.args.apply = True
        session = Session({PAGE: Response(html()), FIRST: Response(png('red'), 'image/png')})
        capture = collector.capture_jobs
        with patch.object(pipeline, 'capture_jobs', side_effect=lambda payload, output: capture(payload, output, session)) as mock:
            self.assertEqual(self.run_pipeline()['completeJobs'], 1)
            self.assertEqual(self.run_pipeline()['completeJobs'], 1)
            self.assertEqual(mock.call_count, 1)
        self.assertEqual(len(session.calls), 2)
        evidence = json.loads((self.args.output / 'capture/0/archive-samples.json').read_bytes())
        Path(evidence['samples'][0]['source_path']).write_bytes(b'corrupt')
        with self.assertRaises(Exception): self.run_pipeline()

    def test_partial_is_retried_and_never_archived(self):
        self.args.apply = True
        session = Session({PAGE: Response(status=404)})
        capture = collector.capture_jobs
        with patch.object(pipeline, 'capture_jobs', side_effect=lambda payload, output: capture(payload, output, session)):
            self.assertEqual(self.run_pipeline()['unavailableJobs'], 1)
            self.assertEqual(self.run_pipeline()['unavailableJobs'], 1)
        self.assertEqual(len(session.calls), 2)
        self.assertEqual(json.loads((self.args.output / 'capture/0/archive-samples.json').read_bytes()), {'samples': []})

    def test_store_uses_existing_archive_and_link_retry_and_stops_on_failure(self):
        self.args.apply = True
        session = Session({PAGE: Response(html()), FIRST: Response(png('red'), 'image/png')})
        capture = collector.capture_jobs
        with patch.object(pipeline, 'capture_jobs', side_effect=lambda payload, output: capture(payload, output, session)):
            self.run_pipeline()
        self.args.phase = 'store'
        self.args.storage_config = self.root / 'storage.json'
        self.args.storage_config.write_text(json.dumps({'folderId': 'synthetic_folder', 'identityFolderId': 'synthetic_folder'}))
        self.args.link_bin = Path('/synthetic/link')
        self.args.service_key = Path('/synthetic/key')
        self.args.env_file = Path('/synthetic/env')
        with patch.object(pipeline, 'archive') as archive, patch.object(pipeline.subprocess, 'run') as link, patch.object(pipeline, 'capture_jobs') as capture:
            link.side_effect = [argparse.Namespace(returncode=0), argparse.Namespace(returncode=1)]
            with self.assertRaisesRegex(collector.CaptureError, 'pointer_sync_pending'): self.run_pipeline()
            link.side_effect = None
            link.return_value.returncode = 0
            self.assertEqual(self.run_pipeline()['completeJobs'], 1)
            self.assertEqual(archive.call_count, 2)
            self.assertEqual(archive.call_args.kwargs, {'upload': True})
            self.assertEqual(link.call_args.args[0][-1], '--apply')
            capture.assert_not_called()

    def test_changed_plan_rejected_before_get(self):
        self.args.apply = True
        self.args.output.mkdir()
        (self.args.output / 'plan.json').write_text('{"inputHash":"other"}')
        with patch.object(pipeline, 'capture_jobs') as capture:
            with self.assertRaisesRegex(collector.CaptureError, 'plan_changed'): self.run_pipeline()
            capture.assert_not_called()


    def test_missing_existing_property_stops_before_drive(self):
        self.args.apply = True
        self.args.phase = 'store'
        self.args.link_bin = Path('/synthetic/link')
        self.args.env_file = self.args.service_key = Path('/synthetic/private')
        with patch.object(pipeline.subprocess, 'run') as link, patch.object(pipeline, 'archive') as archive:
            link.return_value.returncode = 1
            with self.assertRaisesRegex(collector.CaptureError, 'existing_image_property_required'): self.run_pipeline()
            archive.assert_not_called()


if __name__ == '__main__': unittest.main()
