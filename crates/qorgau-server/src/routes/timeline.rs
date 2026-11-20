use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use qorgau_common::dlp::mask_text;
use qorgau_common::Role;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/events/timeline", get(get_timeline))
        .route("/events/agent-stats", get(agent_event_stats))
}

#[derive(Deserialize)]
struct TimelineQuery {
    types: Option<String>,
    agent_id: Option<String>,
    username: Option<String>,
    severity: Option<String>,
    search: Option<String>,
    date_from: Option<DateTime<Utc>>,
    date_to: Option<DateTime<Utc>>,
    #[serde(default = "def_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn def_limit() -> i64 {
    100
}

#[derive(Serialize, sqlx::FromRow)]
struct TimelineItem {
    id: i64,
    #[serde(rename = "type")]
    event_type: String,
    agent_id: String,
    hostname: String,
    username: String,
    ts: DateTime<Utc>,
    summary: String,
    severity: Option<String>,
    subtype: String,
}

#[derive(Serialize)]
struct TimelineResp {
    items: Vec<TimelineItem>,
    total: i64,
}

async fn get_timeline(
    AuthUser(user): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<TimelineQuery>,
) -> AppResult<Json<TimelineResp>> {
    let all_allowed: Vec<&str> = if user.role.is_officer_or_admin() {
        vec![
            "incident",
            "usb",
            "messenger",
            "app",
            "web",
            "screenshot",
            "clipboard",
            "network",
            "print",
            "session",
        ]
    } else {
        // RBAC: viewer cannot query raw messenger/clipboard/screenshot streams
        vec!["incident", "usb", "app", "web", "network", "print", "session"]
    };

    let allowed_set: HashSet<&str> = all_allowed.iter().copied().collect();
    let requested: Vec<String> = match q.types.as_deref() {
        Some(s) if !s.trim().is_empty() => s
            .split(',')
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| allowed_set.contains(t.as_str()))
            .collect(),
        _ => all_allowed.iter().map(|s| s.to_string()).collect(),
    };

    if requested.is_empty() {
        return Ok(Json(TimelineResp {
            items: Vec::new(),
            total: 0,
        }));
    }

    let sev = q.severity.map(|s| s.to_ascii_lowercase());
    let search_pat = q
        .search
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("%{}%", s.trim()));
    let limit = q.limit.clamp(1, 500);
    let offset = q.offset.max(0);

    let base_cte = "WITH unified AS ( \
        SELECT i.id, 'incident'::text AS event_type, i.agent_id, i.hostname, i.username, i.created_at AS ts, \
               LEFT(i.snippet, 200) AS summary, i.severity::text AS severity, i.channel AS subtype \
        FROM incidents i \
        UNION ALL \
        SELECT u.id, 'usb'::text, u.agent_id, COALESCE(a.hostname, ''), u.username, u.created_at, \
               COALESCE(u.file_path, u.device_name, u.action), NULL::text, u.action \
        FROM usb_events u LEFT JOIN agents a ON a.agent_id = u.agent_id \
        UNION ALL \
        SELECT m.id, 'messenger'::text, m.agent_id, COALESCE(a.hostname, ''), m.username, m.created_at, \
               LEFT(COALESCE(m.content, m.contact, m.messenger), 200), CASE WHEN m.flagged THEN 'high' ELSE NULL END, m.messenger \
        FROM messenger_events m LEFT JOIN agents a ON a.agent_id = m.agent_id \
        UNION ALL \
        SELECT ap.id, 'app'::text, ap.agent_id, COALESCE(a.hostname, ''), ap.username, ap.created_at, \
               COALESCE(ap.window_title, ap.app_name), NULL::text, ap.app_name \
        FROM app_activity ap LEFT JOIN agents a ON a.agent_id = ap.agent_id \
        UNION ALL \
        SELECT w.id, 'web'::text, w.agent_id, COALESCE(a.hostname, ''), w.username, w.created_at, \
               LEFT(w.url, 200), CASE WHEN w.blocked THEN 'medium' ELSE NULL END, w.domain \
        FROM web_activity w LEFT JOIN agents a ON a.agent_id = w.agent_id \
        UNION ALL \
        SELECT sc.id, 'screenshot'::text, sc.agent_id, sc.hostname, sc.username, sc.created_at, \
               'Скриншот (' || sc.file_size || ' Б)', NULL::text, 'screenshot' \
        FROM screenshots sc \
        UNION ALL \
        SELECT c.id, 'clipboard'::text, c.agent_id, COALESCE(a.hostname, ''), c.username, c.created_at, \
               LEFT(c.content, 200), CASE WHEN c.flagged THEN 'high' ELSE NULL END, c.source_app \
        FROM clipboard_events c LEFT JOIN agents a ON a.agent_id = c.agent_id \
        UNION ALL \
        SELECT n.id, 'network'::text, n.agent_id, COALESCE(a.hostname, ''), n.username, n.created_at, \
               n.process_name || ' -> ' || n.remote_ip || ':' || n.remote_port, NULL::text, n.protocol \
        FROM network_connections n LEFT JOIN agents a ON a.agent_id = n.agent_id \
        UNION ALL \
        SELECT p.id, 'print'::text, p.agent_id, COALESCE(a.hostname, ''), p.username, p.created_at, \
               p.document_name || ' (' || p.pages || ' стр.)', NULL::text, p.printer_name \
        FROM print_jobs p LEFT JOIN agents a ON a.agent_id = p.agent_id \
        UNION ALL \
        SELECT s.id, 'session'::text, s.agent_id, s.hostname, s.username, s.created_at, \
               s.event_type || ' (' || s.logon_type || ')', NULL::text, s.event_type \
        FROM user_sessions s \
    ) ";

    let where_clause = "WHERE event_type = ANY($1) \
         AND ($2::text IS NULL OR agent_id = $2) \
         AND ($3::text IS NULL OR username = $3) \
         AND ($4::text IS NULL OR severity = $4) \
         AND ($5::text IS NULL OR summary ILIKE $5 OR subtype ILIKE $5 OR username ILIKE $5) \
         AND ($6::timestamptz IS NULL OR ts >= $6) \
         AND ($7::timestamptz IS NULL OR ts <= $7)";

    let count_sql = format!("{base_cte} SELECT COUNT(*) FROM unified {where_clause}");
    let total: (i64,) = sqlx::query_as(&count_sql)
        .bind(&requested)
        .bind(&q.agent_id)
        .bind(&q.username)
        .bind(&sev)
        .bind(&search_pat)
        .bind(q.date_from)
        .bind(q.date_to)
        .fetch_one(&state.db)
        .await?;

    let items_sql = format!(
        "{base_cte} SELECT id, event_type, agent_id, hostname, username, ts, summary, severity, subtype \
         FROM unified {where_clause} ORDER BY ts DESC LIMIT $8 OFFSET $9"
    );
    let mut items: Vec<TimelineItem> = sqlx::query_as(&items_sql)
        .bind(&requested)
        .bind(&q.agent_id)
        .bind(&q.username)
        .bind(&sev)
        .bind(&search_pat)
        .bind(q.date_from)
        .bind(q.date_to)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

    if user.role == Role::Viewer {
        for it in &mut items {
            it.summary = mask_text(&it.summary);
        }
    }

    Ok(Json(TimelineResp {
        items,
        total: total.0,
    }))
}

