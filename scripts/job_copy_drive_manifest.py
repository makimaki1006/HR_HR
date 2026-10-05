"""Immutable per-listing Drive manifest pilot; no HubSpot or permission writes.

Only get/generateIds/create are used. Reuse the same private receipt directory.
Reserved IDs and immutable manifest bytes precede CREATE; pending appProperties
are evidence for a later sync step, not an adopted production Pending store.
"""
from __future__ import annotations

import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import re
import tempfile
import time

from job_copy_drive_pilot import DrivePilot, PilotError, CliError, GwsCli, evidence, identifier, FILE_FIELDS, MAX_BYTES
from job_copy_snapshot_store import canonical


def decimal(value):
    if not isinstance(value, str) or not re.fullmatch(r'[1-9][0-9]{0,29}', value):
        raise PilotError('invalid_crm_id')
    return value


def observed(value):
    if not isinstance(value, str) or not re.fullmatch(
            r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})', value):
        raise PilotError('invalid_observed_at')
    try:
        datetime.fromisoformat(value.replace('Z', '+00:00'))
    except ValueError:
        raise PilotError('invalid_observed_at') from None
    return value


def validate_manifest(manifest):
    required = {'schemaVersion', 'listingId', 'companyIds', 'observedAt', 'images', 'operationId'}
    allowed = required | {'previousManifestFileId', 'previousManifestSha256'}
    if (not isinstance(manifest, dict) or set(manifest) - allowed
            or not required.issubset(manifest) or type(manifest.get('schemaVersion')) is not int
            or manifest.get('schemaVersion') != 1):
        raise PilotError('invalid_manifest_schema')
    if not isinstance(manifest.get('operationId'), str) or not re.fullmatch(r'[a-f0-9]{64}', manifest['operationId']):
        raise PilotError('manifest_operation_mismatch')
    decimal(manifest.get('listingId'))
    observed(manifest.get('observedAt'))
    companies = manifest.get('companyIds')
    if not isinstance(companies, list) or not 0 < len(companies) <= 100:
        raise PilotError('invalid_company_ids')
    companies = [decimal(value) for value in companies]
    if len(set(companies)) != len(companies):
        raise PilotError('invalid_company_ids')
    images = manifest.get('images')
    if not isinstance(images, list) or len(images) > 100:
        raise PilotError('invalid_manifest_images')
    seen = set()
    for image in images:
        if not isinstance(image, dict) or set(image) != {'slot', 'fileId', 'sha256', 'mimeType', 'size'}:
            raise PilotError('invalid_manifest_image')
        slot, size, sha = image.get('slot'), image.get('size'), image.get('sha256')
        if not isinstance(slot, int) or isinstance(slot, bool) or not 1 <= slot <= 100 or slot in seen:
            raise PilotError('invalid_image_slot')
        seen.add(slot)
        identifier(image.get('fileId'))
        if (image.get('mimeType') not in ('image/jpeg', 'image/png', 'image/webp')
                or not isinstance(size, int) or isinstance(size, bool) or not 0 < size <= 5 * 1024 * 1024
                or not isinstance(sha, str) or not re.fullmatch(r'[a-f0-9]{64}', sha)):
            raise PilotError('invalid_manifest_image')
    previous, previous_hash = manifest.get('previousManifestFileId'), manifest.get('previousManifestSha256')
    if (previous is None) != (previous_hash is None):
        raise PilotError('incomplete_previous_manifest_reference')
    if previous is not None:
        identifier(previous)
        if not isinstance(previous_hash, str) or not re.fullmatch(r'[a-f0-9]{64}', previous_hash):
            raise PilotError('invalid_previous_manifest_hash')


