pub mod activity;
pub mod agents;
pub mod audit;
pub mod auth;
pub mod configs;
pub mod events;
pub mod incidents;
pub mod media;
pub mod monitor;
pub mod olap;
pub mod prevention;
pub mod reports;
pub mod rules;
pub mod timeline;
pub mod users;

use std::sync::Arc;

use axum::Router;

use crate::state::AppState;

pub fn api_v1_router() -> Router<Arc<AppState>> {
    Router::new()
        .merge(auth::router())
        .merge(agents::router())
        .merge(incidents::router())
        .merge(events::router())
        .merge(timeline::router())
        .merge(olap::router())
        .merge(activity::router())
        .merge(configs::router())
        .merge(rules::router())
        .merge(media::router())
        .merge(audit::router())
        .merge(reports::router())
        .merge(users::router())
        .merge(prevention::router())
        .merge(monitor::router())
}
