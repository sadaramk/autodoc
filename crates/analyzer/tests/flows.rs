//! Flows that don't start with an HTTP request (schedules, message consumers,
//! in-process events) and events followed from the publisher into the
//! handlers that consume them, across languages.

use std::path::{Path, PathBuf};

use autodoc_analyzer::scan::EvidenceRef;
use autodoc_analyzer::trace::{worth_drawing, Flow, Step, StepKind, TriggerKind};
use autodoc_analyzer::{scan, ScanOptions, ScanReport};

fn fixture(name: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(name)
}

fn behaviour(name: &str) -> (PathBuf, ScanReport) {
    let root = fixture(name);
    let r = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
    (root, r)
}

fn flow<'a>(r: &'a ScanReport, id: &str) -> &'a Flow {
    r.flows
        .iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("flow {id} missing; have {:?}", r.flows.iter().map(|f| &f.id).collect::<Vec<_>>()))
}

fn shape(f: &Flow) -> Vec<(String, String, StepKind, String, bool)> {
    f.steps.iter().map(|s| (s.from.clone(), s.to.clone(), s.kind, s.label.clone(), s.asynchronous)).collect()
}

fn step(
    from: &str,
    to: &str,
    kind: StepKind,
    label: &str,
    asynchronous: bool,
) -> (String, String, StepKind, String, bool) {
    (from.into(), to.into(), kind, label.into(), asynchronous)
}

/// The cited line exists and mentions what the step claims.
fn assert_cites(root: &Path, s: &Step, needle: &str) {
    let e: &EvidenceRef = &s.evidence;
    let text = std::fs::read_to_string(root.join(&e.file_path)).unwrap_or_else(|_| panic!("{} missing", e.file_path));
    let lines: Vec<&str> = text.lines().collect();
    assert!(e.start_line >= 1 && e.end_line as usize <= lines.len(), "{e:?} out of range");
    let window = lines[e.start_line as usize - 1..e.end_line as usize].join("\n");
    assert!(window.contains(needle), "`{needle}` not within {e:?}:\n{window}");
}

#[test]
fn request_flow_continues_into_consumers_in_other_languages() {
    let (root, r) = behaviour("flows");
    let f = flow(&r, "flow:orders-api:POST /orders");
    assert_eq!(f.trigger.kind, TriggerKind::Http);
    assert_eq!(
        shape(f),
        vec![
            step("client", "orders-api", StepKind::Call, "POST /orders", false),
            step("orders-api", "postgres", StepKind::Write, "write orders", false),
            step("orders-api", "kafka", StepKind::Publish, "publishes order.created", false),
            step("orders-api", "client", StepKind::Reply, "201 OrdersPostResponse", false),
            // Java consumer, then the in-process event it publishes.
            step("kafka", "inventory", StepKind::Deliver, "delivers order.created", true),
            step("inventory", "postgres", StepKind::Write, "write stock", true),
            step("inventory", "inventory", StepKind::Publish, "publishes StockReserved", true),
            step("inventory", "inventory", StepKind::Deliver, "StockReserved → on", true),
            step("inventory", "postgres", StepKind::Write, "write stock_audit", true),
            // Python consumer.
            step("kafka", "billing-worker", StepKind::Deliver, "delivers order.created", true),
            step("billing-worker", "postgres", StepKind::Write, "write invoices", true),
        ]
    );
    let cites = [
        "INSERT INTO orders",
        "order.created",
        "\"order.created\"",
        "UPDATE stock",
        "publishEvent(new StockReserved",
        "@TransactionalEventListener",
        "INSERT INTO stock_audit",
        "subscribe([\"order.created\"])",
        "INSERT INTO invoices",
    ];
    for (s, needle) in f.steps.iter().skip(1).filter(|s| s.kind != StepKind::Reply).zip(cites) {
        assert_cites(&root, s, needle);
    }
    assert!(worth_drawing(f));
}