#[derive(Serialize, sqlx::FromRow)]
struct AgentStatRow {
    agent_id: String,
    incident: i64,
    usb: i64,
    messenger: i64,
    app: i64,
    web: i64,
    screenshot: i64,
    total: i64,
}

async fn agent_event_stats(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<AgentStatRow>>> {
    let rows: Vec<AgentStatRow> = sqlx::query_as(
        "SELECT a.agent_id, \
            COALESCE(i.c, 0) AS incident, \
            COALESCE(u.c, 0) AS usb, \
            COALESCE(m.c, 0) AS messenger, \
            COALESCE(ap.c, 0) AS app, \
            COALESCE(w.c, 0) AS web, \
            COALESCE(sc.c, 0) AS screenshot, \
            (COALESCE(i.c,0)+COALESCE(u.c,0)+COALESCE(m.c,0)+COALESCE(ap.c,0)+COALESCE(w.c,0)+COALESCE(sc.c,0)) AS total \
         FROM agents a \
         LEFT JOIN (SELECT agent_id, COUNT(*) AS c FROM incidents GROUP BY agent_id) i ON i.agent_id = a.agent_id \
         LEFT JOIN (SELECT agent_id, COUNT(*) AS c FROM usb_events GROUP BY agent_id) u ON u.agent_id = a.agent_id \
         LEFT JOIN (SELECT agent_id, COUNT(*) AS c FROM messenger_events GROUP BY agent_id) m ON m.agent_id = a.agent_id \
         LEFT JOIN (SELECT agent_id, COUNT(*) AS c FROM app_activity GROUP BY agent_id) ap ON ap.agent_id = a.agent_id \
         LEFT JOIN (SELECT agent_id, COUNT(*) AS c FROM web_activity GROUP BY agent_id) w ON w.agent_id = a.agent_id \
         LEFT JOIN (SELECT agent_id, COUNT(*) AS c FROM screenshots GROUP BY agent_id) sc ON sc.agent_id = a.agent_id \
         ORDER BY total DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}
