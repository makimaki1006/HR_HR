//! Offline SVG series. A missing observation breaks the line; observed zero remains zero.
use crate::handlers::helpers::escape_html;

/// 1 か月分の観測値。未取得は None (0 にしない)。
pub(super) type Point<'a> = (&'a str, Option<f64>);

pub(super) fn chart(
    html: &mut String,
    label: &str,
    rows: &[Point<'_>],
    unit: &str,
    color: &str,
    ratio: bool,
) {
    let values: Vec<_> = rows
        .iter()
        .map(|(_, v)| v.filter(|v| v.is_finite() && *v >= 0.0))
        .collect();
    let Some(peak) = values.iter().flatten().copied().reduce(f64::max) else {
        return;
    };
    let step = if ratio {
        (peak / 4.0 * 10.0).ceil().max(1.0) / 10.0
    } else {
        (peak / 4.0).ceil().max(1.0)
    };
    let ymax = step * 4.0;
    let fmt = |v: f64| {
        if ratio {
            format!("{v:.2}")
        } else {
            crate::handlers::helpers::format_number(v as i64)
        }
    };
    html.push_str(&format!(
        "<figure class=\"trend-card\"><figcaption>{}</figcaption>",
        escape_html(label)
    ));
    if let Some(last) = rows.last() {
        let value = values
            .last()
            .copied()
            .flatten()
            .map(&fmt)
            .unwrap_or_else(|| "—".into());
        html.push_str(&format!(
            "<p class=\"trend-value\">最終収録月 {}：{value} {}</p>",
            escape_html(if last.0.is_empty() { "—" } else { last.0 }),
            escape_html(unit)
        ));
    }
    html.push_str(&format!("<div class=\"trend-plot\"><svg viewBox=\"0 0 900 280\" role=\"img\" aria-label=\"{}\"><title>{}</title>",escape_html(label),escape_html(label)));
    for i in 0..=4 {
        let y = 24.0 + 212.0 * (1.0 - i as f64 / 4.0);
        html.push_str(&format!("<path d=\"M85 {y} H876\" stroke=\"#e1e8ec\"/><text x=\"75\" y=\"{}\" font-size=\"14\" text-anchor=\"end\">{}</text>", y+5.0,fmt(step*i as f64)));
    }
    let mut points = Vec::new();
    let flush = |points: &mut Vec<String>, html: &mut String| {
        if points.len() > 1 {
            html.push_str(&format!("<polyline class=\"trend-line\" points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"3\"/>",points.join(" ")));
        }
        points.clear();
    };
    for (i, row) in rows.iter().enumerate() {
        let x = 85.0 + 791.0 * i as f64 / (rows.len().saturating_sub(1).max(1)) as f64;
        let month = escape_html(if row.0.is_empty() { "—" } else { row.0 });
        if let Some(value) = values[i] {
            let y = 24.0 + 212.0 * (1.0 - value / ymax);
            points.push(format!("{x:.3},{y:.3}"));
            html.push_str(&format!("<circle cx=\"{x:.3}\" cy=\"{y:.3}\" r=\"4\" fill=\"white\" stroke=\"{color}\" stroke-width=\"2\" data-month=\"{month}\" data-value=\"{value}\"><title>{month}：{} {}</title></circle>",fmt(value),escape_html(unit)));
        } else {
            flush(&mut points, html);
        }
        if i % rows.len().div_ceil(8).max(1) == 0 || i == rows.len() - 1 {
            html.push_str(&format!(
                "<text x=\"{x}\" y=\"268\" text-anchor=\"middle\" font-size=\"14\">{month}</text>"
            ));
        }
    }
    flush(&mut points, html);
    html.push_str("</svg></div></figure>");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_breaks_line_and_zero_is_retained() {
        let rows = vec![
            ("2026-01", Some(0.0)),
            ("2026-02", None),
            ("2026-03", Some(4.0)),
        ];
        let mut html = String::new();
        chart(&mut html, "<test>", &rows, "件", "#007d79", false);
        assert!(html.contains("data-month=\"2026-01\" data-value=\"0\""));
        assert!(html.contains("cx=\"876.000\" cy=\"24.000\""));
        assert!(!html.contains("polyline"));
        assert!(!html.contains("<test>"));
        assert_eq!(html.matches("<circle").count(), 2);
    }

    #[test]
    fn axis_is_zero_based_and_values_are_exact() {
        let rows = vec![("2026-01", Some(1.0)), ("2026-02", Some(2.5))];
        let mut html = String::new();
        chart(&mut html, "ratio", &rows, "人/求人", "#007d79", true);
        assert!(html.contains("data-value=\"2.5\""));
        assert!(html.contains("points=\"85.000,160.286 876.000,46.714\""));
        assert!(html.contains(">0.00</text>"));
    }
}
