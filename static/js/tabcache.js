// 2026-09-11: キャッシュ再生のあとに htmx.process() を呼ぶようにした。
// DOMParser + adoptNode で #content を差し替えたあと、合成した htmx:afterSettle を
// 投げるだけだったので、再生された中身の hx-get が htmx に登録されていなかった。
// htmx は afterSettle イベントでは要素を登録しない（登録するのは process だけ）。
// 登録されていない <a hx-get href> は素のリンクとして働くので、クリックすると
// ページ全体が遷移する。実測（2026-09-11、採用市場の職種一覧）:
//   サーバから来た一覧のリンク … htmx-internal-data=true  → document リクエスト 0 件
//   キャッシュ再生された一覧   … htmx-internal-data=false → document リクエスト 2 件
//     (/tab/indeed/title?name=一般事務 → /?tab=... へリダイレクト)
//   window に置いた目印がクリック後に消える＝ページが作り直されている
// 機能は壊れない（?tab= の復元経路を通って正しいページには着く）が、画面が
// 作り直され、document を 2 回取り直し、ページ内の状態が失われる。
// tabcache が効く全タブで起きる。
// 呼ぶ位置は afterSettle を投げる前。htmx 本来の swap も 登録 → afterSettle の順。
// 2026-09-10: 保存キーを「active なタブボタンの hx-get」から「実際のリクエスト URL」に変更。
// 参照側は e.detail.path（リクエスト URL）で引いているのに保存側だけボタンを見ていたため、
// 職種辞典で詳細 /tab/driver/123 を開くと active は /tab/driver のままで、
// 詳細ページの HTML が /tab/driver のキーに入っていた。その後 /tab/driver を素で開くと詳細が出る。
!function(){"use strict";function e(e,t){var n=document.createElement("div");return n.className="skeleton skeleton-card",n.style.height=e,t&&(n.style.width=t),n}var t={"/tab/jobmap":"map","/tab/competitive":"table"};window.showTabSkeleton=function(n){var a=document.getElementById("content");if(a){for(;a.firstChild;)a.removeChild(a.firstChild);var r=t[n]||"default";a.appendChild(function(t){var n=document.createElement("div");if(n.className="space-y-6",n.appendChild(e("24px","200px")),"map"===t)n.appendChild(e("500px"));else if("table"===t)n.appendChild(e("56px")),n.appendChild(e("400px"));else{var a=document.createElement("div");a.className="grid-stats";for(var r=0;r<4;r++)a.appendChild(e("88px"));n.appendChild(a);var i=document.createElement("div");i.className="grid-charts";for(var d=0;d<2;d++)i.appendChild(e("360px"));n.appendChild(i)}return n}(r))}};var n={},a=["/tab/jobmap","/tab/indeed"];function r(e){return e+"::"+function(){var e=[],t=[];document.querySelectorAll(".ind-major-cb:checked").forEach(function(t){e.push(t.value)}),document.querySelectorAll(".ind-sub-cb:checked").forEach(function(e){t.push(e.value)});var n=document.getElementById("pref-select"),a=document.getElementById("muni-select");return e.sort().join("+")+"|"+t.sort().join("+")+"|"+(n?n.value:"")+"|"+(a?a.value:"")}()}function i(e){return e&&0===e.indexOf("/tab/")}function d(e){for(var t=0;t<a.length;t++)if(a[t]===e)return!0;return!1}document.body.addEventListener("htmx:configRequest",function(e){var t=e.detail.path;if(i(t)&&!d(t)){var a=r(t),o=n[a];if(o&&Date.now()-o.timestamp<18e5){e.preventDefault();var l=document.getElementById("content");if(l){!function(e){if("undefined"!=typeof echarts)for(var t=e.querySelectorAll(".echart"),n=0;n<t.length;n++){var a=echarts.getInstanceByDom(t[n]);if(a)try{a.dispose()}catch(e){}}}(l);for(var c=(new DOMParser).parseFromString(o.html,"text/html");l.firstChild;)l.removeChild(l.firstChild);for(var u=c.body.childNodes;u.length>0;)l.appendChild(document.adoptNode(u[0]));if(window.htmx&&window.htmx.process)window.htmx.process(l);var s=new CustomEvent("htmx:afterSettle",{bubbles:!0,detail:{target:l}});l.dispatchEvent(s)}}}}),document.body.addEventListener("htmx:beforeSwap",function(e){var t=e.detail.serverResponse;if(t){var o=e.detail.requestConfig&&e.detail.requestConfig.path||null;if(i(o)&&!d(o)){var l=r(o);n[l]={html:t,timestamp:Date.now()}}}}),window.clearTabCache=function(){n={}}}();