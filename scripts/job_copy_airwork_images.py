"""Read public AirWork job galleries into private immutable image evidence.

Input comes from the existing account-scoped aw_recruit_urls/XLSX adapter.
No media login, job edit, Drive upload, HubSpot write or scheduler is invoked.
"""
import argparse
import hashlib
from html.parser import HTMLParser
import io
import json
import os
from pathlib import Path
import re
import tempfile
from datetime import datetime, timezone, timedelta
from urllib.parse import urlsplit, urljoin

import requests
from PIL import Image
from job_copy_snapshot_store import LocalSnapshotStore

MAX_PAGE_BYTES = 2 * 1024 * 1024
MAX_IMAGE_BYTES = 5 * 1024 * 1024
MAX_IMAGES = 100


class CaptureError(ValueError):
    def __init__(self, code):
        self.code = code
        super().__init__(code)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def _id(value):
    if not isinstance(value, str) or not re.fullmatch(r'[0-9]{1,30}', value):
        raise CaptureError('invalid_record_id')
    return value


def _site(value):
    if not isinstance(value, str) or any(ord(c) <= 32 or ord(c) == 127 for c in value):
        raise CaptureError('invalid_recruit_site')
    parsed = urlsplit(value)
    if (parsed.scheme != 'https' or parsed.netloc != 'arwrk.net'
            or parsed.query or parsed.fragment
            or not re.fullmatch(r'/recruit/[A-Za-z0-9_-]+/?', parsed.path)):
        raise CaptureError('invalid_recruit_site')
    return value.rstrip('/')


def job_url(site, job):
    # Same construction as existing aw_csv_fetcher.build_aw_job_url, with
    # strict validation of the account-specific site and decimal job ID.
    return _site(site) + '/' + _id(job) + '/'


def validate_jobs(payload):
    if (not isinstance(payload, dict) or set(payload) != {'schemaVersion', 'jobs'}
            or type(payload['schemaVersion']) is not int or payload['schemaVersion'] != 1):
        raise CaptureError('invalid_input_schema')
    jobs = payload['jobs']
    if not isinstance(jobs, list) or not 1 <= len(jobs) <= 59:
        raise CaptureError('job_batch_limit')
    accounts, sites, seen, listings = {}, {}, set(), set()
    allowed = {'accountKey', 'recruitSiteUrl', 'mediaJobId', 'hubspotListingId', 'companyIds', 'expectedImages',
               'previousManifestFileId', 'previousManifestSha256'}
    for job in jobs:
        if not isinstance(job, dict) or set(job) - allowed:
            raise CaptureError('invalid_job_schema')
        account = job.get('accountKey')
        if not isinstance(account, str) or not 1 <= len(account) <= 200 or account != account.strip():
            raise CaptureError('invalid_account_key')
        site = _site(job.get('recruitSiteUrl'))
        media = _id(job.get('mediaJobId'))
        if accounts.setdefault(account, site) != site or sites.setdefault(site, account) != account:
            raise CaptureError('ambiguous_account_site')
        if (account, media) in seen:
            raise CaptureError('duplicate_job_key')
        seen.add((account, media))
        expected = job.get('expectedImages')
        if expected is not None and (type(expected) is not int or not 0 <= expected <= MAX_IMAGES):
            raise CaptureError('invalid_expected_images')
        listing, companies = job.get('hubspotListingId'), job.get('companyIds')
        if (listing is None) != (companies is None):
            raise CaptureError('incomplete_crm_mapping')
        if listing is not None:
            _id(listing)
            if listing in listings:
                raise CaptureError('duplicate_listing')
            listings.add(listing)
            if not isinstance(companies, list) or not 1 <= len(companies) <= 100:
                raise CaptureError('invalid_company_ids')
            for company in companies:
                _id(company)
            if len(set(companies)) != len(companies):
                raise CaptureError('invalid_company_ids')
        previous, previous_sha = job.get('previousManifestFileId'), job.get('previousManifestSha256')
        if (previous is None) != (previous_sha is None):
            raise CaptureError('incomplete_previous_manifest')
        if previous is not None and (listing is None or not isinstance(previous, str)
                or not re.fullmatch(r'[A-Za-z0-9_-]{10,200}', previous)
                or not isinstance(previous_sha, str) or not re.fullmatch(r'[a-f0-9]{64}', previous_sha)):
            raise CaptureError('invalid_previous_manifest')
    return jobs


def from_recruit_cache(payload, cache):
    """Attach sites from the existing aw_recruit_urls.json account-key map.

    The caller supplies current XLSX IDs and optional validated CRM mapping.
    Missing sites fail instead of guessing a site from another account.
    """
    if not isinstance(payload, dict) or not isinstance(payload.get('jobs'), list) or not isinstance(cache, dict):
        raise CaptureError('invalid_input_schema')
    result = {**payload, 'jobs': []}
    for job in payload['jobs']:
        if not isinstance(job, dict) or not isinstance(job.get('accountKey'), str) or job.get('accountKey') not in cache:
            raise CaptureError('recruit_site_not_available')
        site = _site(cache[job['accountKey']])
        if job.get('recruitSiteUrl') is not None and _site(job['recruitSiteUrl']) != site:
            raise CaptureError('account_site_mismatch')
        result['jobs'].append({**job, 'recruitSiteUrl': site})
    validate_jobs(result)
    return result


