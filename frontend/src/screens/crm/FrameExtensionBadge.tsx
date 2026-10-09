import { useFrameExtension } from './useFrameExtension';

/** リンクパネルの操作行に出す小さな表示。拡張機能が入っているときだけ出す (入っていなくても何も出さない) */
export function FrameExtensionBadge() {
  const { installed, version } = useFrameExtension();
  if (!installed) return null;
  return <span className="cq-linkbar-ext" data-testid="frame-extension-badge" data-version={version ?? ''}
    title="埋め込みを禁止しているサイトも枠の中で表示できます（ブラックリストのサイトを除く）">拡張機能: 有効</span>;
}
