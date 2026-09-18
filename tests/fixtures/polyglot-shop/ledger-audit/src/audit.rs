//! Reconciliation queries over the payments ledger.

use serde::Serialize;
use sqlx::PgPool;

/// Daily totals grouped by payment status.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DailyTotal {
    pub status: String,
    pub count: i64,
    pub amount_cents: i64,
}

/// Summarises today's payments by status. Read-only.
pub async fn daily_totals(pool: &PgPool) -> Result<Vec<DailyTotal>, sqlx::Error> {
    sqlx::query_as::<_, DailyTotal>(
        "SELECT status, COUNT(*) AS count, COALESCE(SUM(amount_cents), 0) AS amount_cents \
         FROM payments WHERE created_at >= CURRENT_DATE GROUP BY status",
    )
    .fetch_all(pool)
    .await
}