#[test]
fn schedules_listeners_and_events_start_their_own_flows() {
    let (root, r) = behaviour("flows");

    let cron = flow(&r, "flow:orders-api:schedule:expireCarts");
    assert_eq!((cron.trigger.kind, cron.trigger.label.as_str()), (TriggerKind::Schedule, "cron 0 * * * *"));
    assert_eq!(
        shape(cron),
        vec![
            step("trigger:schedule", "orders-api", StepKind::Trigger, "cron 0 * * * *", false),
            step("orders-api", "postgres", StepKind::Write, "write carts", false),
        ]
    );
    assert_cites(&root, &cron.steps[0], "cron.schedule(\"0 * * * *\", expireCarts)");
    assert_cites(&root, &cron.steps[1], "UPDATE carts");
    assert_eq!(cron.participants[0].label, "Scheduler");

    let job = flow(&r, "flow:inventory:schedule:reconcile");
    assert_eq!(job.trigger.label, "cron 0 0 * * * *");
    assert_cites(&root, &job.steps[0], "@Scheduled(cron = \"0 0 * * * *\")");
    assert_eq!(job.steps[1].label, "write stock");

    let ticker = flow(&r, "flow:metrics:schedule:collect");
    assert_eq!(ticker.trigger.label, "ticker every time.Minute");
    assert_cites(&root, &ticker.steps[0], "time.NewTicker");
    assert_eq!(shape(ticker)[1], step("metrics", "postgres", StepKind::Write, "write samples", false));

    let listener = flow(&r, "flow:inventory:message:onOrder");
    assert_eq!(
        (listener.trigger.kind, listener.trigger.label.as_str()),
        (TriggerKind::Message, "Kafka topic order.created")
    );
    assert_cites(&root, &listener.steps[0], "@KafkaListener(topics = \"order.created\"");
    // The event it publishes continues asynchronously; the listener's own steps don't.
    assert!(!listener.steps[1].asynchronous && listener.steps.last().unwrap().asynchronous);

    let python = flow(&r, "flow:billing-worker:message:main");
    assert_eq!(python.trigger.label, "Kafka topic order.created");
    assert_eq!(shape(python)[1], step("billing-worker", "postgres", StepKind::Write, "write invoices", false));

    let event = flow(&r, "flow:inventory:event:on");
    assert_eq!((event.trigger.kind, event.trigger.label.as_str()), (TriggerKind::Event, "StockReserved"));
    assert_eq!(event.participants[0].label, "Application events");
    assert_eq!(event.steps[1].label, "write stock_audit");

    for f in r.flows.iter().filter(|f| f.trigger.kind != TriggerKind::Http) {
        assert!(worth_drawing(f), "{} has effects", f.id);
    }
}

#[test]
fn consumer_loops_follow_the_callback_they_dispatch_to() {
    // polyglot-shop's fulfillment polls in `OrderConsumer.run(handler)` and `main` passes `handle_order`.
    let (root, r) = behaviour("polyglot-shop");
    let checkout = flow(&r, "flow:api-gateway:POST /checkout");
    let deliver = checkout.steps.iter().position(|s| s.kind == StepKind::Deliver).expect("continues into fulfillment");
    assert_eq!(
        shape(checkout)[deliver..],
        [
            step("kafka", "fulfillment", StepKind::Deliver, "delivers order.placed", true),
            step("fulfillment", "postgres", StepKind::Write, "write orders", true),
        ]
    );
    assert_cites(&root, &checkout.steps[deliver + 1], "UPDATE orders SET status = 'shipped'");
    let consumer = flow(&r, "flow:fulfillment:message:handle_order");
    assert_eq!(consumer.title, "handle_order · Kafka topic order.placed");
    // The synchronous reply is still the reply to the web client.
    assert!(checkout.steps.iter().any(|s| s.kind == StepKind::Reply && s.to == "web" && !s.asynchronous));
}

#[test]
fn flows_are_deterministic() {
    let (_, a) = behaviour("flows");
    let (_, b) = behaviour("flows");
    assert_eq!(serde_json::to_string(&a.flows).unwrap(), serde_json::to_string(&b.flows).unwrap());
}

