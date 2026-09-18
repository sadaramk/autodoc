//! Accounts service.

mod accounts;

use axum::{middleware, routing::get, Router};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api", api_routes());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn api_routes() -> Router {
    let admin = Router::new()
        .route("/accounts/{id}", axum::routing::delete(accounts::close_account))
        .route_layer(middleware::from_fn(accounts::require_admin));
    Router::new().merge(accounts::routes()).nest("/admin", admin)
}
