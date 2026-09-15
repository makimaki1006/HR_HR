/*!
 * 軸をドラッグして目盛りの幅を変える。
 *
 * 株のチャートにある操作と同じ。縦軸の数字のあたりを上下にドラッグすると、
 * 表示している値の範囲が広がったり狭まったりする。
 *
 * # なぜ要るのか
 * 1 本だけ桁の大きい系列があると、ほかの線が下に潰れて動きが読めない。
 * 実例:「求職者の内訳の移り変わり」は 1 本が 55〜60% を占め、残り 6 本が
 * 0〜3% に貼り付いて、どれがどう動いたか見分けられなかった。
 * 軸の範囲を手元で変えられれば、その場で見たいところを広げられる。
 *
 * # 作りの方針
 * * **図ごとに何も登録しない。** document の 1 か所で受けて、押された場所から
 *   図を逆引きする。htmx でタブを差し替えると app.js が図を作り直すので、
 *   図ごとに登録すると毎回付け直しが要る。それを避けている。
 * * **元に戻せる形だけを使う。** 変えるのは軸の min/max だけで、データには触らない。
 *   元の値は DOM に残っている data-chart-config から読み直すので、
 *   何回いじってもダブルクリックで確実に戻る。
 * * **図の設定に関数を入れない。** この画面の顧客レポート側には関数を復元する
 *   処理が無いため、設定を JSON のまま保つ決まりになっている。
 *   この操作は JS 側だけで完結していて、設定には何も足さない。
 */
