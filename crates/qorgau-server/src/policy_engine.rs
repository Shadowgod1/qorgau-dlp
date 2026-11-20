use std::sync::Arc;

use arc_swap::ArcSwap;
use dashmap::DashMap;
use qorgau_common::dlp::{scan_builtin_dlp, DlpMatch};
use qorgau_common::Severity;
use regex::Regex;
use sqlx::PgPool;

use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    pub name: String,
    pub channel: String,
    pub action: String,
    pub severity: Severity,
    pub regex: Regex,
}

#[derive(Debug, Clone)]
pub struct BlockedDomainEntry {
    pub category: String,
}

#[derive(Debug, Clone)]
pub struct ActivityCat {
    pub pattern: String,
    pub productivity: String,
}

pub struct PolicyEngine {
    policies: ArcSwap<Vec<CompiledPolicy>>,
    blocked_domains: DashMap<String, BlockedDomainEntry>,
    activity_cats: ArcSwap<Vec<ActivityCat>>,
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            policies: ArcSwap::from_pointee(Vec::new()),
            blocked_domains: DashMap::new(),
            activity_cats: ArcSwap::from_pointee(Vec::new()),
        }
    }

    pub async fn reload_all(&self, db: &PgPool) -> AppResult<()> {
        self.reload_policies(db).await?;
        self.reload_blocked_domains(db).await?;
        self.reload_activity_categories(db).await?;
        Ok(())
    }

    pub async fn reload_policies(&self, db: &PgPool) -> AppResult<()> {
        let rows: Vec<(i64, String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, name, channel, action, severity, pattern FROM policies WHERE enabled = TRUE",
        )
        .fetch_all(db)
        .await?;

        let mut compiled = Vec::with_capacity(rows.len());
        for (id, name, channel, action, severity, pattern) in rows {
            match regex::RegexBuilder::new(&pattern)
                .case_insensitive(true)
                .size_limit(1 << 20)
                .build()
            {
                Ok(re) => compiled.push(CompiledPolicy {
                    name,
                    channel: channel.to_ascii_lowercase(),
                    action: action.to_ascii_lowercase(),
                    severity: Severity::from_str_loose(&severity),
                    regex: re,
                }),
                Err(e) => {
                    tracing::warn!(policy_id = id, %name, error = %e, "Skipping invalid policy regex");
                }
            }
        }
        let count = compiled.len();
        self.policies.store(Arc::new(compiled));
        tracing::info!(count, "Reloaded DLP policies into memory");
        Ok(())
    }

    pub async fn reload_blocked_domains(&self, db: &PgPool) -> AppResult<()> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT LOWER(domain), category FROM blocked_domains WHERE enabled = TRUE",
        )
        .fetch_all(db)
        .await?;

        self.blocked_domains.clear();
        for (domain, category) in rows {
            self.blocked_domains
                .insert(domain, BlockedDomainEntry { category });
        }
        tracing::info!(count = self.blocked_domains.len(), "Reloaded blocked domains into memory");
        Ok(())
    }

    pub async fn reload_activity_categories(&self, db: &PgPool) -> AppResult<()> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT LOWER(pattern), productivity FROM activity_categories")
                .fetch_all(db)
                .await?;
        let cats: Vec<ActivityCat> = rows
            .into_iter()
            .map(|(pattern, productivity)| ActivityCat {
                pattern,
                productivity,
            })
            .collect();
        self.activity_cats.store(Arc::new(cats));
        Ok(())
    }

    /// Evaluates both built-in KZ DLP detectors (checksum-validated) and custom DB regex policies.
    pub fn evaluate(&self, channel: &str, text: &str) -> Vec<DlpMatch> {
        if text.is_empty() {
            return Vec::new();
        }
        let ch = channel.to_ascii_lowercase();
        let mut results = scan_builtin_dlp(text);

        let policies = self.policies.load();
        for p in policies.iter() {
            if p.channel != "all" && p.channel != "*" && p.channel != ch {
                continue;
            }
            if let Some(m) = p.regex.find(text) {
                results.push(DlpMatch {
                    rule_name: p.name.clone(),
                    severity: p.severity,
                    action: p.action.clone(),
                    matched_sample: m.as_str().chars().take(48).collect(),
                });
            }
        }
        results
    }

    /// Checks whether a domain (or any parent suffix, e.g., `sub.vk.com` -> `vk.com`) is blocked.
    pub fn check_domain_blocked(&self, domain: &str) -> Option<BlockedDomainEntry> {
        let d = domain.trim().to_ascii_lowercase();
        let d = d.trim_start_matches("www.");
        if d.is_empty() {
            return None;
        }
        if let Some(entry) = self.blocked_domains.get(d) {
            return Some(entry.clone());
        }
        // Check parent domain suffixes
        let mut rest = d;
        while let Some(pos) = rest.find('.') {
            rest = &rest[pos + 1..];
            if let Some(entry) = self.blocked_domains.get(rest) {
                return Some(entry.clone());
            }
        }
        None
    }

    /// Classifies domain into coarse category for web_activity & AgentConfig alert toggles.
    pub fn classify_web_category(&self, domain: &str) -> &'static str {
        let d = domain.to_ascii_lowercase();
        const SOCIAL: &[&str] = &[
            "vk.com", "instagram.com", "facebook.com", "tiktok.com", "twitter.com", "x.com", "ok.ru",
        ];
        const STREAMING: &[&str] = &["youtube.com", "twitch.tv", "netflix.com", "kinopoisk.ru"];
        const GAMBLING: &[&str] = &["1xbet", "olimpbet", "parimatch", "fonbet", "pin-up", "casino"];
        const CRYPTO: &[&str] = &["binance.com", "bybit.com", "okx.com", "coinbase.com", "kraken.com"];
        const ADULT: &[&str] = &["pornhub", "xvideos", "xnxx", "onlyfans"];
        const WORK: &[&str] = &[
            "github.com", "gitlab.com", "jira", "confluence", "docs.google.com", "office.com",
            "egov.kz", "goszakup.gov.kz", "kgd.gov.kz", "1c", "bitrix",
        ];

        if ADULT.iter().any(|k| d.contains(k)) {
            "adult"
        } else if GAMBLING.iter().any(|k| d.contains(k)) {
            "gambling"
        } else if SOCIAL.iter().any(|k| d.contains(k)) {
            "social"
        } else if STREAMING.iter().any(|k| d.contains(k)) {
            "streaming"
        } else if CRYPTO.iter().any(|k| d.contains(k)) {
            "crypto"
        } else if WORK.iter().any(|k| d.contains(k)) {
            "work"
        } else {
            "other"
        }
    }

    pub fn classify_productivity(&self, app_name: &str, window_title: &str) -> String {
        let hay_app = app_name.to_ascii_lowercase();
        let hay_title = window_title.to_ascii_lowercase();
        let cats = self.activity_cats.load();
        for cat in cats.iter() {
            if hay_app.contains(&cat.pattern) || hay_title.contains(&cat.pattern) {
                return cat.productivity.clone();
            }
        }
        "neutral".to_string()
    }
}

