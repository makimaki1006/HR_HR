/**
 * choropleth_overlay.js
 * 求人地図タブにデータレイヤー（コロプレス）オーバーレイ機能を追加する。
 *
 * 既存の postingMap.js は minified のため直接変更せず、
 * DOM上の Leaflet マップインスタンスを取得して独立レイヤーとして管理する。
 *
 * API: GET /api/jobmap/choropleth?layer={layerId}&prefecture={pref}
 * レスポンス: { choropleth, legend, geojsonUrl, layer, prefecture, count }
 */
var choroplethOverlay = (function () {
    'use strict';

    // --- 内部状態 ---
    var geoLayer = null;       // L.geoJSON レイヤー（コロプレスポリゴン）
    var legendDiv = null;      // 凡例表示用 DOM 要素
    var currentLayer = '';     // 現在選択中のレイヤーID
    var currentPref = '';      // 最後に取得した都道府県
    var loading = false;       // 通信中フラグ（多重リクエスト防止）

    // --- Leaflet マップインスタンス取得 ---

    /**
     * jm-map 要素に紐づいた Leaflet マップインスタンスを取得する。
     * Leaflet は L.map() 呼び出し時に DOM 要素に _leaflet_id を設定し、
     * 内部レジストリでインスタンスを管理している。
     * ただし L.Map._instances は公開 API ではないため、
     * 複数のフォールバック方式を試行する。
     */
    function getMap() {
        // 方式0: postingmap.js パッチによるグローバル参照（最も確実）
        if (window.__jmMap && typeof window.__jmMap.addLayer === 'function') {
            return window.__jmMap;
        }

        var el = document.getElementById('jm-map');
        if (!el) return null;

        // 方式1: __choroplethMapRef（テンプレートで設定）
        if (el.__choroplethMapRef) return el.__choroplethMapRef;

        // 方式2: DOM 要素のプロパティを走査して Leaflet map オブジェクトを特定
        for (var key in el) {
            if (!el.hasOwnProperty(key)) continue;
            try {
                var val = el[key];
                if (val && typeof val === 'object' &&
                    typeof val._zoom !== 'undefined' &&
                    typeof val._layers !== 'undefined' &&
                    typeof val.addLayer === 'function') {
                    return val;
                }
            } catch (e) {
                // 一部ブラウザでアクセス不可のプロパティがある
            }
        }

        // 方式3: choropleth 初期化時に外部から設定された参照
        if (el.__choroplethMapRef) {
            return el.__choroplethMapRef;
        }

        return null;
    }

    /**
     * 現在選択中の都道府県を jm-pref セレクト要素から取得する。
     */
    function getSelectedPrefecture() {
        var sel = document.getElementById('jm-pref');
        if (!sel) return '';
        return sel.value || '';
    }

    // --- コロプレスレイヤー切り替え ---

    /**
     * 指定されたレイヤーIDでコロプレスを表示/切替する。
     * layerId が空文字の場合はレイヤーを削除してマーカーのみ表示に戻る。
     *
     * @param {string} layerId - 'posting_count', 'avg_salary', 'day_night_ratio', 等
     */
    function switchLayer(layerId) {
        currentLayer = layerId;

        // レイヤー選択を同期（ドロップダウンの値が外部から変更された場合）
        var sel = document.getElementById('jm-choropleth-layer');
        if (sel && sel.value !== layerId) {
            sel.value = layerId;
        }

        // 空選択ならレイヤーを除去して凡例も非表示
        if (!layerId) {
            removeLayer();
            hideLegend();
            return;
        }

        // 都道府県が未選択なら警告メッセージ
        var pref = getSelectedPrefecture();
        if (!pref) {
            showLegendMessage('都道府県を選択してからデータレイヤーを切り替えてください。');
            return;
        }

        // 多重リクエスト防止
        if (loading) return;
        loading = true;

        // ローディング表示
        showLegendMessage('読み込み中...');

        var url = '/api/jobmap/choropleth?layer=' +
            encodeURIComponent(layerId) +
            '&prefecture=' + encodeURIComponent(pref);

        fetch(url)
            .then(function (res) { return res.json(); })
            .then(function (data) {
                loading = false;
                currentPref = pref;

                if (data.error) {
                    showLegendMessage(data.error);
                    return;
                }

                var choropleth = data.choropleth || {};
                var legend = data.legend || [];
                var geojsonUrl = data.geojsonUrl || '';

                // choropleth データが空
                if (Object.keys(choropleth).length === 0) {
                    showLegendMessage('データがありません（' + layerId + '）');
                    removeLayer();
                    return;
                }

                if (!geojsonUrl) {
                    showLegendMessage('GeoJSONデータが見つかりません。');
                    removeLayer();
                    return;
                }

                // GeoJSON を取得してレイヤーを描画
                applyGeoJSON(geojsonUrl, choropleth, legend);
            })
            .catch(function (err) {
                loading = false;
                console.warn('[choropleth_overlay] fetch error:', err);
                showLegendMessage('読み込みエラー: ' + (err.message || '不明'));
            });
    }

    /**
     * GeoJSON を取得し、choropleth スタイルを適用してマップに追加する。
     *
     * @param {string} geojsonUrl - GeoJSON の URL パス
     * @param {Object} choropleth - 市区町村名 → スタイル情報のマップ
     * @param {Array} legend - 凡例配列 [{color, label}]
     */
    function applyGeoJSON(geojsonUrl, choropleth, legend) {
        var map = getMap();
        if (!map) {
            showLegendMessage('マップの初期化を待っています。しばらくお待ちください。');
            // マップ未初期化の場合、リトライ
            setTimeout(function () {
                var m = getMap();
                if (m) {
                    applyGeoJSON(geojsonUrl, choropleth, legend);
                } else {
                    showLegendMessage('マップインスタンスが見つかりません。');
                }
            }, 500);
            return;
        }

        fetch(geojsonUrl)
            .then(function (res) { return res.json(); })
            .then(function (geojsonData) {
                // 既存のコロプレスレイヤーを除去
                removeLayer();

                // 新規 L.geoJSON レイヤーを作成
                geoLayer = L.geoJSON(geojsonData, {
                    style: function (feature) {
                        // GeoJSON の properties から市区町村名を取得
                        var name = feature.properties.name ||
                                   feature.properties.N03_004 ||
                                   feature.properties.N03_003 ||
                                   '';
                        var style = choropleth[name];
                        if (style) {
                            return {
                                fillColor: style.fillColor || '#1e3a5f',
                                weight: style.weight || 1,
                                opacity: 1,
                                color: style.color || '#475569',
                                fillOpacity: style.fillOpacity || 0.5
                            };
                        }
                        // データ対象外の市区町村はグレーの薄い表示
                        return {
                            fillColor: '#374151',
                            weight: 0.5,
                            opacity: 0.5,
                            color: '#4b5563',
                            fillOpacity: 0.15
                        };
                    },
                    onEachFeature: function (feature, layer) {
                        var name = feature.properties.name ||
                                   feature.properties.N03_004 ||
                                   feature.properties.N03_003 ||
                                   '不明';
                        var style = choropleth[name];
                        var tooltipContent = name;

                        if (style && typeof style.value !== 'undefined') {
                            tooltipContent += ': ' + formatValue(style.value, currentLayer);
                        }

                        layer.bindTooltip(tooltipContent, {
                            sticky: true,
                            direction: 'top',
                            className: 'choropleth-tooltip'
                        });

                        // クリックでその市区町村を選択
                        layer.on('click', function () {
                            var muniSel = document.getElementById('jm-muni');
                            if (muniSel) {
                                // 該当する option を探して選択
                                for (var i = 0; i < muniSel.options.length; i++) {
                                    if (muniSel.options[i].value === name ||
                                        muniSel.options[i].text === name) {
                                        muniSel.selectedIndex = i;
                                        break;
                                    }
                                }
                            }
                        });
                    }
                });

                // ポリゴンを既存マーカーの下に配置するため pane を指定
                // Leaflet のデフォルト pane 順: tile(200) < overlay(400) < marker(600)
                // geoLayer は overlayPane に入るので、マーカーの下に表示される
                geoLayer.addTo(map);

                // 都道府県にズーム（離島除外版）
                // GeoJSONから緯度34-37, 経度138-141 内のフィーチャーのみでboundsを計算
                // （東京都の小笠原諸島等を除外するため）
                try {
                    var mainlandBounds = null;
                    geoLayer.eachLayer(function(layer) {
                        if (!layer.getBounds) return;
                        var b = layer.getBounds();
                        if (!b.isValid()) return;
                        var c = b.getCenter();
                        // 本土フィーチャーのみ（離島除外）
                        // 緯度33以上（伊豆諸島・小笠原除外）、経度128-146（南鳥島除外）
                        // 沖縄(lat 26)は全体のlatSpanが小さいので別途フォールバック
                        if (c.lat >= 33 && c.lat <= 46 && c.lng >= 128 && c.lng <= 146) {
                            if (!mainlandBounds) {
                                mainlandBounds = L.latLngBounds(b.getSouthWest(), b.getNorthEast());
                            } else {
                                mainlandBounds.extend(b);
                            }
                        }
                    });
                    if (mainlandBounds && mainlandBounds.isValid()) {
                        map.fitBounds(mainlandBounds, { padding: [30, 30], maxZoom: 13 });
                    } else {
                        // 本土フィルタでマッチしない場合（沖縄等）はフィルタなしのboundsを使用
                        var rawBounds = geoLayer.getBounds();
                        if (rawBounds && rawBounds.isValid()) {
                            map.fitBounds(rawBounds, { padding: [30, 30], maxZoom: 12 });
                        }
                    }
                } catch (e) { /* bounds計算失敗は無視 */ }

                // 凡例を更新
                updateLegend(legend);
            })
            .catch(function (err) {
                console.warn('[choropleth_overlay] GeoJSON fetch error:', err);
                showLegendMessage('GeoJSON読み込みエラー');
            });
    }

    /**
     * 値をレイヤーの種類に応じてフォーマットする。
     *
     * @param {number} value - 数値
     * @param {string} layerId - レイヤーID
     * @returns {string} フォーマット済み文字列
     */
    function formatValue(value, layerId) {
        switch (layerId) {
            case 'posting_count':
                return Math.round(value).toLocaleString() + '件';
            case 'avg_salary':
                if (value >= 10000) {
                    return (value / 10000).toFixed(1) + '万円';
                }
                return Math.round(value).toLocaleString() + '円';
            case 'day_night_ratio':
                return value.toFixed(1) + '%';
            case 'net_migration':
                var prefix = value >= 0 ? '+' : '';
                return prefix + value.toFixed(2);
            case 'vacancy_rate':
                return value.toFixed(1) + '%';
            case 'min_wage':
                return Math.round(value).toLocaleString() + '円/時';
            default:
                return value.toFixed(1);
        }
    }

    // --- レイヤー管理 ---

    /**
     * 現在のコロプレスレイヤーをマップから除去する。
     */
    function removeLayer() {
        if (geoLayer) {
            var map = getMap();
            if (map) {
                try { map.removeLayer(geoLayer); } catch (e) { /* 安全に無視 */ }
            }
            geoLayer = null;
        }
    }

    // --- 凡例管理 ---

    /**
     * 凡例コンテナの DOM 要素を取得（遅延取得）。
     */
    function getLegendDiv() {
        if (!legendDiv) {
            legendDiv = document.getElementById('jm-choropleth-legend');
        }
        return legendDiv;
    }

    /**
     * 凡例を更新する。
     *
     * @param {Array} legend - [{color, label}] の配列
     */
    function updateLegend(legend) {
        var div = getLegendDiv();
        if (!div) return;

        // コンテンツをクリア
        while (div.firstChild) {
            div.removeChild(div.firstChild);
        }

        if (!legend || legend.length === 0) {
            div.style.display = 'none';
            return;
        }

        // タイトル行
        var layerNames = {
            'posting_count': '求人件数',
            'avg_salary': '平均月給',
            'day_night_ratio': '昼夜間人口比率',
            'net_migration': '純移動率',
            'vacancy_rate': '欠員率',
            'min_wage': '最低賃金'
        };
        var title = document.createElement('div');
        title.className = 'font-medium text-slate-200 mb-1.5 text-xs';
        title.textContent = layerNames[currentLayer] || 'データレイヤー';
        div.appendChild(title);

        // 凡例アイテム
        for (var i = 0; i < legend.length; i++) {
            var item = legend[i];
            var row = document.createElement('div');
            row.className = 'flex items-center gap-2 mb-0.5';

            var swatch = document.createElement('span');
            swatch.style.cssText =
                'display:inline-block;width:14px;height:10px;border-radius:2px;border:1px solid #475569;flex-shrink:0;';
            swatch.style.backgroundColor = item.color || '#999';

            var label = document.createElement('span');
            label.className = 'text-slate-300';
            label.textContent = item.label || '';

            row.appendChild(swatch);
            row.appendChild(label);
            div.appendChild(row);
        }

        div.style.display = 'block';
    }

    /**
     * 凡例にメッセージを表示する。
     *
     * @param {string} msg - 表示メッセージ
     */
    function showLegendMessage(msg) {
        var div = getLegendDiv();
        if (!div) return;

        while (div.firstChild) {
            div.removeChild(div.firstChild);
        }

        var p = document.createElement('div');
        p.className = 'text-slate-400 text-xs';
        p.textContent = msg;
        div.appendChild(p);
        div.style.display = 'block';
    }

    /**
     * 凡例を非表示にする。
     */
    function hideLegend() {
        var div = getLegendDiv();
        if (div) {
            div.style.display = 'none';
        }
    }

    // --- 都道府県変更の監視 ---

    /**
     * 都道府県ドロップダウンの変更を監視して、
     * コロプレスが表示中であれば自動的にリフレッシュする。
     */
    function watchPrefectureChange() {
        var prefSel = document.getElementById('jm-pref');
        if (!prefSel) return;

        // 既に監視ハンドラが設定済みか確認
        if (prefSel.__choroplethWatcher) return;
        prefSel.__choroplethWatcher = true;

        prefSel.addEventListener('change', function () {
            if (currentLayer) {
                // 少し遅延を入れて市区町村リストの更新を待つ
                setTimeout(function () {
                    switchLayer(currentLayer);
                }, 300);
            }
        });
    }

    // --- 初期化（DOMContentLoaded / HTMXタブ切り替え対応） ---

    function init() {
        legendDiv = null; // キャッシュをリセット
        watchPrefectureChange();

        // ドロップダウンの状態をリセット
        var sel = document.getElementById('jm-choropleth-layer');
        if (sel) {
            sel.value = '';
        }
        currentLayer = '';
        hideLegend();
        removeLayer();
    }

    // HTMX のタブ切り替え後に初期化
    document.body.addEventListener('htmx:afterSettle', function (evt) {
        var target = evt.detail.target || document;
        if (target.querySelector && target.querySelector('#jm-map')) {
            setTimeout(init, 200);
        }
    });

    // DOMContentLoaded 時の初期化
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', function () {
            if (document.getElementById('jm-map')) {
                setTimeout(init, 200);
            }
        });
    }

    // --- 公開 API ---
    return {
        /**
         * レイヤーを切り替える。
         * @param {string} layerId - レイヤーID（空文字でオフ）
         */
        switchLayer: switchLayer,

        /**
         * 現在のレイヤーを再取得して更新する。
         * 都道府県や産業フィルタが変更された後に呼ぶ。
         */
        refresh: function () {
            if (currentLayer) {
                switchLayer(currentLayer);
            }
        },

        /**
         * レイヤーを除去して初期状態に戻す。
         */
        clear: function () {
            currentLayer = '';
            removeLayer();
            hideLegend();
            var sel = document.getElementById('jm-choropleth-layer');
            if (sel) sel.value = '';
        },

        /**
         * 手動初期化（タブ切り替え後に呼ばれる場合に使用）。
         */
        init: init
    };
})();
