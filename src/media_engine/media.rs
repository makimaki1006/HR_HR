//! ドメイン→媒体(名・種別)の対応づけ。
//!
//! Python 版 `media_master` の移植。SERP ドメインを媒体名だけでなく **種別**
//! (service_type: 求人検索エンジン/求人ボード/企業サイト、job_scope: 総合/介護/
//! 医療介護/ドライバー)まで解決する。種別は LLM に「専門媒体か総合か」を判断させる
//! 材料になる(順位だけでは分からない示唆)。

use std::collections::HashMap;

/// 媒体マスタの1行。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaRow {
    pub media_id: String,
    pub media_name: String,
    /// job_search_engine / job_board / corporate。
    pub service_type: String,
    /// general(総合) / care / medical_care / driver 等。general 以外は職種特化。
    pub job_scope: String,
}

impl MediaRow {
    /// 職種特化媒体か(job_scope が general 以外)。
    pub fn is_specialized(&self) -> bool {
        self.job_scope != "general"
    }
}

/// ホストから媒体行を返す。完全一致が無ければ親ドメインでも照合する。
/// `host` は正規化済み(小文字・www 除去)前提。
pub fn resolve_by_host<'a>(
    host: &str,
    index: &'a HashMap<String, MediaRow>,
) -> Option<&'a MediaRow> {
    if host.is_empty() {
        return None;
    }
    if let Some(row) = index.get(host) {
        return Some(row);
    }
    let parts: Vec<&str> = host.split('.').collect();
    for i in 1..parts.len().saturating_sub(1) {
        let parent = parts[i..].join(".");
        if let Some(row) = index.get(&parent) {
            return Some(row);
        }
    }
    None
}

/// (media_id, 表示用媒体名, 登録済みか) を返す。未登録はドメインのまま通す。
pub fn media_key(host: &str, index: &HashMap<String, MediaRow>) -> (String, String, bool) {
    match resolve_by_host(host, index) {
        Some(row) => (row.media_id.clone(), row.media_name.clone(), true),
        None => (host.to_string(), host.to_string(), false),
    }
}

/// 媒体マスタ静的データ(Python `media_master.MEDIA_MASTER` の移植)。
/// 各要素 = (media_id, media_name, [domains...], service_type, job_scope)。
pub const MEDIA_MASTER: &[(&str, &str, &[&str], &str, &str)] = &[
    ("indeed", "Indeed", &["jp.indeed.com", "indeed.com"], "job_search_engine", "general"),
    ("stanby", "スタンバイ", &["jp.stanby.com", "stanby.com"], "job_search_engine", "general"),
    ("doraever", "ドラエバー", &["doraever.jp"], "job_board", "driver"),
    ("kyujinbox", "求人ボックス", &["xn--pckua2a7gp15o89zb.com", "求人ボックス.com", "kyujinbox.com"], "job_search_engine", "general"),
    ("townwork", "タウンワーク", &["townwork.net"], "job_board", "general"),
    ("job_medley", "ジョブメドレー", &["job-medley.com"], "job_board", "medical_care"),
    ("ekaigotenshoku", "e介護転職", &["ekaigotenshoku.com"], "job_board", "care"),
    ("kaigojob", "ウェルミージョブ（旧カイゴジョブ）", &["kaigojob.com"], "job_board", "care"),
    ("kiracare", "レバウェル介護（きらケア）", &["job.kiracare.jp", "kiracare.jp"], "job_board", "care"),
    ("co_medical", "コメディカルドットコム", &["co-medical.com"], "job_board", "medical_care"),
    ("mynavi_kaigo", "マイナビ介護職", &["kaigoshoku.mynavi.jp"], "job_board", "care"),
    ("mynavi_tenshoku", "マイナビ転職", &["tenshoku.mynavi.jp"], "job_board", "general"),
    ("kaigo_worker", "介護ワーカー", &["kaigoworker.jp"], "job_board", "care"),
    ("creatework", "クリエイトワーク", &["creatework.jp"], "job_board", "care"),
    ("kaigo_kyuujin_navi", "介護求人ナビ", &["kaigo-kyuujin.com"], "job_board", "care"),
    ("iryou21", "医療21", &["iryou21.jp"], "job_board", "medical_care"),
    ("doda", "doda", &["doda.jp"], "job_board", "general"),
    ("en_tenshoku", "エン転職", &["employment.en-japan.com", "en-japan.com"], "job_board", "general"),
    ("baitoru", "バイトル", &["baitoru.com"], "job_board", "general"),
    ("hatalike", "はたらいく", &["hatalike.jp"], "job_board", "general"),
    ("toranet", "とらばーゆ", &["toranet.jp"], "job_board", "general"),
    ("froma", "フロム・エー ナビ", &["froma.com"], "job_board", "general"),
    ("mynavi_baito", "マイナビバイト", &["baito.mynavi.jp"], "job_board", "general"),
    ("engage", "engage", &["en-gage.net"], "job_board", "general"),
    ("kaigo_shikaku", "かいごの資格", &["xn--u9jv84l7ea468b.com"], "job_board", "care"),
    ("hellowork_careers", "ハローワークの求人検索（hellowork.careers）", &["hellowork.careers"], "job_search_engine", "general"),
    ("karukeru", "かる・ける", &["karu-keru.com"], "job_board", "medical_care"),
    ("solasto", "ソラスト採用サイト", &["solasto-career.com"], "corporate", "medical_care"),
    ("benesse_saiyo", "ベネッセスタイルケア採用サイト", &["saiyo.benesse-style-care.co.jp"], "corporate", "care"),
];

