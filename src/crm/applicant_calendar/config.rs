//! 応募の日付をカレンダーに載せる機能の環境変数 (`docs/env_variables_reference.md`)。
use std::time::Duration;

/// 載せる項目の既定 (HubSpot の内部名:予定の名前の頭の種類)
pub const DEFAULT_PROPS: &str = "mensetsuyoteibi:面接,nyuushayoteibi:入社,naiteinichi:内定";

#[derive(Debug, Clone, PartialEq)]
pub struct CalendarConfig {
    /// `APPLICANT_CALENDAR_ENABLED=1` のときだけ背景の周回が動く (許可の画面は設定がそろえば使える)
    pub enabled: bool,
    /// `APPLICANT_CALENDAR_INTERVAL_SECS` (既定 300)
    pub interval: Duration,
    /// `APPLICANT_CALENDAR_ID` (既定 `primary` = 許可したアカウントの既定のカレンダー)
    pub calendar_id: String,
    /// `APPLICANT_CALENDAR_ACCOUNT`: 許可できるアドレス (小文字)
    pub account: Option<String>,
    /// `APPLICANT_CALENDAR_PROPS`: (内部名, 種類)
    pub props: Vec<(String, String)>,
    /// `APPLICANT_CALENDAR_EVENT_MINUTES` (既定 60): 時刻つきの予定の長さ
    pub event_minutes: i64,
    /// `APPLICANT_CALENDAR_REDIRECT_URL`: 許可の戻り先
    pub redirect_url: Option<String>,
    /// 起動直後に読む範囲 (今日からさかのぼる日数)。管理する予定の範囲もこれ
    pub lookback_days: i64,
}

impl CalendarConfig {
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let text = |k: &str| {
            get(k)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let num = |k: &str, lo: i64, hi: i64, default: i64| {
            text(k)
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| (lo..=hi).contains(n))
                .unwrap_or(default)
        };
        let props = parse_props(text("APPLICANT_CALENDAR_PROPS").as_deref())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| parse_props(Some(DEFAULT_PROPS)).unwrap_or_default());
        Self {
            enabled: text("APPLICANT_CALENDAR_ENABLED").as_deref() == Some("1"),
            interval: Duration::from_secs(
                num("APPLICANT_CALENDAR_INTERVAL_SECS", 60, 3600, 300) as u64
            ),
            calendar_id: text("APPLICANT_CALENDAR_ID")
                .filter(|s| s.len() <= 300 && !s.contains('/') && !s.contains('?'))
                .unwrap_or_else(|| "primary".into()),
            account: text("APPLICANT_CALENDAR_ACCOUNT")
                .map(|s| s.to_ascii_lowercase())
                .filter(|s| crate::crm::email::valid_address(s)),
            props,
            event_minutes: num("APPLICANT_CALENDAR_EVENT_MINUTES", 5, 24 * 60, 60),
            redirect_url: text("APPLICANT_CALENDAR_REDIRECT_URL"),
            lookback_days: 30,
        }
    }

    /// 種類の名前 (予定の名前の頭)
    pub fn label_of(&self, prop: &str) -> Option<&str> {
        self.props
            .iter()
            .find(|(p, _)| p == prop)
            .map(|(_, l)| l.as_str())
    }
}

/// `name:label,name:label`。内部名は英小文字・数字・`_` だけ、種類は 1〜10 文字。
/// 1 つでも形が違えば全体を捨てる (`None`、呼び出し側が既定を使う)
pub fn parse_props(raw: Option<&str>) -> Option<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::new();
    for part in raw?.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (name, label) = part.split_once(':')?;
        let (name, label) = (name.trim(), label.trim());
        let name_ok = !name.is_empty()
            && name.len() <= 100
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        let label_len = label.chars().count();
        if !name_ok || label_len == 0 || label_len > 10 || label.contains('｜') {
            tracing::warn!("APPLICANT_CALENDAR_PROPS の形が正しくないので既定の項目を使う");
            return None;
        }
        if !out.iter().any(|(n, _)| n == name) {
            out.push((name.to_string(), label.to_string()));
        }
    }
    Some(out)
}