def build_manifests(samples, uploads, folder_id, *, identity_folder_id=None):
    """Validate local originals against prior readback receipts; retain no paths."""
    folder_id = identifier(folder_id)
    identity_folder_id = identifier(identity_folder_id) if identity_folder_id is not None else folder_id
    if not isinstance(samples, dict) or not isinstance(samples.get('samples'), list) or not samples['samples']:
        raise PilotError('invalid_samples')
    if not isinstance(uploads, list) or not uploads:
        raise PilotError('invalid_upload_results')
    by_hash = {}
    for upload in uploads:
        result = upload.get('result') if isinstance(upload, dict) else None
        if not isinstance(result, dict) or result.get('readback_hash_matches') is not True:
            raise PilotError('unverified_image_receipt')
        digest = result.get('sha256')
        if not isinstance(digest, str) or not re.fullmatch(r'[a-f0-9]{64}', digest):
            raise PilotError('invalid_image_hash')
        identifier(result.get('file_id'))
        if digest in by_hash and by_hash[digest] != result:
            raise PilotError('conflicting_image_receipts')
        by_hash[digest] = result
    jobs = {}
    for sample in samples['samples']:
        if not isinstance(sample, dict):
            raise PilotError('invalid_sample')
        listing = decimal(sample.get('listing_id'))
        companies = sample.get('company_ids')
        if not isinstance(companies, list) or not companies:
            raise PilotError('invalid_company_ids')
        companies = sorted([decimal(value) for value in companies], key=int)
        if len(set(companies)) != len(companies):
            raise PilotError('invalid_company_ids')
        timestamp = observed(sample.get('observed_at'))
        slot = sample.get('source_slot')
        if not isinstance(slot, int) or isinstance(slot, bool) or slot < 1:
            raise PilotError('invalid_image_slot')
        try:
            raw, mime, _, digest = evidence(sample.get('source_path'))
        except (TypeError, OSError):
            raise PilotError('original_unavailable') from None
        if sample.get('sha256') != digest or sample.get('bytes') != len(raw):
            raise PilotError('sample_original_mismatch')
        result = by_hash.get(digest)
        if result is None or result.get('original_bytes') != len(raw):
            raise PilotError('upload_original_mismatch')
        previous = sample.get('previous_manifest_file_id')
        previous_hash = sample.get('previous_manifest_sha256')
        if (previous is None) != (previous_hash is None):
            raise PilotError('incomplete_previous_manifest_reference')
        if previous is not None:
            identifier(previous)
            if not isinstance(previous_hash, str) or not re.fullmatch(r'[a-f0-9]{64}', previous_hash):
                raise PilotError('invalid_previous_manifest_hash')
        base = {'schemaVersion': 1, 'listingId': listing, 'companyIds': companies,
                'observedAt': timestamp, 'images': []}
        if previous is not None:
            base['previousManifestFileId'] = previous
            base['previousManifestSha256'] = previous_hash
        job = jobs.setdefault(listing, base)
        if any(job.get(k) != base.get(k) for k in ['companyIds', 'observedAt', 'previousManifestFileId', 'previousManifestSha256']):
            raise PilotError('listing_observation_conflict')
        if any(image['slot'] == slot for image in job['images']):
            raise PilotError('duplicate_image_slot')
        job['images'].append({'slot': slot, 'fileId': result['file_id'], 'sha256': digest,
                              'mimeType': mime, 'size': len(raw)})
    manifests = []
    for listing in sorted(jobs, key=int):
        job = jobs[listing]
        job['images'].sort(key=lambda image: image['slot'])
        job['operationId'] = hashlib.sha256(canonical({'folderId': identity_folder_id, 'manifest': job})).hexdigest()
        validate_manifest(job)
        manifests.append(job)
    return manifests


class RetryReadCli:
    """Retry transient GET failures only; never blindly replay CREATE."""
    def __init__(self, cli, sleep=time.sleep):
        self.cli = cli
        self.sleep = sleep
        self.phase = None
        self.transient_read_retries = 0

    def call(self, method, params, body=None, upload=None, output=None):
        self.phase = ('get_media' if params.get('alt') == 'media' else 'get_metadata') if method == 'get' else method
        for attempt in range(3):
            try:
                return self.cli.call(method, params, body, upload, output)
            except CliError as error:
                transient = error.code in (429, 500, 502, 503, 504) or str(error) in (
                    'readback_credential_transport_failed', 'readback_transport_failed')
                if method != 'get' or not transient or attempt == 2:
                    raise
                self.transient_read_retries += 1
                self.sleep(0.25 * (attempt + 1))


