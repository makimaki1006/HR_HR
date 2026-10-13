import { useFrameExtension } from './useFrameExtension';

/** リンクパネルの操作行に出す小さな表示。拡張機能が入っているとき (正常) は出さず、入っていない (無効) ときだけ出す */
export function FrameExtensionBadge() {
  const { installed, version } = useFrameExtension();
  if (installed) return null;
  return <span className="cq-linkbar-ext is-off" data-testid="frame-extension-badge" data-version={version ?? ''}
    title="拡張機能を入れると枠の中で開けるサイトが増えます">枠で開けないサイトがあります</span>;
}
