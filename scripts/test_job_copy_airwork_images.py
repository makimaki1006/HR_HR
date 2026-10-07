"""Synthetic offline collector checks. No external GET, Drive or HubSpot writes."""
import copy
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from PIL import Image
import requests

import job_copy_airwork_images as collector
from job_copy_drive_manifest import build_manifests

SITE = 'https://arwrk.net/recruit/synthetic-account'
PAGE = SITE + '/123/'
FIRST = 'https://cdn.arwrk.net/images/rct/synthetic-first.png'
SECOND = 'https://cdn.arwrk.net/images/rct/synthetic-second.png'


def png(color):
    stream = io.BytesIO()
    Image.new('RGB', (2, 3), color).save(stream, format='PNG')
    return stream.getvalue()


def html(images=(FIRST,), canonical=PAGE, extra=''):
    return (f'<html><head><link rel="canonical" href="{canonical}"></head><body>'
            f'<img src="https://example.invalid/logo.png"><div class="job-img">'
            f'<ul class="slider">' + ''.join(f'<li><img src="{url}"></li>' for url in images)
            + extra + f'</ul><div class="thumbnails"><img src="{SECOND}"></div></div></body></html>').encode()


class Response:
    def __init__(self, raw=b'', mime='text/html', status=200):
        self.raw, self.status_code = raw, status
        self.headers = {'Content-Type': mime}
        self.closed = False

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        self.closed = True

    def iter_content(self, size):
        for start in range(0, len(self.raw), size):
            yield self.raw[start:start + size]


class Session:
    def __init__(self, answers):
        self.answers = answers
        self.calls = []
        self.trust_env = True

    def get(self, url, **options):
        self.calls.append((url, options))
        if url not in self.answers:
            raise AssertionError('Unexpected synthetic URL; no external request allowed')
        answer = self.answers[url]
        if isinstance(answer, Exception):
            raise answer
        return answer


class AirWorkImagesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name)
        self.raw = png('red')
        self.job = {'accountKey': 'synthetic-account', 'recruitSiteUrl': SITE,
                    'mediaJobId': '123', 'hubspotListingId': '30', 'companyIds': ['10'], 'expectedImages': 1}

    def capture(self, job=None, answers=None):
        session = Session(answers or {PAGE: Response(html()), FIRST: Response(self.raw, 'image/png')})
        result = collector.capture_jobs({'schemaVersion': 1, 'jobs': [job or self.job]}, self.output, session=session)
        return result, session

    def test_only_gallery_originals_not_logos_thumbnails_or_clones(self):
        clone = f'<li class="is-clone"><img src="{SECOND}"></li>'
        self.assertEqual(collector.parse_gallery(html(extra=clone), PAGE), [FIRST])
        self.assertEqual(collector.job_url(SITE + '/', '123'), PAGE)

    def test_slider_must_be_a_descendant_not_ancestor_or_same_element(self):
        reversed_layout = html().replace(b'<div class="job-img"><ul class="slider">', b'<div class="slider"><ul class="job-img">')
        same_element = html().replace(b'<div class="job-img"><ul class="slider">', b'<div class="job-img slider"><ul>')
        for raw in (reversed_layout, same_element):
            with self.assertRaisesRegex(collector.CaptureError, 'gallery_layout_unrecognized'):
                collector.parse_gallery(raw, PAGE)

    def test_canonical_mismatch_duplicate_missing_and_unrecognized_gallery_fail(self):
        cases = [html(canonical=SITE + '/456/'), html().replace(b'<link ', b'<meta '),
                 html().replace(b'</head>', f'<link rel="canonical" href="{PAGE}"></head>'.encode()),
                 html().replace(b'class="slider"', b'class="other"'), b'\xff']
        for raw in cases:
            with self.subTest(case=cases.index(raw)), self.assertRaises(collector.CaptureError):
                collector.parse_gallery(raw, PAGE)

    def test_strict_source_urls_prevent_foreign_hosts_ports_credentials_and_traversal(self):
        for site in ['http://arwrk.net/recruit/synthetic', 'https://evil.invalid/recruit/synthetic',
                     'https://arwrk.net:443/recruit/synthetic', 'https://user@arwrk.net/recruit/synthetic',
                     SITE + '?next=x', SITE + '#fragment', SITE + '/../other']:
            with self.subTest(site=site), self.assertRaises(collector.CaptureError):
                collector.job_url(site, '123')
        for url in ['http://cdn.arwrk.net/images/rct/a.png', 'https://127.0.0.1/images/rct/a.png',
                    'https://cdn.arwrk.net.evil.invalid/images/rct/a.png', 'https://cdn.arwrk.net:443/images/rct/a.png',
                    'https://user@cdn.arwrk.net/images/rct/a.png', 'https://cdn.arwrk.net/images/rct/../a.png',
                    'https://cdn.arwrk.net/images/rct/%2e%2e/a.png', FIRST + '#fragment', FIRST + '\\x']:
            with self.subTest(url=url), self.assertRaises(collector.CaptureError):
                collector.parse_gallery(html(images=[url]), PAGE)

    def test_all_job_preflight_happens_before_any_get(self):
        session = Session({})
        invalid = {**self.job, 'mediaJobId': '../123'}
        with self.assertRaises(collector.CaptureError):
            collector.capture_jobs({'schemaVersion': 1, 'jobs': [self.job, invalid]}, self.output, session)
        self.assertEqual(session.calls, [])

    def test_account_site_job_listing_and_company_conflicts_fail_preflight(self):
        pairs = [self.job, {**self.job, 'accountKey': 'other-account', 'mediaJobId': '124', 'hubspotListingId': '31'},
                 {**self.job, 'recruitSiteUrl': SITE + '-other', 'mediaJobId': '124', 'hubspotListingId': '31'},
                 {**self.job, 'mediaJobId': '124'}, {**self.job, 'mediaJobId': '124', 'companyIds': ['10', '10']}]
        for invalid in pairs:
            with self.subTest(case=pairs.index(invalid)), self.assertRaises(collector.CaptureError):
                collector.validate_jobs({'schemaVersion': 1, 'jobs': [self.job, invalid]})
        for invalid in [{**self.job, 'companyIds': None}, {**self.job, 'expectedImages': True},
                        {**self.job, 'previousManifestFileId': 'synthetic_manifest_12345'}]:
            with self.assertRaises(collector.CaptureError):
                collector.validate_jobs({'schemaVersion': 1, 'jobs': [invalid]})

    def test_complete_bytes_hashes_jst_and_archive_manifest_contract(self):
        with patch.object(collector, '_now', return_value='2026-10-06T15:00:00+00:00'):
            result, session = self.capture()
        row = result['results'][0]
        image = row['images'][0]
        self.assertEqual((row['status'], row['observedGalleryCount'], row['observedDateJst']), ('complete', 1, '2026-10-07'))
        self.assertFalse(row['historicalImageBytesAvailable'])
        self.assertEqual(Path(image['sourcePath']).read_bytes(), self.raw)
        self.assertEqual(image['sha256'], hashlib.sha256(self.raw).hexdigest())
        self.assertEqual(image['sourceReferenceHash'], hashlib.sha256(FIRST.encode()).hexdigest())
        self.assertEqual(image['acquiredAt'], '2026-10-06T15:00:00+00:00')
        self.assertFalse(session.trust_env)
        self.assertTrue(all(call[1]['allow_redirects'] is False and call[1]['stream'] is True for call in session.calls))
        uploads = [{'result': {'file_id': 'synthetic_image_12345', 'sha256': image['sha256'],
                               'original_bytes': len(self.raw), 'readback_hash_matches': True}}]
        manifests = build_manifests(result['archive'], uploads, 'synthetic_folder_12345')
        self.assertEqual(manifests[0]['listingId'], '30')
        self.assertEqual(manifests[0]['companyIds'], ['10'])
        self.assertEqual(manifests[0]['images'][0]['sha256'], image['sha256'])
        self.assertEqual(manifests[0]['images'][0]['slot'], 1)

    def test_same_url_is_refetched_and_changed_bytes_remain_as_two_originals(self):
        first, _ = self.capture()
        changed = png('blue')
        second, session = self.capture(answers={PAGE: Response(html()), FIRST: Response(changed, 'image/png')})
        before, after = first['results'][0]['images'][0], second['results'][0]['images'][0]
        self.assertEqual(before['sourceReferenceHash'], after['sourceReferenceHash'])
        self.assertNotEqual(before['sha256'], after['sha256'])
        self.assertEqual(Path(before['sourcePath']).read_bytes(), self.raw)
        self.assertEqual(Path(after['sourcePath']).read_bytes(), changed)
        self.assertEqual([url for url, _ in session.calls], [PAGE, FIRST])

    def test_slot_order_and_duplicate_original_references_are_preserved(self):
        job = {**self.job, 'expectedImages': 2}
        result, _ = self.capture(job, {PAGE: Response(html([SECOND, FIRST])), FIRST: Response(self.raw, 'image/png'), SECOND: Response(png('blue'), 'image/png')})
        self.assertEqual([image['slot'] for image in result['results'][0]['images']], [1, 2])
        self.assertEqual([image['sourceReferenceHash'] for image in result['results'][0]['images']], [hashlib.sha256(url.encode()).hexdigest() for url in [SECOND, FIRST]])
        duplicate, _ = self.capture(job, {PAGE: Response(html([FIRST, FIRST])), FIRST: Response(self.raw, 'image/png')})
        self.assertEqual(len(duplicate['archive']['samples']), 2)
        self.assertEqual(len(list((self.output / 'originals').glob('*'))), 2)

    def test_partial_failure_count_mismatch_and_unmapped_never_archive_as_complete(self):
        partial, _ = self.capture({**self.job, 'expectedImages': 2}, {PAGE: Response(html([FIRST, SECOND])), FIRST: Response(self.raw, 'image/png'), SECOND: Response(status=404, mime='image/png')})
        self.assertEqual(partial['results'][0]['status'], 'partial')
        self.assertEqual(partial['results'][0]['failedImages'], 1)
        self.assertEqual(len(partial['results'][0]['images']), 1)
        self.assertEqual(partial['archive']['samples'], [])
        mismatch, _ = self.capture({**self.job, 'expectedImages': 2})
        self.assertEqual(mismatch['results'][0]['status'], 'partial')
        self.assertEqual(mismatch['archive']['samples'], [])
        unmapped = {key: value for key, value in self.job.items() if key not in ('hubspotListingId', 'companyIds')}
        result, _ = self.capture(unmapped)
        self.assertEqual(result['results'][0]['status'], 'complete')
        self.assertEqual(result['archive']['samples'], [])

    def test_missing_page_redirect_transport_and_bad_canonical_are_unavailable_not_zero(self):
        for response in [Response(status=404), Response(status=410), Response(status=302),
                         requests.Timeout('synthetic'), Response(html(canonical=SITE + '/456/'))]:
            result, session = self.capture(answers={PAGE: response})
            row = result['results'][0]
            self.assertEqual(row['status'], 'unavailable')
            self.assertIsNone(row['observedGalleryCount'])
            self.assertEqual(row['images'], [])
            self.assertEqual(result['archive']['samples'], [])
            self.assertEqual(len(session.calls), 1)

    def test_invalid_pixels_wrong_mime_redirect_and_size_limits_are_partial(self):
        cases = [Response(b'not pixels', 'image/png'), Response(self.raw, 'image/jpeg'),
                 Response(self.raw, 'image/svg+xml'), Response(status=302, mime='image/png'),
                 Response(b'x' * (collector.MAX_IMAGE_BYTES + 1), 'image/png')]
        for response in cases:
            result, _ = self.capture(answers={PAGE: Response(html()), FIRST: response})
            self.assertEqual(result['results'][0]['status'], 'partial')
            self.assertEqual(result['results'][0]['failedImages'], 1)
            self.assertEqual(result['archive']['samples'], [])
        result, _ = self.capture(answers={PAGE: Response(b'x' * (collector.MAX_PAGE_BYTES + 1))})
        self.assertEqual(result['results'][0]['status'], 'unavailable')
        self.assertIsNone(result['results'][0]['observedGalleryCount'])

    def test_recognized_empty_gallery_is_zero_only_when_successfully_observed(self):
        result, session = self.capture({**self.job, 'expectedImages': 0}, {PAGE: Response(html([]))})
        self.assertEqual(result['results'][0]['status'], 'complete')
        self.assertEqual(result['results'][0]['observedGalleryCount'], 0)
        self.assertEqual(result['archive']['samples'], [])
        self.assertEqual(len(session.calls), 1)

    def test_gallery_limit_and_previous_manifest_reference(self):
        with self.assertRaisesRegex(collector.CaptureError, 'gallery_image_limit'):
            collector.parse_gallery(html([FIRST] * 101), PAGE)
        job = {**self.job, 'previousManifestFileId': 'synthetic_manifest_12345', 'previousManifestSha256': 'a' * 64}
        result, _ = self.capture(job)
        sample = result['archive']['samples'][0]
        self.assertEqual(sample['previous_manifest_file_id'], job['previousManifestFileId'])
        self.assertEqual(sample['previous_manifest_sha256'], 'a' * 64)

    def test_binary_delivery_requires_real_supported_pixels_and_records_detected_mime(self):
        for mime in ('binary/octet-stream', 'application/octet-stream'):
            result, _ = self.capture(answers={PAGE: Response(html()), FIRST: Response(self.raw, mime)})
            self.assertEqual(result['results'][0]['status'], 'complete')
            self.assertEqual(result['results'][0]['images'][0]['mimeType'], 'image/png')
            self.assertEqual(result['archive']['samples'][0]['sha256'], hashlib.sha256(self.raw).hexdigest())
        result, _ = self.capture(answers={PAGE: Response(html()), FIRST: Response(b'<svg/>', 'binary/octet-stream')})
        self.assertEqual(result['results'][0]['status'], 'partial')
        self.assertEqual(result['archive']['samples'], [])

    def test_cached_account_lookup_is_exact_does_not_guess_and_preserves_mapping(self):
        source = {'schemaVersion': 1, 'jobs': [{key: value for key, value in self.job.items() if key != 'recruitSiteUrl'}]}
        before = copy.deepcopy(source)
        resolved = collector.from_recruit_cache(source, {'synthetic-account': SITE + '/', 'other-account': SITE + '-other'})
        self.assertEqual(resolved['jobs'][0]['recruitSiteUrl'], SITE)
        self.assertEqual(resolved['jobs'][0]['hubspotListingId'], '30')
        self.assertEqual(source, before)
        for cache in [{}, {'other-account': SITE}, {'synthetic-account': 'http://127.0.0.1/recruit/x'}]:
            with self.assertRaises(collector.CaptureError):
                collector.from_recruit_cache(source, cache)
        with self.assertRaisesRegex(collector.CaptureError, 'account_site_mismatch'):
            collector.from_recruit_cache({'schemaVersion': 1, 'jobs': [self.job]}, {'synthetic-account': SITE + '-other'})

    def test_next_failed_run_invalidates_old_archive_and_original_corruption_is_not_overwritten(self):
        first, _ = self.capture()
        self.assertEqual(len(json.loads((self.output / 'archive-samples.json').read_bytes())['samples']), 1)
        original = Path(first['results'][0]['images'][0]['sourcePath'])
        original.write_bytes(b'corrupted synthetic evidence')
        failed, _ = self.capture()
        self.assertEqual(failed['results'][0]['status'], 'partial')
        self.assertEqual(failed['results'][0]['error'], 'stored_blob_mismatch')
        self.assertEqual(json.loads((self.output / 'archive-samples.json').read_bytes())['samples'], [])
        self.assertEqual(original.read_bytes(), b'corrupted synthetic evidence')

    def test_large_pixel_count_is_rejected_even_when_transfer_bytes_are_small(self):
        stream = io.BytesIO()
        Image.new('1', (6001, 5000)).save(stream, format='PNG')
        raw = stream.getvalue()
        self.assertLess(len(raw), collector.MAX_IMAGE_BYTES)
        result, _ = self.capture(answers={PAGE: Response(html()), FIRST: Response(raw, 'image/png')})
        self.assertEqual(result['results'][0]['status'], 'partial')
        self.assertEqual(result['results'][0]['error'], 'invalid_image_format')
        self.assertEqual(result['archive']['samples'], [])

    def test_schema_boolean_unhashable_company_and_control_character_urls_fail_cleanly(self):
        for index, payload in enumerate([{'schemaVersion': True, 'jobs': [self.job]},
                        {'schemaVersion': 1, 'jobs': [{**self.job, 'companyIds': [{}]}]},
                        {'schemaVersion': 1, 'jobs': [{**self.job, 'recruitSiteUrl': '\n' + SITE}]}]):
            with self.subTest(case=index), self.assertRaises(collector.CaptureError):
                collector.validate_jobs(payload)
        with self.assertRaises(collector.CaptureError):
            collector.parse_gallery(html(images=[FIRST + '\n']), PAGE)

    def test_four_original_slots_are_archived_without_claiming_runtime_ui_support(self):
        urls = [f'https://cdn.arwrk.net/images/rct/synthetic-{slot}.png' for slot in range(1, 5)]
        answers = {PAGE: Response(html(urls)), **{url: Response(self.raw, 'image/png') for url in urls}}
        result, session = self.capture({**self.job, 'expectedImages': 4}, answers)
        self.assertEqual(result['results'][0]['status'], 'complete')
        self.assertEqual([image['slot'] for image in result['results'][0]['images']], [1, 2, 3, 4])
        self.assertEqual(len(session.calls), 5)
        self.assertEqual(len(result['archive']['samples']), 4)
        digest = hashlib.sha256(self.raw).hexdigest()
        uploads = [{'result': {'file_id': 'synthetic_image_12345', 'sha256': digest,
                               'original_bytes': len(self.raw), 'readback_hash_matches': True}}]
        manifest = build_manifests(result['archive'], uploads, 'synthetic_folder_12345')[0]
        self.assertEqual([image['slot'] for image in manifest['images']], [1, 2, 3, 4])

    def test_interrupted_batch_never_leaves_previous_archive_success_publishable(self):
        self.capture()
        second = {**self.job, 'mediaJobId': '124', 'hubspotListingId': '31'}
        session = Session({PAGE: Response(html()), FIRST: Response(self.raw, 'image/png'),
                           SITE + '/124/': RuntimeError('synthetic interruption')})
        with self.assertRaisesRegex(RuntimeError, 'synthetic interruption'):
            collector.capture_jobs({'schemaVersion': 1, 'jobs': [self.job, second]}, self.output, session)
        self.assertEqual(json.loads((self.output / 'archive-samples.json').read_bytes()), {'samples': []})
        checkpoint = json.loads((self.output / 'capture-results.json').read_bytes())
        self.assertEqual(checkpoint['state'], 'running')
        self.assertEqual(len(checkpoint['results']), 1)
        self.assertEqual(checkpoint['results'][0]['status'], 'complete')
        self.assertEqual(Path(checkpoint['results'][0]['images'][0]['sourcePath']).read_bytes(), self.raw)


if __name__ == '__main__':
    unittest.main()
