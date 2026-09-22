//! Integration: one book for a system spread over several repositories.
//!
//! The `cross-repo` fixture calls a `notifications` service that lives in
//! another repository. Read alone it can only report that the call leaves; read
//! together with `notifications` the call has an operation, a contract and a
//! citation — and the book has to keep saying which repository each cited line
//! came from, because a path is not an address once there is more than one root.

use std::path::{Path, PathBuf};

use nunki_book::{plan, BookOptions};

fn fixtures() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).to_path_buf()
}

fn with_members(members: Vec<PathBuf>) -> BookOptions {
    BookOptions { members, ..Default::default() }
}

#[test]
fn a_call_into_a_member_repository_becomes_an_edge_the_book_describes() {
    let out = tempfile::tempdir().unwrap();
    let members = vec![fixtures().join("notifications")];
    let planned = plan(&fixtures().join("cross-repo"), out.path(), &with_members(members)).unwrap();
    let book = &planned.built.book;

    // The call no longer only "leaves": it resolves to the member's operation.
    let api = planned.built.report.api.as_ref().unwrap();
    let call =
        api.client_calls.iter().find(|c| c.path == "/v1/notifications").expect("the outbound call is still extracted");
    assert_eq!(
        call.operation.as_deref(),
        Some("notifications.notifications:POST /v1/notifications"),
        "resolved against the repository that answers it"
    );

    // And the far side is in the model, so the book can describe the contract
    // rather than name a host and stop.
    let op = api.operations.iter().find(|o| Some(o.id.as_str()) == call.operation.as_deref()).expect("adopted");
    assert_eq!(op.evidence.repo.as_deref(), Some("notifications"));

    // The evidence page no longer reports it as unattributable, and does say
    // which repositories were read.
    let evidence = book.pages.iter().find(|p| p.id == "evidence").expect("evidence page");
    let text = serde_json::to_string(&evidence.blocks).unwrap();
    assert!(text.contains("Repositories this book was read from"), "{text}");
    assert!(text.contains("notifications"), "{text}");
    assert!(
        !text.contains("POST /v1/notifications"),
        "a call that resolved must not still be listed as one that leaves: {text}"
    );

    // The edge itself: a flow that crosses out of this repository and back.
    let crossing = planned
        .built
        .report
        .flows
        .iter()
        .find(|f| f.steps.iter().any(|s| s.to.starts_with("notifications.")))
        .expect("a flow crosses into the member repository");
    assert!(
        crossing.steps.iter().any(|s| s.from.starts_with("notifications.")),
        "and comes back: {:?}",
        crossing.steps
    );

    // The figure drawn from that flow keeps the member's citation. Two things
    // used to take it away: the unit id was joined with a `/`, which is not a
    // legal IR id, and validation checked the member's lines against *this*
    // repository and then healed the "unverifiable" evidence out of the figure.
    assert_eq!(planned.built.warnings, Vec::<String>::new(), "no figure fails validation");
    let figure = planned.built.diagrams.iter().find(|d| d.id.starts_with("flow-")).expect("the crossing flow is drawn");
    assert!(
        figure.ir.edges.iter().any(|e| e.evidence.as_ref().and_then(|x| x.repo.as_deref()) == Some("notifications")),
        "the figure keeps the citation into the member: {:?}",
        figure.notes
    );
    assert!(
        !figure.notes.iter().any(|n| n.contains("unverifiable")),
        "and nothing was healed away: {:?}",
        figure.notes
    );

    // The member's own operation is not documented as one of ours: no flow of
    // its own, and no functional requirement claiming another team's contract.
    assert!(
        !planned.built.report.flows.iter().any(|f| f.entry.starts_with("notifications.")),
        "a member's operation must not get a flow of its own: {:?}",
        planned.built.report.flows.iter().map(|f| &f.entry).collect::<Vec<_>>()
    );
    let requirements = &planned.built.behaviour.requirements;
    assert!(
        requirements.iter().all(|r| !r.operation.starts_with("notifications.")),
        "a member's operation must not become one of our requirements: {:?}",
        requirements.iter().map(|r| &r.operation).collect::<Vec<_>>()
    );
    assert!(!requirements.is_empty(), "our own requirements are still there");

    // Every citation into the member names it, and none of this repository's do.
    let from_member: Vec<&nunki_book::model::Cite> =
        book.cites.values().filter(|c| c.repo.as_deref() == Some("notifications")).collect();
    assert!(!from_member.is_empty(), "the member's lines are cited somewhere in the book");
    for c in &from_member {
        assert!(
            !c.file.starts_with("src/clients"),
            "a member citation must point into the member, not into this repository: {c:?}"
        );
    }
    assert!(book.cites.values().any(|c| c.repo.is_none()), "this repository's own citations stay unqualified");
    assert_eq!(
        book.meta.members.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        vec!["notifications"],
        "the book records which other repositories it was read from"
    );
}

/// The same fixture read alone: the previous behaviour, unchanged.
///
/// Multi-repository support is opt-in. If configuring nothing changed anything,
/// every existing book would silently start describing whatever happened to sit
/// beside it on disk.
#[test]
fn a_single_repository_book_is_unchanged() {
    let alone = tempfile::tempdir().unwrap();
    let a = plan(&fixtures().join("cross-repo"), alone.path(), &BookOptions::default()).unwrap();
    let text = serde_json::to_string(&a.built.book.pages).unwrap();
    assert!(text.contains("Calls that leave what is documented"), "still reported as unresolved");
    assert!(!text.contains("Repositories this book was read from"), "and no multi-repository framing appears");
    assert!(a.built.book.cites.values().all(|c| c.repo.is_none()));
    assert!(a.built.book.meta.members.is_empty());
}

/// A member named in the configuration but absent from disk.
///
/// The scan reports it and the book carries that into "what remains unknown".
/// The alternative — treating an absent repository as "nothing to resolve" —
/// would turn a missing checkout into a book that quietly claims the call
/// answers nothing.
#[test]
fn a_member_that_is_not_checked_out_is_reported_in_the_book() {
    let out = tempfile::tempdir().unwrap();
    let missing = fixtures().join("no-such-service");
    let planned = plan(&fixtures().join("cross-repo"), out.path(), &with_members(vec![missing])).unwrap();
    let evidence = planned.built.book.pages.iter().find(|p| p.id == "evidence").unwrap();
    let text = serde_json::to_string(&evidence.blocks).unwrap();
    assert!(text.contains("no-such-service"), "the missing member is named: {text}");
    assert!(text.contains("Calls that leave what is documented"), "and the call is still reported as unresolved");
}