/// A job whose schedule, event and side effects are all stated indirectly:
/// the cron lives in configuration, the event is built rather than
/// constructed, the handler names its type in the annotation, and the mail
/// sender is infrastructure of its own.
#[test]
fn configured_schedules_built_events_and_email_are_followed() {
    let (root, r) = behaviour("flows");

    let job = flow(&r, "flow:inventory:schedule:expire");
    assert_eq!(job.trigger.label, "cron 0 0 3 * * * (stock.expiry.cron)", "the value the configuration gives it");
    assert_cites(&root, &job.steps[0], "@Scheduled(cron = \"${stock.expiry.cron}\")");
    assert_eq!(
        shape(job),
        vec![
            step("trigger:schedule", "inventory", StepKind::Trigger, "cron 0 0 3 * * * (stock.expiry.cron)", false),
            step("inventory", "postgres", StepKind::Write, "write stock", false),
            step("inventory", "inventory", StepKind::Publish, "publishes StockExpired", false),
            step("inventory", "smtp", StepKind::Call, "sends email", false),
            step("inventory", "inventory", StepKind::Deliver, "StockExpired → onExpiry", true),
            step("inventory", "postgres", StepKind::Write, "write stock_audit", true),
        ]
    );
    // `StockExpired.builder()…build()` names the event, and the send cites its own line.
    assert_cites(&root, &job.steps[2], "publishEvent(StockExpired.builder()");
    assert_cites(&root, &job.steps[3], "mailSender.send(warning)");

    // `@TransactionalEventListener(classes = StockExpired.class)` on a method taking `Object`.
    let handler = flow(&r, "flow:inventory:event:onExpiry");
    assert_eq!((handler.trigger.kind, handler.trigger.label.as_str()), (TriggerKind::Event, "StockExpired"));
    assert_cites(&root, &handler.steps[0], "classes = StockExpired.class");

    let smtp = r.infrastructure.iter().find(|i| i.id == "smtp").expect("smtp");
    assert_eq!((smtp.label.as_str(), smtp.role.as_str()), ("Email (SMTP)", "Email delivery"));
    assert_eq!(smtp.used_by, vec!["inventory".to_string()]);
    let rel = r.relationships.iter().find(|x| x.source == "inventory" && x.target == "smtp").expect("inventory → smtp");
    assert_eq!(rel.label, "sends email");
    assert_eq!(rel.evidence[0].file_path, "inventory/src/main/java/com/acme/inventory/StockJobs.java");

    // An unresolvable placeholder stays as written rather than reading as a value.
    let kept = flow(&r, "flow:inventory:schedule:reconcile");
    assert_eq!(kept.trigger.label, "cron 0 0 * * * *");
}

/// Go's CQRS and hexagonal layouts dispatch through struct fields and ports.
/// `h.app.Queries.AllTrainings.Handle` names no function: `Handle` is declared on
/// every handler in the service, so a trace by name stops at the HTTP handler and
/// the request flow is lost. Walking the field chain to a type, and a port to its
/// one adapter, is what carries the flow through to the store.
#[test]
fn go_traces_a_request_through_field_chains_and_ports() {
    let (root, r) = behaviour("real-world/go-hexagonal");

    let get = flow(&r, "flow:go-hexagonal:GET /trainings");
    let names: Vec<&str> = get.functions.iter().map(|f| f.as_str()).collect();
    assert!(
        names.iter().any(|f| f.ends_with(":Handle")),
        "the query handler should be reached through h.app.Queries.AllTrainings: {names:?}"
    );

    // The port resolves to its single adapter, which is what touches Firestore.
    let post = flow(&r, "flow:go-hexagonal:POST /trainings");
    let reached: Vec<&str> = post.functions.iter().map(|f| f.as_str()).collect();
    assert!(
        reached.iter().any(|f| f.ends_with(":AddTraining")),
        "the Repository port should resolve to TrainingsFirestoreRepository: {reached:?}"
    );

    let write = post
        .steps
        .iter()
        .find(|s| s.kind == StepKind::Write || s.to.to_lowercase().contains("firestore"))
        .unwrap_or_else(|| panic!("no store step in {:?}", shape(post)));
    assert_cites(&root, write, "trainings");
}

/// A command bus dispatches by message type: `mediatr.Send[*CreateOrder](…)`
/// names the command, not the handler, so a trace by call name stops at the
/// endpoint. The handler is the one method that takes that message.
#[test]
fn go_traces_a_request_through_a_command_bus() {
    let (root, r) = behaviour("real-world/go-hexagonal");

    let flow = flow(&r, "flow:go-hexagonal:POST /orders");
    let reached: Vec<&str> = flow.functions.iter().map(|f| f.as_str()).collect();
    assert!(
        reached.iter().any(|f| f.ends_with(":Handle")),
        "the bus should reach CreateOrderHandler.Handle: {reached:?}"
    );

    let write = flow
        .steps
        .iter()
        .find(|s| s.kind == StepKind::Write)
        .unwrap_or_else(|| panic!("no write in {:?}", shape(flow)));
    assert_cites(&root, write, "orders");
}
