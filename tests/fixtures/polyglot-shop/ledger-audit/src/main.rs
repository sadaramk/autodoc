//! ledger-audit: serves reconciliation reports over the payments table.

mod audit;
mod routes;

use sqlx::postgres::PgPoolOptions;

/// Connects to Postgres and serves the audit API on :9090.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let app = routes::router(pool);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:9090").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
