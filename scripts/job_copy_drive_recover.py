"""Read-only reconstruction of manifest candidates from one Drive folder.

No local upload receipts are needed. Every discovered manifest is a candidate;
pending appProperties do not prove missing HubSpot acknowledgement. The next
authorized Rust step compares the current/history pointer before deciding writes.
Incomplete enumeration fails without replacing an existing output file.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

from job_copy_drive_manifest import ServiceReadbackCli, RetryReadCli, validate_manifest
from job_copy_drive_pilot import GwsCli, CliError, PilotError, identifier, FILE_FIELDS
from job_copy_snapshot_store import LocalSnapshotStore, canonical

MAX_PAGES = 10
MANIFEST_LIMIT = 2 * 1024 * 1024


class GwsManifestList:
    def __init__(self, folder_id, cli=None):
        self.folder_id = identifier(folder_id)
        self.cli = cli or GwsCli()

    def page(self, cursor=None):
        params = {'q': f"'{self.folder_id}' in parents and trashed = false and mimeType = 'application/json' "
                         "and appProperties has { key='job_copy_kind' and value='manifest' }",
                  'pageSize': 100, 'supportsAllDrives': True, 'includeItemsFromAllDrives': True,
                  'fields': f'nextPageToken,incompleteSearch,files({FILE_FIELDS})'}
        if cursor is not None:
            params['pageToken'] = cursor
        try:
            result = subprocess.run([self.cli.executable, 'drive', 'files', 'list', '--params', json.dumps(params)],
                capture_output=True, encoding='utf-8', errors='replace', timeout=120, check=False)
        except subprocess.TimeoutExpired:
            raise PilotError('listing_transport_unknown') from None
        except OSError:
            raise PilotError('listing_cli_failed') from None
        value = GwsCli._json(result.stdout)
        if result.returncode != 0 or not isinstance(value, dict) or 'error' in value:
            for data in (value, GwsCli._json(result.stderr)):
                error = data.get('error') if isinstance(data, dict) else None
                if isinstance(error, dict) and type(error.get('code')) is int and 100 <= error['code'] <= 599:
                    raise CliError(error['code'])
            raise PilotError('listing_cli_failed')
        return value


def list_candidates(pages, folder_id):
    folder_id = identifier(folder_id)
    results, seen_ids, seen_cursors = [], set(), set()
    cursor = None
    for _ in range(MAX_PAGES):
        page = pages.page(cursor)
        if not isinstance(page, dict) or 'error' in page or page.get('incompleteSearch') is True:
            raise PilotError('incomplete_manifest_listing')
        files = page.get('files')
        if not isinstance(files, list) or len(files) > 100:
            raise PilotError('invalid_manifest_page')
        for value in files:
            if not isinstance(value, dict):
                raise PilotError('invalid_manifest_metadata')
            file_id = identifier(value.get('id'))
            if file_id in seen_ids:
                raise PilotError('duplicate_manifest_id')
            seen_ids.add(file_id)
            props = value.get('appProperties')
            digest = props.get('job_copy_sha256') if isinstance(props, dict) else None
            size = value.get('size')
            if (value.get('parents') != [folder_id] or value.get('mimeType') != 'application/json'
                    or value.get('trashed') is not False
                    or value.get('capabilities', {}).get('canDownload') is not True
                    or not isinstance(props, dict) or props.get('job_copy_kind') != 'manifest'
                    or not isinstance(digest, str) or not re.fullmatch(r'[a-f0-9]{64}', digest)
                    or not isinstance(size, str) or not size.isascii() or not size.isdigit()
                    or not 0 < int(size) <= MANIFEST_LIMIT):
                raise PilotError('invalid_manifest_metadata')
            results.append(value)
        next_cursor = page.get('nextPageToken')
        if next_cursor is None:
            return results
        if not isinstance(next_cursor, str) or not 0 < len(next_cursor) <= 4096 or next_cursor in seen_cursors:
            raise PilotError('invalid_manifest_cursor')
        seen_cursors.add(next_cursor)
        cursor = next_cursor
    raise PilotError('manifest_listing_page_limit')


def recover(folder_id, pages, readback, output, *, identity_folder_id=None):
    folder_id = identifier(folder_id)
    identity_folder_id = identifier(identity_folder_id) if identity_folder_id is not None else folder_id
    output = Path(output).resolve()
    candidates = list_candidates(pages, folder_id)
    results = []
    output.parent.mkdir(parents=True, exist_ok=True)
    for candidate in candidates:
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=output.parent, suffix='.download', delete=False) as handle:
                temporary = Path(handle.name)
            readback.call('get', {'fileId': candidate['id'], 'alt': 'media', 'supportsAllDrives': True}, output=temporary)
            if temporary.stat().st_size != int(candidate['size']) or temporary.stat().st_size > MANIFEST_LIMIT:
                raise PilotError('manifest_size_mismatch')
            raw = temporary.read_bytes()
            digest = hashlib.sha256(raw).hexdigest()
            if digest != candidate['appProperties']['job_copy_sha256']:
                raise PilotError('manifest_hash_mismatch')
            try:
                manifest = json.loads(raw)
            except ValueError:
                raise PilotError('manifest_json_invalid') from None
            validate_manifest(manifest)
            identity = dict(manifest)
            operation = identity.pop('operationId')
            if (operation != hashlib.sha256(canonical({'folderId': identity_folder_id, 'manifest': identity})).hexdigest()
                    or candidate['appProperties'].get('job_copy_operation') != operation):
                raise PilotError('manifest_operation_mismatch')
            results.append({'listing_id': manifest['listingId'], 'company_ids': manifest['companyIds'],
                'observed_at': manifest['observedAt'], 'manifest_file_id': candidate['id'],
                'operation_id': operation, 'sha256': digest, 'bytes': len(raw),
                'image_count': len(manifest['images']), 'readback_hash_matches': True,
                'sync_state': 'candidate_requires_hubspot_comparison', 'create_attempts': 0,
                'classification': 'drive_recovery_candidate'})
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    # Publish only the complete verified enumeration. Keep earlier output on failure.
    store = LocalSnapshotStore(output.parent / '.recovery-output-lock')
    with store._lock():
        store._atomic(output, canonical({'results': results}))
    return {'manifest_candidates': len(results), 'image_references': sum(r['image_count'] for r in results),
            'create_attempts': 0, 'hubspot_writes': 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--folder-id', required=True)
    parser.add_argument('--identity-folder-id', help='Stable initial operation namespace; defaults to folder-id')
    parser.add_argument('--readback-service-key', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        cli = GwsCli()
        result = recover(args.folder_id, GwsManifestList(args.folder_id, cli),
                         RetryReadCli(ServiceReadbackCli(cli, args.readback_service_key)), args.output,
                         identity_folder_id=args.identity_folder_id)
        print(json.dumps({'ok': True, **result}))
    except Exception as error:
        print(json.dumps({'ok': False, 'error': str(error) if isinstance(error, PilotError) else type(error).__name__}))
        raise SystemExit(1)


if __name__ == '__main__':
    main()
