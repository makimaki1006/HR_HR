// ⑤ 画像案・生成プロンプト (旧 renderImages)。
import type { ImageDirection } from '../../../generated/ImageDirection';
import type { ImagePrompt } from '../../../generated/ImagePrompt';
import type { NumberViolation } from '../../../generated/NumberViolation';
import {
  ConfirmBox,
  CopyButton,
  GateBadge,
  NumberCheckNote,
  NumberViolations,
  SectionHead,
} from '../parts';
import type { StepKey } from '../state';

export function ImagesSection({
  images,
  imagePrompts,
  imagePromptsError,
  imagesNv,
  imagesNumberCheck,
  confirmed,
  onConfirm,
}: {
  images: ImageDirection[];
  imagePrompts: ImagePrompt[];
  imagePromptsError: string;
  imagesNv: NumberViolation[];
  imagesNumberCheck: string;
  confirmed: boolean;
  onConfirm: (key: StepKey, checked: boolean) => void;
}) {
  // ⑤b のプロンプトを persona_label で引けるようにしておく。
  const pmap = new Map<string, ImagePrompt>();
  for (const p of imagePrompts) pmap.set(p.persona_label || '', p);
  const nvCount = imagesNv.length;
  return (
    <>
      <SectionHead
        num="⑤"
        name="画像案・生成プロンプト"
        gates={
          nvCount ? (
            <GateBadge label="数値照合" cls="bad" detail={`原文にない数値 ${String(nvCount)}件`} />
          ) : null
        }
      />
      <NumberViolations items={imagesNv} />
      <NumberCheckNote check={imagesNumberCheck} />
      {images.length ? (
        images.map((d, i) => {
          const label = d.persona_label || '';
          const p = pmap.get(label);
          let promptBlk = null;
          if (p) {
            promptBlk = (
              <div className="pwrap">
                {p.appeal_core ? (
                  <div className="pappeal">🎯 この画像の狙い: {p.appeal_core}</div>
                ) : null}
                <div className="phead">
                  🎨 画像生成AI用プロンプト（そのまま貼り付け可・撮影指示書兼用）
                  <CopyButton text={p.prompt || ''} label="プロンプトをコピー" />
                </div>
                <div className="pbox">{p.prompt || ''}</div>
                <div className="phead">
                  🚫 ネガティブプロンプト（避けたい要素）
                  <CopyButton text={p.negative_prompt || ''} label="コピー" />
                </div>
                <div className="pbox pneg">{p.negative_prompt || ''}</div>
                <div className="pmeta">
                  推奨アスペクト比: <b>{p.aspect_ratio || ''}</b>
                </div>
              </div>
            );
          } else if (imagePrompts.length === 0 && imagePromptsError) {
            promptBlk = (
              <div className="err">
                プロンプト化に失敗: {imagePromptsError}（工程⑤を再実行すると再試行します）
              </div>
            );
          } else if (imagePrompts.length === 0) {
            promptBlk = <div className="note">生成AI用プロンプトへ変換中…</div>;
          }
          return (
            <div key={i} className="dircard">
              <div className="dl">{label}</div>
              <div className="dt">{d.direction || ''}</div>
              {promptBlk}
            </div>
          );
        })
      ) : (
        <div className="note">ディレクションがありません。</div>
      )}
      <ConfirmBox stepKey="images" checked={confirmed} onChange={onConfirm} />
    </>
  );
}
