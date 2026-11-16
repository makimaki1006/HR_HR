//! アカウントの無効化 (`accounts.disabled_at`) の照会結果を、プロセス内で 5 分覚える (2026-11-16 ユーザー決定)。
//!
//! それまでは画面の要求ごとに監査 Turso へ SELECT していた (ログイン済みの要求のたび 1 回、CRM・案件管理の API はさらに 1 回)。
//! 無効にした人が最大 5 分使えることはユーザーが了承済み。
//! - 鍵はメールを前後の空白を除いて小文字にしたもの (照会の SQL も大文字小文字を区別しない)。
//! - 照会に失敗した結果は覚えない (止めない方針はそのまま。次の要求でまた照会する)。
//! - ログインの瞬間 (パスワード・Google) と Gmail などの許可の保存は、覚えた値を使わず毎回照会する
//!   (`crate::account_is_disabled_fresh`)。照会できた結果はここに入れ直す。
//! - 同じプロセスで管理者が accounts を書き換えたら (役割の変更)、その人の分を捨てる ([`DisabledCache::forget`])。
//!   無効化・有効化そのものはアプリに操作が無く、監査 DB を直接書き換えて行う。その場合は最大 5 分で反映される。
//! - 件数の上限を超えたら全部捨てる (`crm::rbac` の役割のキャッシュと同じ。ユーザー数より十分大きい)。
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 覚えておく時間
pub const DISABLED_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
/// 覚えておく件数の上限
const DISABLED_CACHE_MAX: usize = 10_000;

#[derive(Default)]
pub struct DisabledCache {
    inner: Mutex<HashMap<String, (Instant, bool)>>,
}

impl DisabledCache {
    fn key(email: &str) -> String {
        email.trim().to_lowercase()
    }

    /// 覚えていて、まだ `DISABLED_CACHE_TTL` 経っていない値
    pub fn get(&self, email: &str, now: Instant) -> Option<bool> {
        let g = self.inner.lock().ok()?;
        let (at, v) = g.get(&Self::key(email))?;
        (now.saturating_duration_since(*at) < DISABLED_CACHE_TTL).then_some(*v)
    }

    /// 照会できた結果だけを入れる (失敗は呼び出し側で入れない)
    pub fn put(&self, email: &str, now: Instant, disabled: bool) {
        if let Ok(mut g) = self.inner.lock() {
            if g.len() >= DISABLED_CACHE_MAX {
                g.clear();
            }
            g.insert(Self::key(email), (now, disabled));
        }
    }

    /// その人の分を捨てる (管理者が accounts を書き換えたとき)
    pub fn forget(&self, email: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.remove(&Self::key(email));
        }
    }

    /// 全部捨てる (テストで「再起動した」状態を作るため)
    #[cfg(test)]
    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.clear();
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 五分のあいだだけ覚え_鍵は大文字小文字と空白を区別しない() {
        let c = DisabledCache::default();
        let t0 = Instant::now();
        c.put(" Taro@F-A-C.co.jp ", t0, true);
        assert_eq!(c.get("taro@f-a-c.co.jp", t0), Some(true));
        assert_eq!(
            c.get("TARO@f-a-c.co.jp", t0 + Duration::from_secs(299)),
            Some(true)
        );
        assert_eq!(
            c.get("taro@f-a-c.co.jp", t0 + Duration::from_secs(300)),
            None
        );
        assert_eq!(c.get("hanako@f-a-c.co.jp", t0), None);
    }

    #[test]
    fn 捨てるとその人の分だけ消える() {
        let c = DisabledCache::default();
        let t0 = Instant::now();
        c.put("a@x.jp", t0, false);
        c.put("b@x.jp", t0, true);
        c.forget("A@X.JP");
        assert_eq!(c.get("a@x.jp", t0), None);
        assert_eq!(c.get("b@x.jp", t0), Some(true));
    }

    #[test]
    fn 上限を超えたら全部捨てて増え続けない() {
        let c = DisabledCache::default();
        let t0 = Instant::now();
        for i in 0..DISABLED_CACHE_MAX {
            c.put(&format!("u{i}@x.jp"), t0, false);
        }
        assert_eq!(c.len(), DISABLED_CACHE_MAX);
        c.put("next@x.jp", t0, true);
        assert_eq!(c.len(), 1);
        assert_eq!(c.get("next@x.jp", t0), Some(true));
    }
}
