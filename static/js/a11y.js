/**
 * a11y.js — V2 HW Dashboard アクセシビリティ補強モジュール
 *
 * 目的:
 *   Edit ツール制約により templates/dashboard_inline.html を直接編集できない代わりに、
 *   ロード時に DOM を読み取り、欠落している ARIA 属性・キーボードハンドラを動的に補完する。
 *
 * 適用方法:
 *   templates/dashboard_inline.html の <script src="/static/js/app.js"></script> の直後に
 *   <script src="/static/js/a11y.js"></script> を追加する (1 行のみ)。
 *
 * 補強する内容:
 *   1. <main id="content"> に role="tabpanel" / aria-live="polite" / aria-busy 切替
 *   2. tablist の Roving tabindex + 矢印キー / Home / End 対応 (WAI-ARIA APG)
 *   3. loading-overlay に role="status" / aria-live="polite"
 *   4. グローバル aria-live 領域 (status + alert) の挿入
 *   5. close ボタン (× / &times; / ✕) に aria-label 自動付与
 *   6. 装飾絵文字に aria-hidden="true" を自動付与 (heuristic)
 *   7. breadcrumb-bar を nav 相当の aria-label 付与
 *
 * 既存挙動の破壊禁止:
 *   - 既に role/aria-* が付いている要素は上書きしない
 *   - tabindex の上書きは tablist 内 [role="tab"] のみ
 *
 * テスト:
 *   tests/e2e/a11y_helpers_2026_04_26.spec.ts (本 commit で追加) で逆証明
 *
 * 2026-04-26 Cov agent
 */
