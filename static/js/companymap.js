/**
 * 企業マーカーレイヤー（postingMapの拡張）
 * 地図タブ上で企業マーカーのトグル表示を提供する。
 */
(function() {
    var companyLayer = null;
    var companyMarkers = [];
    var isLoading = false;
    var isEnabled = false;

    // 企業マーカースタイル（エメラルドグリーン、四角形風）
    var companyStyle = {
        radius: 6,
        fillColor: "#10b981",
        color: "#fff",
        weight: 1.5,
        fillOpacity: 0.8,
        opacity: 1
    };

    function getMap() {
        return window.__jmMap || null;
    }

    function loadCompanyMarkers() {
        var map = getMap();
        if (!map || !isEnabled || isLoading) return;

        var zoom = map.getZoom();
        if (zoom < 10) {
            clearCompanyLayer();
            return;
        }

        var bounds = map.getBounds();
        var params = new URLSearchParams({
            south: bounds.getSouth(),
            north: bounds.getNorth(),
            west: bounds.getWest(),
            east: bounds.getEast(),
            zoom: zoom
        });

        isLoading = true;
        fetch("/api/jobmap/company-markers?" + params.toString())
            .then(function(r) { return r.json(); })
            .then(function(data) {
                renderCompanyMarkers(data.markers || []);
                var countEl = document.getElementById("jm-company-count");
                if (countEl) {
                    var shown = (data.markers || []).length;
                    var total = data.total || 0;
                    countEl.textContent = shown > 0
                        ? (shown < total ? shown + " / " + total + " 社" : shown + " 社")
                        : (data.zoom_required ? "zoom " + data.zoom_required + "+ で表示" : "");
                }
            })
            .catch(function(e) {
                console.warn("[companymap] load error:", e);
            })
            .finally(function() {
                isLoading = false;
            });
    }

    function renderCompanyMarkers(markers) {
        var map = getMap();
        if (!map) return;

        clearCompanyLayer();
        companyLayer = L.featureGroup();

        markers.forEach(function(c) {
            if (!c.lat || !c.lng) return;

            var marker = L.circleMarker([c.lat, c.lng], companyStyle);

            // ツールチップ
            var tip = escapeText(c.company_name);
            if (c.sn_industry) tip += "\n" + escapeText(c.sn_industry);
            if (c.employee_count > 0) tip += "\n従業員: " + c.employee_count.toLocaleString() + "人";
            marker.bindTooltip(tip, { direction: "top", offset: [0, -6] });

            // クリック → ポップアップに従業員推移ミニチャートを表示
            marker.on("click", function() {
                var map = getMap();
                if (!map) return;

                var corp = c.corporate_number;
                var popupId = "popup-chart-" + corp.replace(/\W/g, "_");

                // 仮のポップアップを先に表示（ローディング状態）
                var loadingHtml = '<div style="width:300px;background:#1e293b;color:#e2e8f0;padding:12px;border-radius:8px;">'
                    + '<div style="font-weight:bold;font-size:13px;">' + escapeText(c.company_name) + '</div>'
                    + '<div style="font-size:11px;color:#94a3b8;margin-top:4px;">読み込み中...</div>'
                    + '</div>';

                var popup = L.popup({ maxWidth: 320, className: "company-popup" })
                    .setLatLng([c.lat, c.lng])
                    .setContent(loadingHtml)
                    .openOn(map);

                // 企業詳細APIからdeltaデータを取得
                fetch("/api/v1/companies/" + encodeURIComponent(corp))
                    .then(function(r) { return r.json(); })
                    .then(function(detail) {
                        var d = detail || {};
                        var empCount = d.employee_count || c.employee_count || 0;
                        var industry = d.sn_industry || c.sn_industry || "";

                        // delta値を取得（nullの場合はNaNにしてチャートで欠損扱い）
                        var deltas = [
                            d.employee_delta_1m != null ? d.employee_delta_1m : null,
                            d.employee_delta_3m != null ? d.employee_delta_3m : null,
                            d.employee_delta_6m != null ? d.employee_delta_6m : null,
                            d.employee_delta_1y != null ? d.employee_delta_1y : null,
                            d.employee_delta_2y != null ? d.employee_delta_2y : null
                        ];
                        var hasDeltas = deltas.some(function(v) { return v !== null; });

                        var chartHtml = hasDeltas
                            ? '<div id="' + popupId + '" style="width:280px;height:120px;margin-top:8px;"></div>'
                            : '<div style="font-size:11px;color:#64748b;margin-top:8px;">従業員推移データなし</div>';

                        var html = '<div style="width:300px;background:#1e293b;color:#e2e8f0;padding:12px;border-radius:8px;">'
                            + '<div style="font-weight:bold;font-size:13px;">' + escapeText(d.company_name || c.company_name) + '</div>'
                            + '<div style="font-size:11px;color:#94a3b8;margin-top:2px;">'
                            +   (industry ? escapeText(industry) + ' | ' : '')
                            +   (empCount > 0 ? empCount.toLocaleString() + '人' : '従業員数不明')
                            + '</div>'
                            + chartHtml
                            + '<div style="margin-top:8px;text-align:right;">'
                            +   '<a href="#" style="color:#60a5fa;font-size:11px;text-decoration:none;" '
                            +     'onclick="(function(){var el=document.getElementById(\'content\');if(el&&typeof htmx!==\'undefined\'){'
                            +     'htmx.ajax(\'GET\',\'/api/company/profile/' + encodeURIComponent(corp) + '\',{target:el,swap:\'innerHTML\'});}return false;})();return false;"'
                            +   '>詳細を見る &rarr;</a>'
                            + '</div>'
                            + '</div>';

                        popup.setContent(html);

                        // EChartsの初期化はDOM描画後に実行
                        if (hasDeltas) {
                            setTimeout(function() {
                                var chartEl = document.getElementById(popupId);
                                if (!chartEl) return;
                                if (typeof echarts === "undefined") return;

                                var chart = echarts.init(chartEl);
                                chart.setOption({
                                    grid: { left: 40, right: 10, top: 10, bottom: 20 },
                                    xAxis: {
                                        type: "category",
                                        data: ["1M", "3M", "6M", "1Y", "2Y"],
                                        axisLabel: { fontSize: 10, color: "#94a3b8" },
                                        axisLine: { lineStyle: { color: "#334155" } },
                                        axisTick: { show: false }
                                    },
                                    yAxis: {
                                        type: "value",
                                        axisLabel: { fontSize: 10, color: "#94a3b8", formatter: "{value}%" },
                                        axisLine: { show: false },
                                        splitLine: { lineStyle: { color: "#334155", type: "dashed" } }
                                    },
                                    series: [{
                                        type: "line",
                                        data: deltas,
                                        smooth: true,
                                        symbol: "circle",
                                        symbolSize: 6,
                                        areaStyle: { color: "rgba(59,130,246,0.15)" },
                                        lineStyle: { color: "#3b82f6", width: 2 },
                                        itemStyle: {
                                            color: function(p) {
                                                return (p.data != null && p.data >= 0) ? "#22c55e" : "#ef4444";
                                            }
                                        }
                                    }]
                                });

                                // ポップアップが閉じられたらチャートを破棄
                                map.once("popupclose", function() {
                                    if (chart) {
                                        chart.dispose();
                                        chart = null;
                                    }
                                });
                            }, 50);
                        }
                    })
                    .catch(function(err) {
                        console.warn("[companymap] 企業詳細取得エラー:", err);
                        popup.setContent(
                            '<div style="width:300px;background:#1e293b;color:#e2e8f0;padding:12px;border-radius:8px;">'
                            + '<div style="font-weight:bold;font-size:13px;">' + escapeText(c.company_name) + '</div>'
                            + '<div style="font-size:11px;color:#ef4444;margin-top:4px;">データ取得に失敗しました</div>'
                            + '</div>'
                        );
                    });
            });

            companyLayer.addLayer(marker);
            companyMarkers.push(marker);
        });

        companyLayer.addTo(map);
    }

    function clearCompanyLayer() {
        var map = getMap();
        if (companyLayer && map) {
            map.removeLayer(companyLayer);
        }
        companyLayer = null;
        companyMarkers = [];
    }

    function escapeText(s) {
        if (!s) return "";
        var d = document.createElement("div");
        d.appendChild(document.createTextNode(s));
        return d.textContent;
    }

    // moveendイベントにフック
    var moveEndHooked = false;
    function hookMoveEnd() {
        if (moveEndHooked) return;
        var map = getMap();
        if (map) {
            map.on("moveend", function() {
                if (isEnabled) {
                    loadCompanyMarkers();
                }
            });
            moveEndHooked = true;
        }
    }

    // postingMapオブジェクトにtoggleCompanyLayerメソッドを追加
    function attachToggle() {
        if (typeof postingMap !== "undefined") {
            postingMap.toggleCompanyLayer = function(enabled) {
                isEnabled = enabled;
                if (enabled) {
                    hookMoveEnd();
                    loadCompanyMarkers();
                } else {
                    clearCompanyLayer();
                }
            };
        }
    }

    // 即時実行
    attachToggle();

    // postingMap初期化を待つ（遅延ロード対応）
    var initCheck = setInterval(function() {
        attachToggle();
        if (getMap()) {
            clearInterval(initCheck);
        }
    }, 1000);

    // 10秒でチェック停止
    setTimeout(function() { clearInterval(initCheck); }, 10000);
})();
