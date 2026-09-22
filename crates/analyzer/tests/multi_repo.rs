//! Integration: a system documented from several repositories at once.
//!
//! One repository calls another. Read alone, the caller can only say the call
//! leaves; read together with the repository that answers it, the call has an
//! operation, a contract and a citation — and that citation has to name the
//! repository it was read from, because a file path is not an address once
//! there is more than one root.

use std::path::{Path, PathBuf};

use nunki_analyzer::api::ApiModel;
use nunki_analyzer::{scan, ScanOptions};

/// A storefront that calls billing over HTTP, and the billing service itself.
fn system(dir: &Path) -> (PathBuf, PathBuf) {
    let storefront = dir.join("storefront");
    let billing = dir.join("billing");
    std::fs::create_dir_all(storefront.join("src")).unwrap();
    std::fs::create_dir_all(billing.join("src")).unwrap();

    std::fs::write(storefront.join("package.json"), r#"{"name":"storefront","version":"1.0.0"}"#).unwrap();
    std::fs::write(
        storefront.join("src/checkout.ts"),
        r#"const BILLING = process.env.BILLING_URL ?? "http://billing:9000";

interface Invoice {
  id: string;
  total: number;
}

export async function invoiceFor(orderId: string): Promise<Invoice> {
  const res = await fetch(`${BILLING}/v1/invoices/${orderId}`);
  return (await res.json()) as Invoice;
}
"#,
    )
    .unwrap();

    std::fs::write(billing.join("package.json"), r#"{"name":"billing","version":"1.0.0"}"#).unwrap();
    std::fs::write(
        billing.join("src/server.ts"),
        r#"import express from "express";

const app = express();

interface Invoice {
  id: string;
  total: number;
  currency: string;
}

app.get("/v1/invoices/:id", (req, res) => {
  const invoice: Invoice = { id: req.params.id, total: 0, currency: "usd" };
  res.json(invoice);
});

app.listen(9000);
"#,
    )
    .unwrap();
    (storefront, billing)
}

fn api_of(root: &Path, members: Vec<PathBuf>) -> ApiModel {
    let report = scan(root, &ScanOptions { behavior: true, members, ..Default::default() }).unwrap();
    report.api.expect("behavior scans carry an API model")
}

#[test]
fn a_call_into_another_repository_is_unresolved_until_that_repository_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let (storefront, billing) = system(dir.path());

    // Alone: the call is extracted and named, and answers nothing. This is the
    // honest reading — and the state the book reports under "calls that leave
    // what is documented".
    let alone = api_of(&storefront, vec![]);
    let call = alone
        .client_calls
        .iter()
        .find(|c| c.path == "/v1/invoices/{orderId}")
        .expect("the outbound call is extracted from the caller alone");
    assert_eq!(call.target_host.as_deref(), Some("billing"), "the host it names is kept");
    assert!(call.operation.is_none(), "nothing in this repository answers it");

    // Together: the same call resolves to billing's operation.
    let together = api_of(&storefront, vec![billing.clone()]);
    let call = together
        .client_calls
        .iter()
        .find(|c| c.path == "/v1/invoices/{orderId}")
        .expect("the call survives multi-repository resolution");
    assert_eq!(
        call.operation.as_deref(),
        Some("billing.billing:GET /v1/invoices/{id}"),
        "resolved to billing's operation, under a name that cannot collide with ours"
    );
    assert_eq!(call.target_unit.as_deref(), Some("billing.billing"));

    // The operation it points at is in the model, so the book can describe the
    // far side of the edge rather than name it and stop.
    let op = together
        .operations
        .iter()
        .find(|o| Some(o.id.as_str()) == call.operation.as_deref())
        .expect("the operation that answers the call is adopted into the model");
    assert_eq!(op.evidence.repo.as_deref(), Some("billing"), "its citation names the repository it was read from");
    assert_eq!(
        op.handler.evidence.repo.as_deref(),
        Some("billing"),
        "every citation, not only the operation's own, names the repository"
    );
    assert_eq!(op.evidence.file_path, "src/server.ts");

    // Nothing of ours was stamped: our own citations stay local.
    assert!(
        together.client_calls.iter().all(|c| c.evidence.repo.is_none()),
        "this repository's own evidence carries no repository name"
    );

    // Only what is called is adopted, and the models it needs come with it.
    for t in [&op.request_body, &op.response].into_iter().flatten() {
        if let Some(m) = &t.model {
            let model = together.models.iter().find(|x| &x.id == m).expect("a referenced model is adopted too");
            assert_eq!(model.evidence.repo.as_deref(), Some("billing"));
        }
    }
}

#[test]
fn a_member_that_is_not_checked_out_is_reported_rather_than_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let (storefront, _) = system(dir.path());
    let missing = dir.path().join("not-cloned");

    let report = scan(&storefront, &ScanOptions { behavior: true, members: vec![missing], ..Default::default() })
        .expect("a missing member does not fail the scan");
    assert!(
        report.notes.iter().any(|n| n.contains("not-cloned") && n.contains("unresolved")),
        "a member that cannot be read is reported: {:?}",
        report.notes
    );
    let api = report.api.unwrap();
    assert!(
        api.client_calls.iter().any(|c| c.operation.is_none()),
        "and the call stays unresolved rather than being attributed to a guess"
    );
}

#[test]
fn a_call_to_a_host_no_member_answers_stays_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    let (storefront, billing) = system(dir.path());
    // A path billing does not serve, on billing's host.
    std::fs::write(
        storefront.join("src/other.ts"),
        r#"export async function refunds(): Promise<unknown> {
  const res = await fetch("http://billing:9000/v1/refunds");
  return res.json();
}
"#,
    )
    .unwrap();

    let api = api_of(&storefront, vec![billing]);
    let refunds = api.client_calls.iter().find(|c| c.path == "/v1/refunds").expect("the call is extracted");
    assert!(
        refunds.operation.is_none(),
        "a host match is not an operation match; billing serves no /v1/refunds and the book must say so"
    );
}