/// [`MEDIA_MASTER`] から ドメイン→行 の索引を作る(Python `_DOMAIN_INDEX` と一致)。
pub fn default_index() -> HashMap<String, MediaRow> {
    let mut index = HashMap::new();
    for (media_id, media_name, domains, service_type, job_scope) in MEDIA_MASTER {
        for domain in *domains {
            index.insert(
                domain.to_lowercase(),
                MediaRow {
                    media_id: (*media_id).to_string(),
                    media_name: (*media_name).to_string(),
                    service_type: (*service_type).to_string(),
                    job_scope: (*job_scope).to_string(),
                },
            );
        }
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, name: &str, scope: &str) -> MediaRow {
        MediaRow {
            media_id: id.into(),
            media_name: name.into(),
            service_type: "job_board".into(),
            job_scope: scope.into(),
        }
    }

    fn index() -> HashMap<String, MediaRow> {
        HashMap::from([
            ("indeed.com".to_string(), row("indeed", "Indeed", "general")),
            ("job-medley.com".to_string(), row("job_medley", "ジョブメドレー", "medical_care")),
        ])
    }

    #[test]
    fn resolves_subdomain_via_parent() {
        let (id, name, known) = media_key("jp.indeed.com", &index());
        assert_eq!(id, "indeed");
        assert_eq!(name, "Indeed");
        assert!(known);
    }

    #[test]
    fn exact_match() {
        let (_, name, known) = media_key("job-medley.com", &index());
        assert_eq!(name, "ジョブメドレー");
        assert!(known);
    }

    #[test]
    fn unknown_passes_through_domain() {
        let (id, name, known) = media_key("doraever.jp", &index());
        assert_eq!(id, "doraever.jp");
        assert_eq!(name, "doraever.jp");
        assert!(!known);
    }

    #[test]
    fn default_index_resolves_hosts_and_scope() {
        let idx = default_index();
        // ドライバー専門(ドラエバー)は job_scope=driver → specialized。
        let d = resolve_by_host("doraever.jp", &idx).unwrap();
        assert_eq!(d.media_name, "ドラエバー");
        assert_eq!(d.job_scope, "driver");
        assert!(d.is_specialized());
        // Indeed は総合。サブドメイン→親で解決。
        let i = resolve_by_host("jp.indeed.com", &idx).unwrap();
        assert_eq!(i.media_id, "indeed");
        assert!(!i.is_specialized());
    }

    #[test]
    fn dedup_keeps_topmost_per_media_id() {
        let idx = default_index();
        let ordered = ["jp.indeed.com", "townwork.net", "indeed.com", "doraever.jp"];
        let mut seen = std::collections::HashSet::new();
        let mut kept: Vec<String> = Vec::new();
        for host in ordered {
            let (media_id, _, _) = media_key(host, &idx);
            if seen.insert(media_id.clone()) {
                kept.push(media_id);
            }
        }
        assert_eq!(kept, vec!["indeed", "townwork", "doraever"]);
    }
}
