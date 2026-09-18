//! Account handlers.

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub fn routes() -> Router {
    Router::new()
        .route("/accounts", get(list_accounts).post(open_account))
        .route("/accounts/{id}", get(get_account))
}

/// An account as clients see it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: u64,
    pub owner_email: String,
    /// Balance in minor units.
    pub balance_cents: i64,
    #[serde(rename = "type")]
    pub kind: String,
}

/// Body of POST /api/accounts.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct OpenAccount {
    #[validate(email)]
    pub owner_email: String,
    #[validate(length(min = 3, max = 40))]
    pub nickname: String,
    #[serde(default)]
    #[validate(range(min = 0, max = 1000000))]
    pub opening_deposit_cents: i64,
    pub referral_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Paging {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// Lists accounts page by page.
pub async fn list_accounts(Query(paging): Query<Paging>) -> Json<Vec<Account>> {
    Json(vec![])
}

/// Opens an account after validating the request.
pub async fn open_account(Json(body): Json<OpenAccount>) -> Result<(StatusCode, Json<Account>), StatusCode> {
    body.validate().map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    Ok((StatusCode::CREATED, Json(Account { id: 1, owner_email: body.owner_email, balance_cents: 0, kind: "personal".into() })))
}

pub async fn get_account(Path(id): Path<u64>) -> Result<Json<Account>, StatusCode> {
    Err(StatusCode::NOT_FOUND)
}

pub async fn close_account(Path(account_id): Path<u64>) -> StatusCode {
    StatusCode::NO_CONTENT
}

pub async fn require_admin() {}