(function () {
  'use strict';

  /** 軸とみなす帯の幅（px）。グリッドの外側のこの範囲を掴んだらドラッグ扱い */
  var BAND = 44;
  /** 1px ドラッグしたときの倍率。大きいほど敏感 */
  var SENS = 0.006;
  /** 縮めすぎ・広げすぎの歯止め（元の幅に対する倍率） */
  var MIN_ZOOM = 0.02;
  var MAX_ZOOM = 50;

  var drag = null;

  function inst(el) {
    return typeof echarts !== 'undefined' && el ? echarts.getInstanceByDom(el) : null;
  }

  /** 図の描画領域（grid）の矩形。取れない図（円グラフ・地図など）は null */
  function gridRect(chart) {
    try {
      var g = chart.getModel().getComponent('grid');
      var r = g && g.coordinateSystem && g.coordinateSystem.getRect();
      return r && typeof r.x === 'number' ? r : null;
    } catch (e) {
      return null;
    }
  }

  function asArray(v) {
    return v == null ? [] : Array.isArray(v) ? v : [v];
  }

  /**
   * 押された場所が「どの軸の帯か」を返す。
   * 返り値 {軸:'y'|'x', 番号:0|1} か null。
   */
  function hit(chart, el, ev) {
    var r = gridRect(chart);
    if (!r) return null;
    var box = el.getBoundingClientRect();
    var x = ev.clientX - box.left;
    var y = ev.clientY - box.top;
    var opt = chart.getOption();
    var ys = asArray(opt.yAxis);
    var xs = asArray(opt.xAxis);

    // 縦の帯（グリッドの左右）
    if (y >= r.y - 8 && y <= r.y + r.height + 8) {
      if (x < r.x && x >= r.x - BAND && ys[0] && ys[0].type === 'value') {
        return { 軸: 'y', 番号: 0 };
      }
      var right = r.x + r.width;
      if (x > right && x <= right + BAND && ys[1] && ys[1].type === 'value') {
        return { 軸: 'y', 番号: 1 };
      }
    }
    // 横の帯（グリッドの下）。横棒グラフのように x が数値のときだけ
    if (x >= r.x - 8 && x <= r.x + r.width + 8) {
      var bottom = r.y + r.height;
      if (y > bottom && y <= bottom + BAND && xs[0] && xs[0].type === 'value') {
        return { 軸: 'x', 番号: 0 };
      }
    }
    return null;
  }

  /** いま表示されている範囲。min/max が未指定なら実際に描かれている値から取る */
  function currentRange(chart, 軸, 番号) {
    var opt = chart.getOption();
    var a = asArray(軸 === 'y' ? opt.yAxis : opt.xAxis)[番号];
    if (!a) return null;
    var lo = a.min;
    var hi = a.max;
    if (typeof lo === 'number' && typeof hi === 'number') return { lo: lo, hi: hi };
    // 自動の軸は、内部が決めた目盛りの端を使う
    try {
      var ax = chart.getModel().getComponent(軸 + 'Axis', 番号).axis;
      var ext = ax.scale.getExtent();
      return { lo: ext[0], hi: ext[1] };
    } catch (e) {
      return null;
    }
  }

  /** 元の設定（data-chart-config）にあった min/max。戻すときに使う */
  function originalRange(el, 軸, 番号) {
    try {
      var c = JSON.parse(el.getAttribute('data-chart-config'));
      var a = asArray(軸 === 'y' ? c.yAxis : c.xAxis)[番号];
      if (!a) return null;
      return { lo: a.min, hi: a.max, interval: a.interval };
    } catch (e) {
      return null;
    }
  }

  function applyRange(chart, 軸, 番号, lo, hi, interval) {
    var patch = [];
    var n = asArray(chart.getOption()[軸 + 'Axis']).length;
    for (var i = 0; i < n; i++) {
      patch.push(i === 番号 ? { min: lo, max: hi, interval: interval } : {});
    }
    var o = {};
    o[軸 + 'Axis'] = patch;
    chart.setOption(o);
  }

  /**
   * 描画領域の内側を掴んだか。内側なら「上下に動かす」ほうの操作になる。
   *
   * 尺度を変えるだけでは足りない。実例で、縦軸 0〜80% の図を拡大しても
   * 中心（40%）のまわりが広がるだけで、見たかった 0〜1% の線のところまで
   * 行けなかった。株のチャートと同じで、**尺度を変える**操作と
   * **見る場所を動かす**操作の両方がいる。
   */
  function insideGrid(chart, el, ev) {
    var r = gridRect(chart);
    if (!r) return false;
    var box = el.getBoundingClientRect();
    var x = ev.clientX - box.left;
    var y = ev.clientY - box.top;
    return x > r.x && x < r.x + r.width && y > r.y && y < r.y + r.height;
  }

  document.addEventListener(
    'mousedown',
    function (ev) {
      if (ev.button !== 0) return;
      var el = ev.target.closest ? ev.target.closest('.echart') : null;
      if (!el) return;
      var chart = inst(el);
      if (!chart) return;

      var h = hit(chart, el, ev);
      if (h) {
        var cur = currentRange(chart, h.軸, h.番号);
        if (!cur || !(cur.hi > cur.lo)) return;
        // 掴んだ位置の値を基点にして広げ縮めする。
        //
        // 窓の中心を基点にすると、下のほうに潰れている線を見に行けない。
        // 実測で、縦軸 0〜80% の図を中心基点で拡大しても 31.6〜48.4% に
        // なるだけで、見たかった 0〜1% の線は画面の外にいたままだった。
        // 掴んだところを動かさずに広げれば、軸の下のほうを掴んで引くだけで
        // 低いところが開く。
        var rr = gridRect(chart);
        var bx = el.getBoundingClientRect();
        var t;
        if (h.軸 === 'y') {
          t = (rr.y + rr.height - (ev.clientY - bx.top)) / rr.height;
        } else {
          t = (ev.clientX - bx.left - rr.x) / rr.width;
        }
        t = Math.min(1, Math.max(0, t));
        drag = {
          型: '尺度',
          el: el,
          chart: chart,
          軸: h.軸,
          番号: h.番号,
          始点: h.軸 === 'y' ? ev.clientY : ev.clientX,
          基点: cur.lo + t * (cur.hi - cur.lo),
          lo: cur.lo,
          hi: cur.hi,
        };
        document.body.style.cursor = h.軸 === 'y' ? 'ns-resize' : 'ew-resize';
        ev.preventDefault();
        return;
      }

      // 領域の内側 → 上下に動かす。ただし縦軸が数値の図だけ
      if (!insideGrid(chart, el, ev)) return;
      var ys = asArray(chart.getOption().yAxis);
      if (!ys[0] || ys[0].type !== 'value') return;
      var c0 = currentRange(chart, 'y', 0);
      if (!c0 || !(c0.hi > c0.lo)) return;
      var r0 = gridRect(chart);
      drag = {
        型: '移動',
        el: el,
        chart: chart,
        軸: 'y',
        番号: 0,
        始点: ev.clientY,
        lo: c0.lo,
        hi: c0.hi,
        // 1px が値いくつ分か
        単位: (c0.hi - c0.lo) / r0.height,
        動いた: false,
      };
      // ここでは preventDefault しない。ただの click を邪魔しないため
    },
    true
  );

  document.addEventListener('mousemove', function (ev) {
    if (!drag) {
      // 帯の上に来たらカーソルを変えて、掴めることを伝える
      var el = ev.target.closest ? ev.target.closest('.echart') : null;
      if (!el) return;
      var chart = inst(el);
      if (!chart) return;
      var h = hit(chart, el, ev);
      el.style.cursor = h ? (h.軸 === 'y' ? 'ns-resize' : 'ew-resize') : '';
      return;
    }
    if (drag.型 === '移動') {
      var dy = ev.clientY - drag.始点;
      // 4px 動くまでは「掴んだだけ」とみなす。ただのクリックを壊さない
      if (!drag.動いた && Math.abs(dy) < 4) return;
      if (!drag.動いた) {
        drag.動いた = true;
        document.body.style.cursor = 'grabbing';
      }
      // 下へ引いたら、見ている窓は上へ動く（中身を引き下ろす感覚に合わせる）
      var shift = dy * drag.単位;
      applyRange(drag.chart, 'y', 0, drag.lo + shift, drag.hi + shift, null);
      ev.preventDefault();
      return;
    }
    var now = drag.軸 === 'y' ? ev.clientY : ev.clientX;
    var d = now - drag.始点;
    // 縦軸は下へ引くと範囲が広がる（＝線が平らになる）。株のチャートと同じ向き
    var f = Math.exp((drag.軸 === 'y' ? d : -d) * SENS);
    f = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, f));
    // 掴んだところ（基点）は動かさず、その上下だけを伸び縮みさせる
    applyRange(
      drag.chart,
      drag.軸,
      drag.番号,
      drag.基点 - (drag.基点 - drag.lo) * f,
      drag.基点 + (drag.hi - drag.基点) * f,
      null
    );
    ev.preventDefault();
  });

  function stop() {
    if (!drag) return;
    drag = null;
    document.body.style.cursor = '';
  }
  document.addEventListener('mouseup', stop);
  document.addEventListener('mouseleave', stop);

  // ダブルクリックで元に戻す。軸の帯でも、図の内側でも効く
  document.addEventListener('dblclick', function (ev) {
    var el = ev.target.closest ? ev.target.closest('.echart') : null;
    if (!el) return;
    var chart = inst(el);
    if (!chart) return;
    var h = hit(chart, el, ev);
    if (!h) {
      if (!insideGrid(chart, el, ev)) return;
      h = { 軸: 'y', 番号: 0 };
      var ys = asArray(chart.getOption().yAxis);
      if (!ys[0] || ys[0].type !== 'value') return;
    }
    // 元の値は DOM に残っている設定から読む。何回いじっても確実に戻る
    var o = originalRange(el, h.軸, h.番号);
    applyRange(chart, h.軸, h.番号, o ? o.lo : null, o ? o.hi : null, o ? o.interval : null);
    ev.preventDefault();
  });
})();
