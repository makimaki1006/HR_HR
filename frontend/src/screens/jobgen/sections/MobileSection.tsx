// ⑥ スマホ原稿 (旧 renderMobile + jobPostCard + 意図ポップアップ)。
import { type ReactNode, useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { buildMobileView, type IntentPopup, type JobPostModel } from '../mobileView';
import {
  ConfirmBox,
  CopyButton,
  ExprWarnings,
  GateBadge,
  ManuscriptLines,
  NumberCheckNote,
  NumberViolations,
  SectionHead,
} from '../parts';
import type { MobileResult, PipelineState, StepKey } from '../state';

interface Active {
  idx: number;
  el: HTMLElement;
  /** フォーカスで出したものはマウスが離れても消さない。 */
  byFocus: boolean;
}

/** 表示してから実寸を測り、画面外にはみ出さない位置へ寄せる (旧 ipopPlace)。 */
function place(box: HTMLDivElement, anchor: HTMLElement): void {
  const r = anchor.getBoundingClientRect();
  const pw = box.offsetWidth;
  const ph = box.offsetHeight;
  let left = r.left;
  let top = r.bottom + 8;
  if (left + pw > window.innerWidth - 10) left = window.innerWidth - pw - 10;
  if (left < 10) left = 10;
  if (top + ph > window.innerHeight - 10) top = Math.max(10, r.top - ph - 8);
  box.style.left = `${String(left)}px`;
  box.style.top = `${String(top)}px`;
}

function IntentPopupBox({ popup, anchor }: { popup: IntentPopup; anchor: HTMLElement }) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const box = ref.current;
    if (!box) return;
    place(box, anchor);
    const reflow = () => {
      if (document.body.contains(anchor)) place(box, anchor);
    };
    // スクロール中は位置だけ追従させる (旧 ipopReflow)。
    window.addEventListener('scroll', reflow, true);
    return () => {
      window.removeEventListener('scroll', reflow, true);
    };
  }, [popup, anchor]);
  return (
    <div ref={ref} className="ipop on" id="intentPop">
      <div className="ih">{popup.head}</div>
      {popup.rows.map((r, i) => (
        <div key={i} className="ir">
          <div className="ik">{r[0]}</div>
          <div className="iv">{r[1]}</div>
        </div>
      ))}
      {popup.foot ? <div className="ifoot">{popup.foot}</div> : null}
    </div>
  );
}

interface Hover {
  show: (idx: number | null, el: HTMLElement, byFocus: boolean) => void;
  hide: (byFocus: boolean) => void;
}

/** 意図ポップアップ付きブロック。`popup` が null なら配線しない (旧 ipopAttr)。 */
function Intent({
  as: Tag,
  className,
  popup,
  hover,
  children,
}: {
  as: 'div' | 'span' | 'h3';
  className: string;
  popup: number | null;
  hover: Hover;
  children: ReactNode;
}) {
  if (popup === null) return <Tag className={className}>{children}</Tag>;
  return (
    <Tag
      className={className}
      data-ipop={popup}
      tabIndex={0}
      onMouseEnter={(e) => {
        hover.show(popup, e.currentTarget, false);
      }}
      onMouseLeave={() => {
        hover.hide(false);
      }}
      onFocus={(e) => {
        hover.show(popup, e.currentTarget, true);
      }}
      onBlur={() => {
        hover.hide(true);
      }}
    >
      {children}
    </Tag>
  );
}

function JobPostCard({ m, hover }: { m: JobPostModel; hover: Hover }) {
  return (
    <article className="jpost">
      <Intent as="div" className="jpost-photo" popup={m.photoPopup} hover={hover}>
        <span className="jpost-cam">📷</span>
        <span className="jpost-ph-tag">写真はイメージ</span>
        {m.photoDirection ? (
          <div className="jpost-ph-txt">
            <span className="pht">撮影案（⑤画像案）</span>
            {m.photoDirection}
          </div>
        ) : null}
      </Intent>
      <div className="jpost-main">
        {m.catchCopy ? (
          <Intent as="div" className="jpost-catch" popup={m.catchCopy.popup} hover={hover}>
            {m.catchCopy.style ? <span className="jstyle">{m.catchCopy.style}</span> : null}
            {m.catchCopy.text}
          </Intent>
        ) : null}
        {m.title ? <h3 className="jpost-title">{m.title}</h3> : null}
        {m.badges.length ? (
          <div className="jbadges">
            {m.badges.map((b) => (
              <Intent key={b.key} as="span" className={`jbadge${b.cls}`} popup={b.popup} hover={hover}>
                <span className="jbk">{b.label}</span>
                {b.value}
              </Intent>
            ))}
          </div>
        ) : null}
        {m.chips.length ? (
          <Intent as="div" className="jchips" popup={m.chipsPopup} hover={hover}>
            {m.chips.map((x, i) => (
              <span key={i} className="jchip">
                {x}
              </span>
            ))}
          </Intent>
        ) : null}
        <Intent as="div" className="jsec" popup={m.bodyPopup} hover={hover}>
          <div className="jsec-h">募集メッセージ</div>
          <div className="jbody">
            <ManuscriptLines lines={m.lines} />
          </div>
        </Intent>
        {m.reqRows.length ? (
          <div className="jsec">
            <div className="jsec-h">募集要項</div>
            <div className="jdl">
              {m.reqRows.map((r) => (
                <Intent key={r.key} as="div" className="jrow" popup={r.popup} hover={hover}>
                  <div className="jdt">{r.label}</div>
                  <div className="jdd">{r.value}</div>
                </Intent>
              ))}
            </div>
          </div>
        ) : null}
        <button className="japply" type="button" disabled>
          応募する（イメージ）
        </button>
        <div className="jdisc">※この作成例はあくまでイメージです。</div>
      </div>
    </article>
  );
}

