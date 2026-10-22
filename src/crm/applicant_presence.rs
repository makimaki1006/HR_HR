//! 応募画面で、だれがどの応募を開いているか。同じ応募者に重ねて電話しないための目印。
//! 開いている画面が 15 秒おきに知らせ、届かなくなったら消す。プロセス内のメモリだけ (単一インスタンス前提)。
use crate::crm::{rbac, routes::CrmCtx};
use crate::AppState;
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tower_sessions::Session;
use ts_rs::TS;

/// 隠れたタブはブラウザがタイマーを 1 分おきまで間引くので、2 回分 + 余裕を見る。
const TTL: Duration = Duration::from_secs(150);
const MAX_TABS: usize = 200;

#[derive(Deserialize)]
pub(in crate::crm) struct PresenceBeat {
    /// 画面 (タブ) ごとの乱数。同じ人の別タブを分けて数える。
    tab: String,
    /// 開いている応募。null なら閉じた・何も開いていない。
    applicant: Option<String>,
    #[serde(default)]
    phone: String,
}

#[derive(Serialize, Clone, TS)]
pub struct ApplicantViewer {
    pub applicant: String,
    /// 電話番号の下 9 桁。同じ人の別の応募も見分ける。
    pub phone_key: String,
    /// メールの @ より前
    pub name: String,
}
#[derive(Serialize, TS)]
pub struct ApplicantPresence {
    /// 自分以外の人が開いている応募
    pub viewers: Vec<ApplicantViewer>,
}

struct Entry {
    email: String,
    viewer: ApplicantViewer,
    at: Instant,
}
#[derive(Default)]
pub(in crate::crm) struct PresenceState {
    tabs: Mutex<HashMap<String, Entry>>,
}

pub(in crate::crm) fn phone_key(raw: &str) -> String {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    let n = digits.len();
    if n < 9 {
        return String::new();
    }
    digits[n - 9..].to_string()
}

impl PresenceState {
    fn beat(&self, email: &str, beat: PresenceBeat, now: Instant) -> Vec<ApplicantViewer> {
        let mut tabs = self.tabs.lock().unwrap_or_else(|e| e.into_inner());
        tabs.retain(|_, e| now.duration_since(e.at) < TTL);
        // 別の人のタブ id を上書きしない
        let owned = tabs.get(&beat.tab).is_none_or(|e| e.email == email);
        match beat.applicant.filter(|a| !a.is_empty() && a.len() <= 40) {
            Some(applicant) if owned && beat.tab.len() <= 64 => {
                if tabs.len() < MAX_TABS || tabs.contains_key(&beat.tab) {
                    let name = email.split('@').next().unwrap_or(email).to_string();
                    let viewer = ApplicantViewer {
                        applicant,
                        phone_key: phone_key(&beat.phone),
                        name,
                    };
                    tabs.insert(
                        beat.tab,
                        Entry {
                            email: email.to_string(),
                            viewer,
                            at: now,
                        },
                    );
                }
            }
            _ if owned => {
                tabs.remove(&beat.tab);
            }
            _ => {}
        }
        let mut out: Vec<_> = tabs
            .values()
            .filter(|e| e.email != email)
            .map(|e| e.viewer.clone())
            .collect();
        out.sort_by(|a, b| (&a.applicant, &a.name).cmp(&(&b.applicant, &b.name)));
        out.dedup_by(|a, b| a.applicant == b.applicant && a.name == b.name);
        out
    }
}

/// `POST /api/crm/applicants/presence`: 開いている応募を知らせ、ほかの人が開いている応募を受け取る。
pub(in crate::crm) async fn presence(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    session: Session,
    Json(beat): Json<PresenceBeat>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let email = principal.email.unwrap_or_default().to_lowercase();
    let viewers = ctx.applicants.presence.beat(&email, beat, Instant::now());
    Json(ApplicantPresence { viewers }).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn b(tab: &str, applicant: Option<&str>, phone: &str) -> PresenceBeat {
        PresenceBeat {
            tab: tab.into(),
            applicant: applicant.map(str::to_string),
            phone: phone.into(),
        }
    }
    #[test]
    fn 自分以外の開いている応募を返し_閉じたら_時間が過ぎたら消す() {
        let s = PresenceState::default();
        let t0 = Instant::now();
        assert!(s
            .beat("a@x", b("ta", Some("1"), "090-0000-1234"), t0)
            .is_empty());
        let seen = s.beat("b@x", b("tb", Some("2"), ""), t0);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].applicant, "1");
        assert_eq!(seen[0].name, "a");
        assert_eq!(seen[0].phone_key, "000001234");
        // 同じ人の別タブは自分として扱う
        assert_eq!(s.beat("a@x", b("ta2", Some("2"), ""), t0)[0].name, "b");
        // 閉じると消える
        s.beat("a@x", b("ta", None, ""), t0);
        s.beat("a@x", b("ta2", None, ""), t0);
        assert!(s.beat("b@x", b("tb", Some("2"), ""), t0).is_empty());
        // 知らせが止まった画面は TTL で消える
        s.beat("a@x", b("ta", Some("1"), ""), t0);
        assert_eq!(
            s.beat(
                "b@x",
                b("tb", Some("2"), ""),
                t0 + TTL - Duration::from_secs(1)
            )
            .len(),
            1
        );
        assert!(s.beat("b@x", b("tb", Some("2"), ""), t0 + TTL).is_empty());
    }
    #[test]
    fn 別の人のタブidは上書きも削除もできない() {
        let s = PresenceState::default();
        let t0 = Instant::now();
        s.beat("a@x", b("t", Some("1"), ""), t0);
        s.beat("b@x", b("t", Some("9"), ""), t0);
        s.beat("b@x", b("t", None, ""), t0);
        let seen = s.beat("c@x", b("tc", None, ""), t0);
        assert_eq!(seen.len(), 1);
        assert_eq!(
            (seen[0].applicant.as_str(), seen[0].name.as_str()),
            ("1", "a")
        );
    }
    #[test]
    fn 電話番号は下9桁で比べる() {
        assert_eq!(phone_key("+81 90-1234-5678"), phone_key("090(1234)5678"));
        assert_eq!(phone_key("12345"), "");
    }
}