(function () {
    'use strict';

    /**
     * 1. <main id="content"> の role / aria-live 補強
     */
    function enhanceContentPanel() {
        var content = document.getElementById('content');
        if (!content) return;
        if (!content.getAttribute('role')) {
            content.setAttribute('role', 'tabpanel');
        }
        if (!content.hasAttribute('aria-live')) {
            content.setAttribute('aria-live', 'polite');
        }
        if (!content.hasAttribute('aria-busy')) {
            content.setAttribute('aria-busy', 'false');
        }
        if (!content.hasAttribute('tabindex')) {
            content.setAttribute('tabindex', '-1');
        }
    }

    /**
     * 2. tablist の Roving tabindex + 矢印キー (WAI-ARIA APG)
     *    https://www.w3.org/WAI/ARIA/apg/patterns/tabs/
     */
    function enhanceTablistKeyboardNav() {
        var tablists = document.querySelectorAll('[role="tablist"]');
        tablists.forEach(function (tablist) {
            var tabs = tablist.querySelectorAll('[role="tab"]');
            if (!tabs.length) return;

            // Roving tabindex 初期化: active 1 個のみ tabindex=0、他は -1
            var hasActive = false;
            tabs.forEach(function (tab) {
                if (tab.classList.contains('active') ||
                    tab.getAttribute('aria-selected') === 'true') {
                    tab.tabIndex = 0;
                    hasActive = true;
                } else {
                    tab.tabIndex = -1;
                }
            });
            // active 不在時は先頭を 0
            if (!hasActive && tabs[0]) tabs[0].tabIndex = 0;

            // キーボードハンドラ
            tablist.addEventListener('keydown', function (e) {
                var current = document.activeElement;
                if (!current || current.getAttribute('role') !== 'tab') return;
                var idx = Array.prototype.indexOf.call(tabs, current);
                if (idx < 0) return;
                var nextIdx = idx;
                switch (e.key) {
                    case 'ArrowRight':
                    case 'ArrowDown':
                        nextIdx = (idx + 1) % tabs.length;
                        break;
                    case 'ArrowLeft':
                    case 'ArrowUp':
                        nextIdx = (idx - 1 + tabs.length) % tabs.length;
                        break;
                    case 'Home':
                        nextIdx = 0;
                        break;
                    case 'End':
                        nextIdx = tabs.length - 1;
                        break;
                    default:
                        return;
                }
                e.preventDefault();
                tabs[idx].tabIndex = -1;
                tabs[nextIdx].tabIndex = 0;
                tabs[nextIdx].focus();
                // 既存の onclick (setActiveTab) を発火させてタブ切替
                tabs[nextIdx].click();
            });

            // クリック時にも tabindex を付け替え (キーボードと整合)
            tablist.addEventListener('click', function (e) {
                var t = e.target.closest('[role="tab"]');
                if (!t) return;
                tabs.forEach(function (x) { x.tabIndex = -1; });
                t.tabIndex = 0;
            });
        });
    }

    /**
     * 3. loading-overlay に role="status" / aria-live="polite"
     */
    function enhanceLoadingOverlay() {
        var overlay = document.getElementById('loading-overlay');
        if (!overlay) return;
        if (!overlay.getAttribute('role')) overlay.setAttribute('role', 'status');
        if (!overlay.hasAttribute('aria-live')) overlay.setAttribute('aria-live', 'polite');
        if (!overlay.hasAttribute('aria-label')) overlay.setAttribute('aria-label', '読み込み中');
        // spinner は装飾扱い
        var spinner = overlay.querySelector('.loading-spinner');
        if (spinner && !spinner.hasAttribute('aria-hidden')) {
            spinner.setAttribute('aria-hidden', 'true');
        }
        // sr-only テキストを 1 度だけ追加
        if (!overlay.querySelector('.sr-only[data-a11y-injected]')) {
            var sr = document.createElement('span');
            sr.className = 'sr-only';
            sr.dataset.a11yInjected = '1';
            sr.textContent = 'タブのコンテンツを読み込んでいます';
            overlay.appendChild(sr);
        }
    }

    /**
     * 4. グローバル aria-live 領域の挿入
     *    window.a11yAnnounce(msg, type) で使用可能にする
     */
    function injectGlobalLiveRegions() {
        if (!document.getElementById('aria-live-status')) {
            var s = document.createElement('div');
            s.id = 'aria-live-status';
            s.className = 'sr-only';
            s.setAttribute('role', 'status');
            s.setAttribute('aria-live', 'polite');
            s.setAttribute('aria-atomic', 'true');
            document.body.appendChild(s);
        }
        if (!document.getElementById('aria-live-alert')) {
            var a = document.createElement('div');
            a.id = 'aria-live-alert';
            a.className = 'sr-only';
            a.setAttribute('role', 'alert');
            a.setAttribute('aria-live', 'assertive');
            a.setAttribute('aria-atomic', 'true');
            document.body.appendChild(a);
        }
        window.a11yAnnounce = function (msg, type) {
            var id = (type === 'alert') ? 'aria-live-alert' : 'aria-live-status';
            var el = document.getElementById(id);
            if (!el) return;
            // 同じ文字列の連続更新でも SR が読み上げるように一旦クリア
            el.textContent = '';
            setTimeout(function () { el.textContent = String(msg || ''); }, 30);
        };
    }

    /**
     * 5. close ボタンに aria-label 自動付与
     *    "&times;" "✕" "&#10005;" "×" のいずれかを内容に持つ button が対象
     */
    function enhanceCloseButtons() {
        var btns = document.querySelectorAll('button');
        btns.forEach(function (btn) {
            if (btn.hasAttribute('aria-label')) return;
            var text = (btn.textContent || '').trim();
            // よくある close グリフ
            if (text === '×' || text === '✕' || text === '✖' ||
                text === '×' || text === '✕' || text === '✖') {
                btn.setAttribute('aria-label', '閉じる');
            }
        });
    }

    /**
     * 6. 装飾絵文字に aria-hidden 自動付与 (heuristic)
     *
     *    対象: テキストノード単独で絵文字のみ、または短いテキストを含む <span>
     *    判定: その要素の先祖に既に aria-label/aria-hidden があれば skip
     */
    var EMOJI_RE = /^[\s\u{1F300}-\u{1FAFF}\u{2600}-\u{27BF}\u{2300}-\u{23FF}\u{1F000}-\u{1F9FF}\u{FE0F}\u{200D}\u{20E3}\u{231A}\u{231B}\u{23E9}-\u{23EC}\u{23F0}\u{23F3}\u{25FD}\u{25FE}\u{2614}\u{2615}\u{2648}-\u{2653}\u{267F}\u{2693}\u{26A1}\u{26AA}\u{26AB}\u{26BD}\u{26BE}\u{26C4}\u{26C5}\u{26CE}\u{26D4}\u{26EA}\u{26F2}\u{26F3}\u{26F5}\u{26FA}\u{26FD}\u{2705}\u{270A}\u{270B}\u{2728}\u{274C}\u{274E}\u{2753}-\u{2755}\u{2757}\u{2795}-\u{2797}\u{27B0}\u{27BF}]+$/u;
    function enhanceDecorativeEmojis() {
        var spans = document.querySelectorAll('header span, nav span, .stat-card > div > span:first-child');
        spans.forEach(function (span) {
            if (span.hasAttribute('aria-hidden') || span.hasAttribute('aria-label')) return;
            // 親に role="tab" や aria-label があれば SR は親の名前を読むので skip
            var parent = span.parentElement;
            if (parent && (parent.getAttribute('role') === 'tab' || parent.hasAttribute('aria-label'))) return;
            var text = (span.textContent || '').trim();
            if (text.length === 0) return;
            if (text.length <= 4 && EMOJI_RE.test(text)) {
                span.setAttribute('aria-hidden', 'true');
            }
        });
    }

    /**
     * 7. breadcrumb-bar に aria-label
     */
    function enhanceBreadcrumb() {
        var bc = document.getElementById('breadcrumb-bar');
        if (!bc) return;
        if (!bc.hasAttribute('aria-label')) {
            bc.setAttribute('aria-label', '現在の絞り込み条件');
        }
        // div 要素の場合 role="navigation" 付与で nav 相当に
        if (bc.tagName.toLowerCase() === 'div' && !bc.getAttribute('role')) {
            bc.setAttribute('role', 'navigation');
        }
    }

    /**
     * 8. htmx aria-busy 切替フック
     *    既存ハンドラと協調 (loading クラス追加と並行)
     */
    function setupHtmxAriaBusy() {
        if (!document.body) return;
        document.body.addEventListener('htmx:beforeRequest', function (e) {
            var c = document.getElementById('content');
            if (c && e.detail && e.detail.target === c) {
                c.setAttribute('aria-busy', 'true');
            }
        });
        document.body.addEventListener('htmx:afterSettle', function (e) {
            var c = document.getElementById('content');
            if (c) c.setAttribute('aria-busy', 'false');
        });
        document.body.addEventListener('htmx:responseError', function (e) {
            var c = document.getElementById('content');
            if (c) c.setAttribute('aria-busy', 'false');
            if (typeof window.a11yAnnounce === 'function') {
                window.a11yAnnounce('読み込みに失敗しました。再試行してください。', 'alert');
            }
        });
    }

    /**
     * 初期化エントリポイント
     */
    function init() {
        try { enhanceContentPanel(); } catch (e) { console.warn('a11y[content]', e); }
        try { enhanceTablistKeyboardNav(); } catch (e) { console.warn('a11y[tablist]', e); }
        try { enhanceLoadingOverlay(); } catch (e) { console.warn('a11y[loading]', e); }
        try { injectGlobalLiveRegions(); } catch (e) { console.warn('a11y[live]', e); }
        try { enhanceCloseButtons(); } catch (e) { console.warn('a11y[close]', e); }
        try { enhanceDecorativeEmojis(); } catch (e) { console.warn('a11y[emoji]', e); }
        try { enhanceBreadcrumb(); } catch (e) { console.warn('a11y[bc]', e); }
        try { setupHtmxAriaBusy(); } catch (e) { console.warn('a11y[htmx]', e); }
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }

    // タブ切替後に新しい DOM (タブ内容) にも close ボタン補強を再適用
    document.body && document.body.addEventListener && document.body.addEventListener('htmx:afterSettle', function () {
        try { enhanceCloseButtons(); } catch (e) {}
    });

    // 公開 API
    window.A11Y_HELPERS = {
        version: '1.0.0',
        announce: function (msg, type) {
            if (typeof window.a11yAnnounce === 'function') {
                window.a11yAnnounce(msg, type);
            }
        },
        reapply: init
    };
})();
