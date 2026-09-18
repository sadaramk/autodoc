//! HTTP routes for the audit service.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::audit::{daily_totals, DailyTotal};

/// Builds the router with shared database state.
pub fn router(pool: PgPool) -> Router {
    Router::new().nest("/reports", reports()).with_state(pool)
}

/// Reconciliation reports, mounted under `/reports`.
fn reports() -> Router<PgPool> {
    Router::new()
        .route("/daily", get(daily_report))
        .route("/daily/{status}", get(daily_status))
}

/// Filters for the daily report.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyQuery {
    /// Hide statuses with fewer payments than this.
    #[serde(default)]
    pub min_count: Option<i64>,
}

/// GET /reports/daily — today's payment totals by status.
async fn daily_report(State(pool): State<PgPool>, Query(filter): Query<DailyQuery>) -> Json<Vec<DailyTotal>> {
    let totals = daily_totals(&pool).await.unwrap_or_default();
    let min = filter.min_count.unwrap_or(0);
    Json(totals.into_iter().filter(|t| t.count >= min).collect())
}

/// GET /reports/daily/{status} — today's total for one payment status.
async fn daily_status(State(pool): State<PgPool>, Path(status): Path<String>) -> Result<Json<DailyTotal>, StatusCode> {
    let totals = daily_totals(&pool).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    totals.into_iter().find(|t| t.status == status).map(Json).ok_or(StatusCode::NOT_FOUND)
}