class Gallery(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.stack, self.canonicals, self.images = [], [], []
        self.has_gallery = False

    def handle_starttag(self, tag, attrs):
        attributes = dict(attrs)
        classes = set((attributes.get('class') or '').split())
        ancestry = [frame[1] for frame in self.stack] + [classes]
        in_gallery = any('job-img' in parent and any('slider' in child for child in ancestry[index + 1:])
                         for index, parent in enumerate(ancestry))
        self.has_gallery |= in_gallery
        if tag == 'link' and 'canonical' in (attributes.get('rel') or '').split():
            self.canonicals.append(attributes.get('href', ''))
        if tag == 'img' and in_gallery and not any('is-clone' in c for c in ancestry):
            self.images.append(attributes.get('src') or attributes.get('data-src') or '')
        if tag not in {'area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'param', 'source', 'track', 'wbr'}:
            self.stack.append((tag, classes))

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        self.handle_endtag(tag)

    def handle_endtag(self, tag):
        for index in range(len(self.stack) - 1, -1, -1):
            if self.stack[index][0] == tag:
                del self.stack[index:]
                break


def image_url(value, page_url):
    if not isinstance(value, str) or not value or any(ord(c) <= 32 or ord(c) == 127 for c in value):
        raise CaptureError('image_url_denied')
    resolved = urljoin(page_url, value)
    parsed = urlsplit(resolved)
    if (parsed.scheme != 'https' or parsed.netloc != 'cdn.arwrk.net' or parsed.fragment
            or not re.fullmatch(r'/images/rct/[A-Za-z0-9_./-]+', parsed.path)
            or '..' in parsed.path.split('/') or '\\' in value):
        raise CaptureError('image_url_denied')
    return resolved


def parse_gallery(html, page_url):
    gallery = Gallery()
    try:
        gallery.feed(html.decode('utf-8', errors='strict'))
    except UnicodeError:
        raise CaptureError('invalid_page_encoding') from None
    if gallery.canonicals != [page_url]:
        raise CaptureError('page_identity_mismatch')
    if not gallery.has_gallery:
        raise CaptureError('gallery_layout_unrecognized')
    if len(gallery.images) > MAX_IMAGES:
        raise CaptureError('gallery_image_limit')
    return [image_url(src, page_url) for src in gallery.images]


def _get(session, url, maximum, page=False):
    try:
        with session.get(url, stream=True, allow_redirects=False, timeout=(10, 30)) as response:
            if response.status_code in (404, 410):
                raise CaptureError('public_page_unavailable' if page else 'image_unavailable')
            if 300 <= response.status_code < 400:
                raise CaptureError('redirect_denied')
            if response.status_code != 200:
                raise CaptureError('upstream_http_failure')
            mime = response.headers.get('Content-Type', '').split(';')[0].strip().lower()
            if page and mime not in ('text/html', 'application/xhtml+xml'):
                raise CaptureError('invalid_page_mime')
            if not page and mime not in ('image/jpeg', 'image/png', 'image/webp', 'application/octet-stream', 'binary/octet-stream'):
                raise CaptureError('invalid_image_mime')
            raw = bytearray()
            for chunk in response.iter_content(65536):
                raw.extend(chunk)
                if len(raw) > maximum:
                    raise CaptureError('response_size_limit')
            return bytes(raw), mime
    except requests.RequestException:
        raise CaptureError('upstream_transport_failure') from None


def _atomic(path, raw):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as handle:
            temporary = Path(handle.name)
            handle.write(raw)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def _save_blob(folder, raw, extension):
    path = folder / (sha(raw) + extension)
    if path.exists():
        if path.read_bytes() != raw:
            raise CaptureError('stored_blob_mismatch')
    else:
        _atomic(path, raw)
    return str(path.resolve())


def _now():
    return datetime.now(timezone.utc).isoformat()


def _capture_jobs(payload, output, session=None):
    jobs = validate_jobs(payload)  # All identities checked before any HTTP GET.
    output = Path(output).resolve()
    folder = output / 'originals'
    folder.mkdir(parents=True, exist_ok=True)
    pages = output / 'pages'
    pages.mkdir(exist_ok=True)
    own_session = session is None
    session = session or requests.Session()
    session.trust_env = False
    results, samples = [], []
    # Invalidate the previous run's publishable output before HTTP acquisition.
    _atomic(output / 'archive-samples.json', b'{"samples":[]}')
    _atomic(output / 'capture-results.json', b'{"schemaVersion":1,"state":"running","results":[]}')
    try:
        for job in jobs:
            started = _now()
            url = job_url(job['recruitSiteUrl'], job['mediaJobId'])
            row = {**job, 'pageReferenceHash': sha(url.encode()), 'observationStartedAt': started,
                   'images': [], 'status': 'unavailable', 'error': None,
                   'observedGalleryCount': None, 'failedImages': 0}
            try:
                raw, _ = _get(session, url, MAX_PAGE_BYTES, page=True)
                row['pageSha256'] = sha(raw)
                row['pageEvidencePath'] = _save_blob(pages, raw, '.html')
                references = parse_gallery(raw, url)
                row['observedGalleryCount'] = len(references)
                for slot, reference in enumerate(references, 1):
                    try:
                        original, mime = _get(session, reference, MAX_IMAGE_BYTES)
                        with Image.open(io.BytesIO(original)) as image:
                            formats = {'JPEG': ('image/jpeg', '.jpg'), 'PNG': ('image/png', '.png'), 'WEBP': ('image/webp', '.webp')}
                            if image.format not in formats or image.width * image.height > 30_000_000:
                                raise CaptureError('invalid_image_format')
                            expected_mime, extension = formats[image.format]
                            if mime not in (expected_mime, 'application/octet-stream', 'binary/octet-stream'):
                                raise CaptureError('image_mime_mismatch')
                            mime = expected_mime
                            image.verify()
                        acquired = _now()
                        row['images'].append({'slot': slot, 'sourceReferenceHash': sha(reference.encode()),
                                              'sha256': sha(original), 'mimeType': mime, 'bytes': len(original),
                                              'sourcePath': _save_blob(folder, original, extension), 'acquiredAt': acquired})
                    except (OSError, SyntaxError, Image.DecompressionBombError):
                        row['failedImages'] += 1
                        row['error'] = 'invalid_image_bytes'
                    except CaptureError as error:
                        row['failedImages'] += 1
                        row['error'] = error.code
                expected = job.get('expectedImages')
                mismatch = expected is not None and expected != len(references)
                if mismatch:
                    row['error'] = 'declared_image_count_mismatch'
                row['status'] = 'partial' if row['failedImages'] or mismatch else 'complete'
            except CaptureError as error:
                row['error'] = error.code
            row['observationCompletedAt'] = _now()
            row['observedDateJst'] = datetime.fromisoformat(started).astimezone(timezone(timedelta(hours=9))).date().isoformat()
            row['historicalImageBytesAvailable'] = False
            if row['status'] == 'complete' and job.get('hubspotListingId'):
                for image in row['images']:
                    sample = {'listing_id': job['hubspotListingId'], 'company_ids': job['companyIds'],
                              'observed_at': started, 'source_slot': image['slot'], 'source_path': image['sourcePath'],
                              'sha256': image['sha256'], 'bytes': image['bytes']}
                    if job.get('previousManifestFileId'):
                        sample.update(previous_manifest_file_id=job['previousManifestFileId'],
                                      previous_manifest_sha256=job['previousManifestSha256'])
                    samples.append(sample)
            results.append(row)
            # Checkpoint after each job. Immutable bytes already survive a
            # later URL withdrawal; this is operator evidence, not a scheduler.
            _atomic(output / 'capture-results.json', json.dumps({'schemaVersion': 1, 'state': 'running', 'results': results}, ensure_ascii=False).encode('utf-8'))
        archive = {'samples': samples}
        _atomic(output / 'archive-samples.json', json.dumps(archive, ensure_ascii=False).encode('utf-8'))
        _atomic(output / 'capture-results.json', json.dumps({'schemaVersion': 1, 'state': 'complete' if all(r['status'] == 'complete' for r in results) else 'partial', 'results': results}, ensure_ascii=False).encode('utf-8'))
        return {'schemaVersion': 1, 'results': results, 'archive': archive}
    finally:
        if own_session:
            session.close()


def capture_jobs(payload, output, session=None):
    validate_jobs(payload)
    # Reuse the existing operator OS-lock helper; no new production store.
    # One output directory cannot publish interleaved observation checkpoints.
    with LocalSnapshotStore(Path(output) / '.capture-lock')._lock():
        return _capture_jobs(payload, output, session)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--recruit-cache', type=Path, help='Existing private aw_recruit_urls.json; sites are matched by exact accountKey')
    args = parser.parse_args()
    try:
        if args.input.stat().st_size > MAX_PAGE_BYTES:
            raise CaptureError('input_size_limit')
        payload = json.loads(args.input.read_text(encoding='utf-8'))
        if args.recruit_cache:
            if args.recruit_cache.stat().st_size > MAX_PAGE_BYTES:
                raise CaptureError('input_size_limit')
            payload = from_recruit_cache(payload, json.loads(args.recruit_cache.read_text(encoding='utf-8')))
        result = capture_jobs(payload, args.output)
        rows = result['results']
        print(json.dumps({'jobs': len(rows), 'complete': sum(r['status'] == 'complete' for r in rows),
                          'partial': sum(r['status'] == 'partial' for r in rows),
                          'unavailable': sum(r['status'] == 'unavailable' for r in rows),
                          'savedImageReferences': sum(len(r['images']) for r in rows),
                          'archiveSamples': len(result['archive']['samples']), 'hubspotWrites': 0, 'driveWrites': 0}))
        return 0 if all(r['status'] == 'complete' for r in rows) else 2
    except Exception as error:
        print(json.dumps({'error': error.code if isinstance(error, CaptureError) else 'capture_failed'}))
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