export function MobileSection({
  state,
  mobile,
  confirmed,
  onConfirm,
}: {
  state: PipelineState;
  mobile: MobileResult[];
  confirmed: boolean;
  onConfirm: (key: StepKey, checked: boolean) => void;
}) {
  const view = buildMobileView(state, mobile);
  const [active, setActive] = useState<Active | null>(null);
  const show = useCallback((idx: number | null, el: HTMLElement, byFocus: boolean) => {
    if (idx === null) return;
    setActive({ idx, el, byFocus });
  }, []);
  const hide = useCallback((byFocus: boolean) => {
    setActive((cur) => (cur && cur.byFocus && !byFocus ? cur : null));
  }, []);
  const hover: Hover = { show, hide };
  useEffect(() => {
    const onResize = () => {
      setActive(null);
    };
    window.addEventListener('resize', onResize);
    return () => {
      window.removeEventListener('resize', onResize);
    };
  }, []);
  const activePopup = active ? view.popups[active.idx] : undefined;

  const totalNg = mobile.reduce((n, m) => n + m.ng_violations.length, 0);
  const totalNv = mobile.reduce((n, m) => n + m.number_violations.length, 0);
  const totalEw = mobile.reduce((n, m) => n + m.expression_warnings.length, 0);
  const cardByLabel = new Map(view.cards.map((c) => [c.label, c]));
  const w375 = { width: '375px', maxWidth: '100%' } as const;

  return (
    <>
      <SectionHead
        num="⑥"
        name="スマホ原稿"
        gates={
          <>
            <GateBadge
              label="文字数・法令NGワード"
              cls={totalNg ? 'bad' : 'ok'}
              detail={totalNg ? `違反 ${String(totalNg)}件` : '通過'}
            />
            <GateBadge
              label="数値照合"
              cls={totalNv ? 'bad' : 'ok'}
              detail={totalNv ? `原文にない数値 ${String(totalNv)}件` : '通過'}
            />
            {totalEw ? (
              <GateBadge label="表現レビュー" cls="warn" detail={`要確認 ${String(totalEw)}件`} />
            ) : null}
          </>
        }
      />
      {view.popups.length ? (
        <p className="ihint">
          下は<b>求人ページ風の作成例（イメージ）</b>
          です。各ブロックにマウスを載せる（キーボードならフォーカスする）と、どのターゲットのどんな不満・痛みに向けた訴求なのかが出ます。表示内容は③ペルソナ設計・④キャッチコピー・⑤画像案・①事実抽出の生成結果そのものです。
        </p>
      ) : null}
      <div className="mobwrap">
        {mobile.map((m, i) => {
          if (m.error !== undefined) {
            return (
              <div key={i} className="mobcol">
                <div className="moblabel">{m.label}</div>
                <div className="err">{m.error}</div>
              </div>
            );
          }
          const card = cardByLabel.get(m.label);
          const warn =
            m.review_required || m.number_violations.length || m.expression_warnings.length;
          return (
            <div key={i} className="mobcol">
              <div className="moblabel">
                {m.label}
                {warn ? (
                  <>
                    {' '}
                    <span className="gstat review">要確認</span>
                  </>
                ) : null}
              </div>
              {card ? <JobPostCard m={card} hover={hover} /> : null}
              {/* 生成された原稿そのもの（プレーンテキスト）も畳んで残す。コピーはここから。 */}
              <details className="jraw">
                <summary>原稿テキスト（プレーン）</summary>
                <div className="phone">
                  <ManuscriptLines lines={m.lines} />
                </div>
                <div className="phone-cap">スマホ幅 375px プレビュー</div>
                <div className="jrawbar">
                  <CopyButton text={m.lines.join('\n')} label="原稿テキストをコピー" />
                </div>
              </details>
              {m.ng_violations.length ? (
                <div className="ngbadge" style={w375}>
                  <b>⚠ 法令NG {m.ng_violations.length}件</b>
                  <ul>
                    {m.ng_violations.map((v, j) => (
                      <li key={j}>
                        {v.matched || ''}: {v.reason || ''}
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}
              {m.number_violations.length ? (
                <div style={w375}>
                  <NumberViolations items={m.number_violations} />
                </div>
              ) : null}
              {m.expression_warnings.length ? (
                <div style={w375}>
                  <ExprWarnings items={m.expression_warnings} />
                </div>
              ) : null}
              <NumberCheckNote check={m.number_check} />
            </div>
          );
        })}
      </div>
      <ConfirmBox stepKey="mobile" checked={confirmed} onChange={onConfirm} />
      {active && activePopup ? <IntentPopupBox popup={activePopup} anchor={active.el} /> : null}
    </>
  );
}
