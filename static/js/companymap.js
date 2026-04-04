/**
 * SalesNow企業マーカーレイヤー（postingMapの拡張）
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

            // クリック → 企業プロフィールをHTMX経由で表示
            marker.on("click", function() {
                var contentEl = document.getElementById("content");
                if (contentEl && typeof htmx !== "undefined") {
                    htmx.ajax("GET",
                        "/api/company/profile/" + encodeURIComponent(c.corporate_number),
                        { target: contentEl, swap: "innerHTML" }
                    );
                }
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