class ServiceReadbackCli:
    """Use server credentials for raw media bytes; gws retains upload ownership.

    JSON-returning CLIs can parse/reformat media rather than write original bytes.
    The fixed Drive host GET retains byte-level verification, without exporting
    either credential set or changing permissions.
    """
    def __init__(self, cli, key, credentials=None, session=None, *, max_bytes=MAX_BYTES):
        import requests
        if type(max_bytes) is not int or not 1 <= max_bytes <= 32 * 1024 * 1024:
            raise PilotError('invalid_readback_limit')
        self.max_bytes = max_bytes
        self.cli = cli
        self.session = session or requests.Session()
        self.session.trust_env = False
        if credentials is None:
            from google.oauth2 import service_account
            try:
                credentials = service_account.Credentials.from_service_account_file(
                    str(key), scopes=['https://www.googleapis.com/auth/drive.readonly'])
            except Exception:
                raise PilotError('readback_credentials_unavailable') from None
        self.credentials = credentials

    def call(self, method, params, body=None, upload=None, output=None):
        if method != 'get' or params.get('alt') != 'media' or output is None:
            return self.cli.call(method, params, body, upload, output)
        if body is not None or upload is not None:
            raise PilotError('disallowed_drive_arguments')
        file_id = identifier(params.get('fileId'))
        from google.auth.transport.requests import Request
        from google.auth.exceptions import TransportError
        import requests
        try:
            if not self.credentials.valid:
                auth_request = Request(session=self.session)
                def bounded_auth_request(*args, **kwargs):
                    kwargs['timeout'] = (5, 15)
                    return auth_request(*args, **kwargs)
                self.credentials.refresh(bounded_auth_request)
            with self.session.get('https://www.googleapis.com/drive/v3/files/' + file_id,
                    params={'alt': 'media', 'supportsAllDrives': 'true'},
                    headers={'Authorization': 'Bearer ' + self.credentials.token},
                    timeout=(10, 30), allow_redirects=False, stream=True) as response:
                if response.status_code != 200:
                    raise CliError(code=response.status_code)
                total = 0
                with Path(output).open('wb') as destination:
                    for chunk in response.iter_content(chunk_size=65536):
                        total += len(chunk)
                        if total > self.max_bytes:
                            raise PilotError('readback_too_large')
                        destination.write(chunk)
        except PilotError:
            raise
        except TransportError:
            raise CliError(reason='readback_credential_transport_failed') from None
        except requests.RequestException:
            raise CliError(reason='readback_transport_failed') from None
        except Exception as error:
            category = type(error).__name__
            if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]{0,99}', category):
                category = 'UnknownError'
            raise PilotError('readback_failed_' + category) from None
        return {}


