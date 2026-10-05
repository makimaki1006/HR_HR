import { AppShell } from '../../shell';
import { JobCopyScreen } from './JobCopyScreen';

/** Shared application navigation; CRM-specific filters stay within the screen. */
export function JobCopyApp() {
  return <div className="jc-shell"><a className="jc-skip-link" href="#job-details">求人の詳細へ移動</a><AppShell screen="job-copy"><JobCopyScreen /></AppShell></div>;
}