pub async fn seed_default_categories(db: &PgPool) -> AppResult<()> {
    let defaults: &[(&str, &str, &str)] = &[
        ("code.exe", "productive", "VS Code"),
        ("idea64.exe", "productive", "IntelliJ IDEA"),
        ("pycharm64.exe", "productive", "PyCharm"),
        ("excel.exe", "productive", "Microsoft Excel"),
        ("winword.exe", "productive", "Microsoft Word"),
        ("powerpnt.exe", "productive", "Microsoft PowerPoint"),
        ("outlook.exe", "productive", "Microsoft Outlook"),
        ("1cv8", "productive", "1C:Предприятие"),
        ("mstsc.exe", "productive", "Remote Desktop"),
        ("pgadmin", "productive", "pgAdmin"),
        ("youtube", "unproductive", "YouTube"),
        ("vk.com", "unproductive", "VK"),
        ("instagram", "unproductive", "Instagram"),
        ("tiktok", "unproductive", "TikTok"),
        ("steam.exe", "unproductive", "Steam"),
        ("dota2", "unproductive", "Dota 2"),
        ("cs2.exe", "unproductive", "Counter-Strike 2"),
        ("telegram.exe", "neutral", "Telegram"),
        ("chrome.exe", "neutral", "Google Chrome"),
        ("msedge.exe", "neutral", "Microsoft Edge"),
    ];

    for (pattern, prod, label) in defaults {
        sqlx::query(
            "INSERT INTO activity_categories (pattern, productivity, label) \
             VALUES ($1, $2, $3) ON CONFLICT (pattern) DO NOTHING",
        )
        .bind(pattern)
        .bind(prod)
        .bind(label)
        .execute(db)
        .await?;
    }
    Ok(())
}
