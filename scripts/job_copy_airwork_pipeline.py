"""Operator-only AirWork capture/archive runner. Default is offline dry-run.

Input: collector schema plus mandatory kyuujin_status on each job. Only 公開中
is eligible. Private plan identity and OS lock prevent accidental run mixing.
No credentials/IDs/URLs are printed. Existing HRH receipts must be retained.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from job_copy_airwork_images import CaptureError, capture_jobs, from_recruit_cache, validate_jobs
from job_copy_archive_batch import archive, preflight
from job_copy_snapshot_store import LocalSnapshotStore, canonical


def select(payload, cache=None):
    if not isinstance(payload, dict) or set(payload) != {'schemaVersion', 'jobs'} or type(payload['schemaVersion']) is not int or payload['schemaVersion'] != 1:
        raise CaptureError('invalid_input_schema')
    rows = payload['jobs']
    if not isinstance(rows, list) or not 1 <= len(rows) <= 10000:
        raise CaptureError('input_job_limit')
    jobs, seen = [], set()
    excluded = 0
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get('kyuujin_status'), str):
            raise CaptureError('publication_status_required')
        if row['kyuujin_status'] != '公開中':
            excluded += 1
            continue
        job = {key: value for key, value in row.items() if key != 'kyuujin_status'}
        if not job.get('hubspotListingId') or not job.get('companyIds'):
            raise CaptureError('crm_mapping_required')
        if job['hubspotListingId'] in seen:
            raise CaptureError('duplicate_listing')
        seen.add(job['hubspotListingId'])
        jobs.append(job)
    # Global account/site/key validation, while keeping the existing 59-job limit.
    accounts, sites, keys = {}, {}, set()
    for start in range(0, len(jobs), 59):
        chunk = {'schemaVersion': 1, 'jobs': jobs[start:start + 59]}
        if cache is not None:
            chunk = from_recruit_cache(chunk, cache)
        validate_jobs(chunk)
        jobs[start:start + 59] = chunk['jobs']
        for job in chunk['jobs']:
            account, site = job['accountKey'], job['recruitSiteUrl']
            key = (account, job['mediaJobId'])
            if (accounts.setdefault(account, site) != site or sites.setdefault(site, account) != account or key in keys):
                raise CaptureError('ambiguous_account_site')
            keys.add(key)
    return jobs, excluded


def load(path):
    if path.stat().st_size > 32 * 1024 * 1024:
        raise CaptureError('input_size_limit')
    return json.loads(path.read_bytes())


def captured(job, folder):
    """Resume only identity-matching complete observations and verified original bytes."""
    result_path = folder / 'capture-results.json'
    if not result_path.is_file():
        return None
    result = load(result_path)
    rows = result.get('results', [])
    if result.get('state') != 'complete' or len(rows) != 1 or rows[0].get('status') != 'complete':
        return None
    if any(rows[0].get(key) != value for key, value in job.items()):
        raise CaptureError('checkpoint_identity_mismatch')
    samples = load(folder / 'archive-samples.json')
    expected = [{'listing_id': job['hubspotListingId'], 'company_ids': job['companyIds'],
                 'observed_at': rows[0]['observationStartedAt'], 'source_slot': image['slot'],
                 'source_path': image['sourcePath'], 'sha256': image['sha256'], 'bytes': image['bytes'],
                 'airwork_source': {'mediaJobId': job['mediaJobId'], 'accountKey': job['accountKey']},
                 **({'previous_manifest_file_id': job['previousManifestFileId'],
                     'previous_manifest_sha256': job['previousManifestSha256']} if job.get('previousManifestFileId') else {})}
                for image in rows[0]['images']]
    if samples != {'samples': expected}:
        raise CaptureError('checkpoint_samples_mismatch')
    if expected:
        preflight(samples, 'synthetic_preflight', 'synthetic_preflight')
    return {'results': rows, 'archive': samples}


def run(args):
    jobs, excluded = select(load(args.input), load(args.recruit_cache) if args.recruit_cache else None)
    summary = {'eligibleJobs': len(jobs), 'excludedJobs': excluded, 'batches': (len(jobs) + 58) // 59}
    if not args.apply:
        # Offline: no output/receipt mutation, GET, credentials, gws or Rust invocation.
        if args.phase == 'store':
            config = load(args.storage_config)
            for start in range(0, len(jobs), 59):
                samples = []
                for index, job in enumerate(jobs[start:start + 59], start):
                    result = captured(job, args.output / 'capture' / str(index))
                    if result:
                        samples.extend(result['archive']['samples'])
                if samples:
                    preflight({'samples': samples}, config['folderId'], config['identityFolderId'])
        return {**summary, 'dryRun': True, 'driveWrites': 0, 'hubspotWrites': 0}
    state = LocalSnapshotStore(args.output / '.pipeline-lock')
    with state._lock():
        identity = hashlib.sha256(canonical(jobs)).hexdigest()
        plan_path = args.output / 'plan.json'
        if plan_path.exists() and load(plan_path) != {'inputHash': identity}:
            raise CaptureError('plan_changed_use_new_output')
        state._atomic(plan_path, canonical({'inputHash': identity}))
        if args.phase == 'store':
            check = subprocess.run([str(args.link_bin), str(args.env_file), str(args.service_key), '--check'],
                                   capture_output=True, check=False)
            if check.returncode:
                raise CaptureError('existing_image_property_required')
        unavailable = complete = 0
        for start in range(0, len(jobs), 59):
            samples = []
            for index, job in enumerate(jobs[start:start + 59], start):
                folder = args.output / 'capture' / str(index)
                result = captured(job, folder)
                if result is None and args.phase == 'capture':
                    result = capture_jobs({'schemaVersion': 1, 'jobs': [job]}, folder)
                if result is None or result['results'][0]['status'] != 'complete':
                    unavailable += 1
                    print(json.dumps({'job': index + 1, 'reason': '画像を取得できませんでした'}, ensure_ascii=False), flush=True)
                else:
                    complete += 1
                    samples.extend(result['archive']['samples'])
            if args.phase == 'store' and samples:
                batch = args.output / 'archive' / str(start // 59)
                archive({'samples': samples}, load(args.storage_config), args.service_key, batch, upload=True)
                # Only existing property; no browser or MOC dependency, no property CREATE.
                reply = subprocess.run([str(args.link_bin), str(args.env_file), str(args.service_key),
                                        str(batch / 'manifest-results.json'), str(batch / 'links'), '--apply'],
                                       capture_output=True, check=False)
                if reply.returncode:
                    raise CaptureError('pointer_sync_pending')
                print(json.dumps({'batch': start // 59 + 1, 'state': '登録確認済み'}, ensure_ascii=False), flush=True)
        result = {**summary, 'completeJobs': complete, 'unavailableJobs': unavailable, 'phase': args.phase}
        state._atomic(args.output / ('summary-' + args.phase + '.json'), canonical(result))
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--recruit-cache', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--phase', choices=['capture', 'store'], default='capture')
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--apply', action='store_true')
    mode.add_argument('--dry-run', dest='apply', action='store_false')
    parser.set_defaults(apply=False)
    for flag in ['storage-config', 'service-key', 'link-bin', 'env-file']:
        parser.add_argument('--' + flag, type=Path)
    args = parser.parse_args()
    if args.phase == 'store' and (not args.storage_config or args.apply and not all([args.service_key, args.link_bin, args.env_file])):
        parser.error('store requires storage config; apply also requires key, link binary and env file')
    try:
        result = run(args)
        print(json.dumps(result, ensure_ascii=False), flush=True)
        return 2 if result.get('unavailableJobs') else 0
    except (Exception, KeyboardInterrupt) as error:
        print(json.dumps({'error': error.code if isinstance(error, CaptureError) else '実行を停止しました'}, ensure_ascii=False), flush=True)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