class DriveManifestPilot(DrivePilot):
    def __init__(self, folder_id, receipt_dir, cli=None, *, identity_folder_id=None):
        super().__init__(folder_id, receipt_dir, RetryReadCli(cli or GwsCli()), identity_folder_id=identity_folder_id)

    def verify_remote(self, file_id, mime, size, digest, properties):
        metadata = self.metadata(file_id)
        if metadata is None:
            return None
        actual = metadata.get('appProperties') or {}
        if (metadata.get('id') != file_id or metadata.get('mimeType') != mime
                or str(metadata.get('size')) != str(size) or metadata.get('parents') != [self.folder_id]
                or metadata.get('trashed') is not False
                or metadata.get('capabilities', {}).get('canDownload') is not True
                or any(actual.get(key) != value for key, value in properties.items())):
            raise PilotError('remote_metadata_mismatch')
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=self.output, suffix='.download', delete=False) as handle:
                temporary = Path(handle.name)
            self.cli.call('get', {'fileId': file_id, 'alt': 'media', 'supportsAllDrives': True}, output=temporary)
            if temporary.stat().st_size != size or hashlib.sha256(temporary.read_bytes()).hexdigest() != digest:
                raise PilotError('download_hash_mismatch')
        finally:
            if temporary is not None and temporary.exists():
                temporary.unlink()
        return metadata

    def ensure_manifest(self, manifest, upload=False):
        # Revalidate identity even for callers bypassing build_manifests.
        validate_manifest(manifest)
        expected_operation = dict(manifest)
        operation = expected_operation.pop('operationId', None)
        if operation != hashlib.sha256(canonical({'folderId': self.identity_folder_id, 'manifest': expected_operation})).hexdigest():
            raise PilotError('manifest_operation_mismatch')
        decimal(manifest.get('listingId'))
        observed(manifest.get('observedAt'))
        raw = canonical(manifest)
        digest = hashlib.sha256(raw).hexdigest()
        receipt_path = self.output / f'{operation}.json'
        original_path = self.output / f'{operation}.manifest.json'
        with self.store._lock():
            self.folder()
            for image in manifest['images']:
                if self.verify_remote(identifier(image['fileId']), image['mimeType'], image['size'], image['sha256'],
                                      {'job_copy_sha256': image['sha256']}) is None:
                    raise PilotError('image_file_unavailable')
            receipt = json.loads(receipt_path.read_text(encoding='utf-8')) if receipt_path.exists() else None
            expected = {'folder_id': self.identity_folder_id, 'sha256': digest, 'size': len(raw),
                        'operation_id': operation, 'listing_id': manifest['listingId'],
                        'company_ids': manifest['companyIds'], 'observed_at': manifest['observedAt']}
            if receipt is not None and any(receipt.get(key) != value for key, value in expected.items()):
                raise PilotError('receipt_conflict')
            if receipt is None:
                for prior_path in self.output.glob('*.json'):
                    if not re.fullmatch(r'[a-f0-9]{64}\.json', prior_path.name):
                        continue
                    prior = json.loads(prior_path.read_text(encoding='utf-8'))
                    if (isinstance(prior, dict) and prior.get('listing_id') == manifest['listingId']
                            and prior.get('observed_at') == manifest['observedAt']
                            and prior.get('folder_id') != self.identity_folder_id):
                        raise PilotError('receipt_identity_conflict')
                if not upload:
                    raise PilotError('upload_flag_required')
                self.store._immutable(original_path, raw)
                ids = self.cli.call('generateIds', {'count': 1, 'space': 'drive', 'type': 'files'}).get('ids')
                if not isinstance(ids, list) or len(ids) != 1:
                    raise PilotError('invalid_reserved_id_response')
                file_id = identifier(ids[0])
                receipt = {**expected, 'file_id': file_id, 'state': 'reserved', 'classification': 'local_pilot_only'}
                self.store._atomic(receipt_path, canonical(receipt))
            else:
                file_id = identifier(receipt.get('file_id'))
                if not original_path.is_file() or original_path.read_bytes() != raw:
                    raise PilotError('original_evidence_missing_or_changed')
            properties = {'job_copy_kind': 'manifest', 'job_copy_sync': 'pending',
                          'job_copy_sha256': digest, 'job_copy_operation': operation}
            metadata = self.metadata(file_id)
            if metadata is None:
                if not upload:
                    raise PilotError('reserved_file_not_found_upload_flag_required')
                receipt['state'] = 'sending'
                self.store._atomic(receipt_path, canonical(receipt))
                self.create_attempts += 1
                try:
                    self.cli.call('create', {'supportsAllDrives': True, 'fields': FILE_FIELDS}, body={
                        'id': file_id, 'name': f'job-copy-manifest-{operation}.json', 'mimeType': 'application/json',
                        'parents': [self.folder_id], 'appProperties': properties}, upload=original_path)
                except PilotError as error:
                    receipt.update(state='uncertain', last_error=str(error))
                    self.store._atomic(receipt_path, canonical(receipt))
                    if self.metadata(file_id) is None:
                        raise error
            receipt['state'] = 'uploaded_unverified'
            self.store._atomic(receipt_path, canonical(receipt))
            try:
                metadata = self.verify_remote(file_id, 'application/json', len(raw), digest, properties)
                if metadata is None:
                    raise PilotError('created_file_not_yet_visible')
            except PilotError:
                receipt['state'] = 'verification_failed'
                self.store._atomic(receipt_path, canonical(receipt))
                raise
            receipt.update(state='verified_pending', sync_state='pending')
            receipt.pop('last_error', None)
            self.store._atomic(receipt_path, canonical(receipt))
            return {'listing_id': manifest['listingId'], 'manifest_file_id': file_id,
                    'company_ids': manifest['companyIds'], 'observed_at': manifest['observedAt'],
                    'operation_id': operation, 'sha256': digest, 'bytes': len(raw),
                    'image_count': len(manifest['images']), 'readback_hash_matches': True,
                    'sync_state': 'pending', 'create_attempts': self.create_attempts,
                    'classification': 'local_pilot_only'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--samples', type=Path, required=True)
    parser.add_argument('--upload-results', type=Path, required=True)
    parser.add_argument('--folder-id', required=True)
    parser.add_argument('--identity-folder-id', help='Stable initial operation namespace; defaults to folder-id')
    parser.add_argument('--receipt-dir', type=Path, required=True)
    parser.add_argument('--readback-service-key', type=Path,
                        help='Use this server credential for raw Drive media GET; no credential export')
    parser.add_argument('--upload', action='store_true')
    args = parser.parse_args()
    pilot = None
    try:
        manifests = build_manifests(json.loads(args.samples.read_text(encoding='utf-8-sig')),
            json.loads(args.upload_results.read_text(encoding='utf-8-sig')), args.folder_id,
            identity_folder_id=args.identity_folder_id)
        cli = GwsCli()
        if args.readback_service_key is not None:
            cli = ServiceReadbackCli(cli, args.readback_service_key)
        pilot = DriveManifestPilot(args.folder_id, args.receipt_dir, cli, identity_folder_id=args.identity_folder_id)
        results = [pilot.ensure_manifest(manifest, args.upload) for manifest in manifests]
        pilot.store._atomic(pilot.output / 'manifest-results.json', canonical({'results': results}))
        print(json.dumps({'ok': True, 'manifest_count': len(results),
                          'image_references': sum(r['image_count'] for r in results),
                          'create_attempts': pilot.create_attempts, 'hubspot_writes': 0}))
    except Exception as error:
        reason = str(error) if isinstance(error, PilotError) else type(error).__name__
        diagnostic = {'ok': False, 'error': reason,
                      'phase': pilot.cli.phase if pilot is not None else 'local_validation',
                      'transient_read_retries': pilot.cli.transient_read_retries if pilot is not None else 0}
        if pilot is not None:
            pilot.store._atomic(pilot.output / 'last-failure.json', canonical(diagnostic))
        print(json.dumps(diagnostic))
        raise SystemExit(1)


if __name__ == '__main__':
    main()
