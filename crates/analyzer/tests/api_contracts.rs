//! Integration: API-contract extraction against fixture apps. Operations must carry their full
//! path (every router / mount / scope prefix resolved), declared request and response models with
//! validation rules, errors and auth — and every citation must point at lines that exist.

use std::path::{Path, PathBuf};

use nunki_analyzer::api::{ApiModel, Confidence, Model, Operation};
use nunki_analyzer::scan::EvidenceRef;
use nunki_analyzer::{scan, ScanOptions};

fn fixture(name: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(name)
}

fn api_of(root: &Path) -> ApiModel {
    let report = scan(root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
    let api = report.api.expect("behavior scans carry an API model");
    assert_evidence_real(root, &api);
    api
}

fn assert_ev(root: &Path, e: &EvidenceRef) {
    let text = std::fs::read_to_string(root.join(&e.file_path)).unwrap_or_else(|_| panic!("{} missing", e.file_path));
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        e.start_line >= 1 && e.start_line <= e.end_line && e.end_line as usize <= lines.len(),
        "{e:?} out of range"
    );
    if let Some(sym) = &e.symbol_name {
        let window = lines[e.start_line as usize - 1..e.end_line as usize].join("\n");
        assert!(window.contains(sym.as_str()), "{sym} not within {e:?}");
    }
}

fn assert_evidence_real(root: &Path, api: &ApiModel) {
    for op in &api.operations {
        assert_ev(root, &op.evidence);
        assert_ev(root, &op.handler.evidence);
        op.params.iter().for_each(|p| assert_ev(root, &p.evidence));
        op.errors.iter().for_each(|e| assert_ev(root, &e.evidence));
        op.auth.iter().for_each(|a| assert_ev(root, &a.evidence));
        for t in [&op.request_body, &op.response].into_iter().flatten() {
            if let Some(m) = &t.model {
                assert!(api.models.iter().any(|x| x.id == *m), "{} references missing model {m}", op.id);
            }
        }
    }
    for m in &api.models {
        assert_ev(root, &m.evidence);
        for f in &m.fields {
            assert_ev(root, &f.evidence);
            f.rules.iter().for_each(|r| assert_ev(root, &r.evidence));
            if let Some(nested) = &f.model {
                assert!(api.models.iter().any(|x| x.id == *nested), "{}.{} references missing {nested}", m.id, f.name);
            }
        }
    }
    api.client_calls.iter().for_each(|c| assert_ev(root, &c.evidence));
    api.excluded.iter().for_each(|x| assert_ev(root, &x.evidence));
}

fn ids(api: &ApiModel) -> Vec<&str> {
    api.operations.iter().map(|o| o.id.as_str()).collect()
}

fn op<'a>(api: &'a ApiModel, id: &str) -> &'a Operation {
    api.operations.iter().find(|o| o.id == id).unwrap_or_else(|| panic!("no {id} in {:?}", ids(api)))
}

fn model<'a>(api: &'a ApiModel, id: &str) -> &'a Model {
    api.models.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("no model {id}"))
}

/// `name: type [required] {rules}` for compact field assertions.
fn fields(m: &Model) -> Vec<String> {
    m.fields
        .iter()
        .map(|f| {
            let rules: Vec<&str> = f.rules.iter().map(|r| r.statement.as_str()).collect();
            format!(
                "{}: {}{}{}",
                f.name,
                f.type_name,
                if f.required { "" } else { "?" },
                if rules.is_empty() { String::new() } else { format!(" {{{}}}", rules.join("; ")) }
            )
        })
        .collect()
}

fn params(o: &Operation) -> Vec<String> {
    o.params
        .iter()
        .map(|p| format!("{} {}: {}{}", p.location, p.name, p.type_name, if p.required { "" } else { "?" }))
        .collect()
}

fn errors(o: &Operation) -> Vec<(Option<u16>, Option<&str>)> {
    o.errors.iter().map(|e| (e.status, e.message.as_deref())).collect()
}

fn auth(o: &Operation) -> Vec<String> {
    o.auth.iter().map(|a| format!("{}: {}", a.kind, a.detail)).collect()
}

fn line_of(root: &Path, e: &EvidenceRef) -> String {
    std::fs::read_to_string(root.join(&e.file_path))
        .unwrap()
        .lines()
        .nth(e.start_line as usize - 1)
        .unwrap()
        .to_string()
}

#[test]
fn polyglot_shop_contracts_link_services_and_expose_drift() {
    let root = fixture("polyglot-shop");
    let api = api_of(&root);
    assert_eq!(
        ids(&api),
        [
            "api-gateway:GET /catalog",
            "api-gateway:POST /checkout",
            "ledger-audit:GET /reports/daily",
            "ledger-audit:GET /reports/daily/{status}",
            "payments:POST /charges",
        ]
    );

    // Express: router mounted at /checkout, zod-validated body, inline 201 response.
    let checkout = op(&api, "api-gateway:POST /checkout");
    assert_eq!(
        (checkout.framework.as_str(), checkout.path_partial, checkout.success_status),
        ("express", false, Some(201))
    );
    assert!(line_of(&root, &checkout.evidence).contains("checkoutRouter.post(\"/\""));
    assert_eq!(checkout.request_body.as_ref().unwrap().model.as_deref(), Some("api-gateway:CheckoutRequest"));
    assert_eq!(checkout.confidence, Confidence::Partial, "response is inferred from the literal, not declared");
    assert_eq!(
        errors(checkout),
        [(Some(400), Some("invalid checkout request")), (Some(402), Some("payment declined"))]
    );
    assert_eq!(auth(checkout), ["authenticated: requireCustomer"]);
    assert_eq!(checkout.summary.as_deref(), Some("POST /checkout — the critical transaction path."));
    assert_eq!(
        fields(model(&api, "api-gateway:CheckoutRequest")),
        [
            "items: CheckoutRequestItem[] {at least 1 item}",
            "paymentToken: string {at least 1 character}",
            "couponCode: string? {at most 32 characters}"
        ]
    );
    assert_eq!(model(&api, "api-gateway:CheckoutRequest").fields[0].doc.as_deref(), Some("Cart lines to buy."));
    assert_eq!(
        fields(model(&api, "api-gateway:CheckoutRequestItem")),
        [
            "sku: string {at least 1 character}",
            "quantity: number {must be an integer; must be greater than 0; must be ≤ 99}"
        ]
    );
    let response = model(&api, checkout.response.as_ref().unwrap().model.as_deref().unwrap());
    assert_eq!(fields(response), ["orderId: unknown", "status: \"placed\""]);

    // Go net/http: method pattern, struct tags, statuses.
    let charges = op(&api, "payments:POST /charges");
    assert_eq!(
        (charges.framework.as_str(), charges.handler.name.as_str(), charges.success_status),
        ("net/http", "createCharge", Some(201))
    );
    assert_eq!(charges.confidence, Confidence::Typed);
    assert_eq!(
        fields(model(&api, "payments:chargeRequest")),
        [
            "orderId: string {must be a UUID}",
            "amountCents: int64 {must be > 0}",
            "currency: string? {one of: usd, eur, gbp}",
            "token: string"
        ]
    );
    assert_eq!(model(&api, "payments:chargeRequest").fields[0].code_name.as_deref(), Some("OrderID"));
    assert_eq!(
        errors(charges),
        [
            (Some(400), Some("bad request")),
            (Some(402), None),
            (Some(422), Some("orderId and a positive amountCents are required"))
        ]
    );

    // axum: nest("/reports", reports()) resolves; Query<T> fields become params; Path binds {status}.
    let daily = op(&api, "ledger-audit:GET /reports/daily");
    assert_eq!(params(daily), ["query minCount: Option<i64>?"]);
    assert_eq!(
        daily.response.as_ref().map(|t| (t.model.as_deref(), t.collection)),
        Some((Some("ledger-audit:DailyTotal"), true))
    );
    let status = op(&api, "ledger-audit:GET /reports/daily/{status}");
    assert_eq!(params(status), ["path status: String"]);
    assert_eq!(errors(status), [(Some(404), None), (Some(503), None)]);

    // Health probes are listed, not documented as features.
    assert_eq!(
        api.excluded.iter().map(|e| (e.operation.as_str(), e.reason.as_str())).collect::<Vec<_>>(),
        [("api-gateway:GET /healthz", "health / liveness probe")]
    );

    // Clients: every cross-service call resolves to the operation it hits; drift is named.
    type Call<'a> = (&'a str, &'a str, Option<&'a str>, Option<&'a str>, &'a [String]);
    let calls: Vec<Call> = api
        .client_calls
        .iter()
        .map(|c| (c.unit.as_str(), c.path.as_str(), c.operation.as_deref(), c.caller.as_deref(), c.drift.as_slice()))
        .collect();
    assert_eq!(
        calls,
        [
            ("api-gateway", "/charges", Some("payments:POST /charges"), Some("chargeOrder"), &[][..]),
            (
                "web",
                "/checkout",
                Some("api-gateway:POST /checkout"),
                Some("submitCheckout"),
                &["estimatedDelivery".to_string()][..]
            ),
            ("web", "/catalog", Some("api-gateway:GET /catalog"), Some("fetchCatalog"), &[][..]),
        ]
    );
}

#[test]
fn fastapi_prefixes_from_constants_and_nested_routers_are_resolved() {
    let root = fixture("real-world/uv-workspace");
    let api = api_of(&root);
    assert_eq!(
        ids(&api),
        [
            "backend:GET /api/v1/items",
            "backend:POST /api/v1/items",
            "backend:GET /api/v1/items/{item_id}",
            "backend:GET /items"
        ]
    );
    let search = op(&api, "backend:GET /api/v1/items");
    assert_eq!(params(search), ["query q: str | None?", "query limit: int?", "header x-request-id: str | None?"]);
    assert_eq!(
        search.params[1].rules.iter().map(|r| r.statement.as_str()).collect::<Vec<_>>(),
        ["must be ≥ 1", "must be ≤ 100"]
    );
    assert_eq!(
        search.response.as_ref().map(|t| (t.model.as_deref(), t.collection)),
        Some((Some("backend:ItemPublic"), true))
    );

    let create = op(&api, "backend:POST /api/v1/items");
    assert_eq!((create.success_status, create.confidence), (Some(201), Confidence::Typed));
    assert_eq!(auth(create), ["authenticated: Depends(get_current_user)"]);
    assert_eq!(errors(create), [(Some(422), Some("Tags must not be blank"))]);
    assert_eq!(
        fields(model(&api, "backend:ItemCreate")),
        [
            "title: str {at least 1 character; at most 255 characters}",
            "description: str | None? {at most 255 characters}",
            "tags: list[str]? {at most 5 items}"
        ]
    );
    // alias_generator=to_camel renames inherited and own fields on the wire.
    assert_eq!(
        fields(model(&api, "backend:ItemPublic"))[2..],
        [
            "id: int",
            "ownerEmail: EmailStr {must be a valid email}",
            "status: Literal[\"draft\", \"published\"]? {one of: draft, published}"
        ]
    );

    let read = op(&api, "backend:GET /api/v1/items/{item_id}");
    assert_eq!(read.summary.as_deref(), Some("Fetch one item by id."));
    assert_eq!(errors(read), [(Some(404), Some("Item not found"))]);
    assert_eq!(op(&api, "backend:GET /items").confidence, Confidence::Opaque);
    assert_eq!(api.excluded.len(), 1);

    let call = &api.client_calls[0];
    assert_eq!(
        (call.unit.as_str(), call.path.as_str(), call.operation.as_deref()),
        ("frontend", "/api/v1/items/{id}", Some("backend:GET /api/v1/items/{item_id}"))
    );
    assert!(call.drift.is_empty(), "{:?}", call.drift);
}

#[test]
fn typescript_frameworks() {
    let root = fixture("api-frameworks/express");
    let api = api_of(&root);
    assert_eq!(
        ids(&api),
        [
            "express:GET /api/users",
            "express:POST /api/users",
            "express:GET /api/users/{id}",
            "express:DELETE /api/users/{id}"
        ]
    );
    let list = op(&api, "express:GET /api/users");
    assert_eq!(params(list), ["query page: number?", "query limit: string?"]);
    assert_eq!(list.response.as_ref().unwrap().type_name, "User[]");
    let create = op(&api, "express:POST /api/users");
    assert_eq!(auth(create), ["role: requireRole(\"admin\")"]);
    assert_eq!((create.confidence, create.success_status), (Confidence::Typed, Some(201)));
    assert_eq!(
        fields(model(&api, "express:CreateUser")),
        [
            "email: string {must be a valid email}",
            "role: \"admin\" | \"member\" {one of: admin, member}",
            "name: string? {at least 2 characters; at most 80 characters}"
        ]
    );
    assert_eq!(errors(op(&api, "express:GET /api/users/{id}")), [(Some(404), Some("user not found"))]);
    assert_eq!(op(&api, "express:DELETE /api/users/{id}").success_status, Some(204));
    assert_eq!(api.excluded[0].operation, "express:GET /healthz");

    let api = api_of(&fixture("api-frameworks/fastify"));
    assert_eq!(ids(&api), ["fastify:GET /v1/notes", "fastify:POST /v1/notes"]);
    assert_eq!(params(op(&api, "fastify:GET /v1/notes")), ["query q: string?", "query limit: number?"]);
    let post = op(&api, "fastify:POST /v1/notes");
    assert_eq!((post.confidence, post.success_status), (Confidence::Typed, Some(201)));
    assert_eq!(auth(post), ["authenticated: app.authenticate"]);
    assert_eq!(fields(model(&api, "fastify:NewNote")), ["title: string", "body: string"], "Omit<Note, \"id\">");

    let api = api_of(&fixture("api-frameworks/nestjs"));
    assert_eq!(ids(&api), ["nestjs:POST /api/orders", "nestjs:GET /api/orders/{id}", "nestjs:DELETE /api/orders/{id}"]);
    assert_eq!(
        fields(model(&api, "nestjs:CreateOrderDto")),
        [
            "sku: string {between 3 and 12 characters}",
            "quantity: number {must be an integer; must be ≥ 1}",
            "shipping: string? {one of: standard, express}"
        ]
    );
    let find = op(&api, "nestjs:GET /api/orders/{id}");
    assert_eq!(params(find), ["path id: string", "query expand: string?"]);
    assert_eq!(errors(find), [(Some(404), Some("order not found"))]);
    assert_eq!(
        auth(op(&api, "nestjs:DELETE /api/orders/{id}")),
        ["authenticated: @UseGuards(AuthGuard(\"jwt\"))", "role: @Roles(\"admin\")"]
    );
    assert_eq!(op(&api, "nestjs:DELETE /api/orders/{id}").success_status, Some(204));
    assert_eq!(op(&api, "nestjs:POST /api/orders").success_status, Some(201));

    let api = api_of(&fixture("api-frameworks/hono"));
    assert_eq!(ids(&api), ["hono:POST /books", "hono:GET /books/{isbn}"]);
    let post = op(&api, "hono:POST /books");
    assert_eq!(
        (post.request_body.as_ref().unwrap().model.as_deref(), post.success_status),
        (Some("hono:Book"), Some(201))
    );
    assert_eq!(
        fields(model(&api, "hono:Book")),
        ["title: string {at least 1 character}", "year: number {must be an integer; must be ≥ 1450}"]
    );
    let get = op(&api, "hono:GET /books/{isbn}");
    assert_eq!(params(get), ["path isbn: string", "query fields: string?"]);
    assert_eq!(errors(get), [(Some(404), Some("book not found"))]);
}

#[test]
fn python_frameworks() {
    let api = api_of(&fixture("api-frameworks/fastapi"));
    assert_eq!(
        ids(&api),
        [
            "fastapi:DELETE /api/v2/admin/products/{product_id}",
            "fastapi:GET /api/v2/products",
            "fastapi:POST /api/v2/products",
            "fastapi:GET /api/v2/products/{product_id}"
        ]
    );
    assert_eq!(
        auth(op(&api, "fastapi:DELETE /api/v2/admin/products/{product_id}")),
        ["role: Depends(get_current_admin)"]
    );
    let get = op(&api, "fastapi:GET /api/v2/products/{product_id}");
    assert_eq!(get.params[0].rules[0].statement, "must be > 0");
    assert_eq!(errors(get), [(Some(404), Some("Product not found"))]);
    assert_eq!(get.response.as_ref().unwrap().model.as_deref(), Some("fastapi:ProductOut"), "return annotation");
    assert_eq!(
        fields(model(&api, "fastapi:ProductCreate")),
        [
            "name: str {at least 2 characters; at most 120 characters}",
            "priceCents: int {must be ≥ 0}",
            "category: Category? {one of: books, games}",
            "supplier_email: EmailStr {must be a valid email}",
            "sku: str {must match ^[A-Z]{3}-\\d{4}$}",
        ]
    );
    let price = &model(&api, "fastapi:ProductCreate").fields[1];
    assert_eq!(price.code_name.as_deref(), Some("price_cents"));
    assert_eq!(model(&api, "fastapi:ProductCreate").fields[0].doc.as_deref(), Some("Shown in listings."));
    assert_eq!(model(&api, "fastapi:ProductPage").fields[0].model.as_deref(), Some("fastapi:ProductOut"));
    assert_eq!(api.excluded[0].operation, "fastapi:GET /healthz");

    let api = api_of(&fixture("api-frameworks/flask"));
    assert_eq!(
        ids(&api),
        [
            "flask:GET /api/posts",
            "flask:POST /api/posts",
            "flask:GET /api/posts/{post_id}",
            "flask:PUT /api/posts/{post_id}"
        ]
    );
    let detail = op(&api, "flask:PUT /api/posts/{post_id}");
    assert_eq!(
        (params(detail), errors(detail), auth(detail)),
        (
            vec!["path post_id: int".to_string()],
            vec![(Some(404), None)],
            vec!["authenticated: login_required".to_string()]
        )
    );
    assert_eq!(op(&api, "flask:POST /api/posts").success_status, Some(201));
    assert_eq!(params(op(&api, "flask:GET /api/posts")), ["query tag: string?"]);
}

#[test]
fn go_frameworks() {
    let api = api_of(&fixture("api-frameworks/go-chi"));
    assert_eq!(
        ids(&api),
        [
            "go-chi:DELETE /api/admin/tasks/{taskID}",
            "go-chi:GET /api/tasks",
            "go-chi:POST /api/tasks",
            "go-chi:GET /api/tasks/{taskID}",
            // Mounted under an env-var prefix, so the path is what is known of it.
            "go-chi:GET /reports/daily"
        ],
        "exactly these: `fcgi.go` reassigns a parameter to \"/\" and calls an outbound \
         `.Post(p, …)`, which must not register a `POST /` handled by `int64`"
    );
    let create = op(&api, "go-chi:POST /api/tasks");
    assert_eq!((create.success_status, create.confidence), (Some(201), Confidence::Typed));
    assert_eq!(auth(create), ["authenticated: requireAuth"]);
    assert_eq!(
        fields(model(&api, "go-chi:createTaskRequest")),
        [
            "title: string {at least 3 characters; at most 140 characters}",
            "priority: int? {must be ≥ 1; must be ≤ 5}",
            "labels: []string? {at most 10 items}"
        ]
    );
    assert_eq!(model(&api, "go-chi:Task").fields[2].doc.as_deref(), Some("Done is true once the task is completed."));
    assert_eq!(auth(op(&api, "go-chi:DELETE /api/admin/tasks/{taskID}")), ["role: requireAdmin"]);
    assert_eq!(params(op(&api, "go-chi:GET /api/tasks")), ["query status: string?"]);
    assert_eq!(errors(op(&api, "go-chi:GET /api/tasks/{taskID}")), [(Some(404), Some("task not found"))]);

    let api = api_of(&fixture("api-frameworks/gin"));
    assert_eq!(ids(&api), ["gin:POST /api/v1/admin/books", "gin:GET /api/v1/books", "gin:GET /api/v1/books/{id}"]);
    let create = op(&api, "gin:POST /api/v1/admin/books");
    assert_eq!(auth(create), ["authenticated: AuthRequired()"]);
    assert_eq!(
        fields(model(&api, "gin:NewBook")),
        ["title: string {at most 200 characters}", "author: string", "isbn: string? {exactly 13 characters}"]
    );
    assert_eq!(errors(op(&api, "gin:GET /api/v1/books/{id}")), [(Some(404), Some("book not found"))]);
    assert_eq!(params(op(&api, "gin:GET /api/v1/books")), ["query page: string?"]);
}

#[test]
fn rust_frameworks() {
    let root = fixture("api-frameworks/axum");
    let api = api_of(&root);
    assert_eq!(
        ids(&api),
        [
            "axum:GET /api/accounts",
            "axum:POST /api/accounts",
            "axum:GET /api/accounts/{id}",
            "axum:DELETE /api/admin/accounts/{id}"
        ]
    );
    let open = op(&api, "axum:POST /api/accounts");
    assert_eq!((open.success_status, open.confidence), (Some(201), Confidence::Typed));
    assert_eq!(errors(open), [(Some(422), None)]);
    assert_eq!(
        fields(model(&api, "axum:OpenAccount")),
        [
            "ownerEmail: String {must be a valid email}",
            "nickname: String {at least 3 characters; at most 40 characters}",
            "openingDepositCents: i64? {must be ≥ 0; must be ≤ 1000000}",
            "referralCode: Option<String>?",
        ]
    );
    assert_eq!(fields(model(&api, "axum:Account"))[3], "type: String", "serde rename beats rename_all");
    assert_eq!(
        params(op(&api, "axum:GET /api/accounts")),
        ["query page: Option<u32>?", "query per_page: Option<u32>?"]
    );
    let close = op(&api, "axum:DELETE /api/admin/accounts/{id}");
    assert_eq!((close.params[0].code_name.as_deref(), close.success_status), (Some("account_id"), Some(204)));
    assert_eq!(auth(close), ["role: accounts::require_admin"]);
    assert_eq!(api.excluded[0].operation, "axum:GET /healthz");

    let api = api_of(&fixture("api-frameworks/actix"));
    assert_eq!(
        ids(&api),
        [
            "actix:GET /api/v1/items",
            "actix:POST /api/v1/items",
            "actix:GET /api/v1/items/{sku}",
            "actix:POST /api/v1/items/{sku}/restock"
        ]
    );
    assert!(api.operations.iter().all(|o| !o.path_partial));
    let create = op(&api, "actix:POST /api/v1/items");
    assert_eq!((create.success_status, create.confidence), (Some(201), Confidence::Typed));
    assert_eq!(errors(op(&api, "actix:GET /api/v1/items/{sku}")), [(Some(404), None)]);
    assert_eq!(op(&api, "actix:POST /api/v1/items/{sku}/restock").success_status, Some(202));
}

#[test]
fn java_spring_mvc_contracts_feign_and_security() {
    let root = fixture("api-frameworks/spring-mvc");
    let api = api_of(&root);
    assert_eq!(
        ids(&api),
        [
            "inventory-service:GET /inventory/{sku}",
            "inventory-service:POST /inventory/{sku}/reservations",
            "orders-service:GET /api/orders",
            "orders-service:POST /api/orders",
            "orders-service:GET /api/orders/{id}",
            "orders-service:DELETE /api/orders/{id}",
        ]
    );
    // Context path + class @RequestMapping + method mapping. The probe is excluded,
    // and so are the two server-rendered pages — named with a reason, not silently dropped.
    assert_eq!(
        api.excluded.iter().map(|x| (x.operation.as_str(), x.reason.as_str())).collect::<Vec<_>>(),
        [
            ("orders-service:GET /api/internal/health", "health / liveness probe"),
            ("orders-service:GET /api/ui/orders", "server-rendered view, not an API operation"),
            ("orders-service:GET /api/ui/orders/{id}", "server-rendered view, not an API operation"),
        ]
    );
    assert_eq!(api.excluded[1].evidence.symbol_name.as_deref(), Some("list"));

    let list = op(&api, "orders-service:GET /api/orders");
    assert_eq!((list.framework.as_str(), list.path_partial), ("spring", false));
    assert_eq!(params(list), ["query status: OrderStatus?", "query limit: int?"]);
    assert_eq!(list.params[0].rules[0].statement, "one of: PENDING, PAID, SHIPPED");
    assert_eq!(list.params[1].rules[0].statement, "must be ≤ 100");
    assert_eq!(
        list.response.as_ref().map(|t| (t.model.as_deref(), t.collection)),
        Some((Some("orders-service:OrderDto"), true))
    );
    assert_eq!(list.summary.as_deref(), Some("Lists the caller's orders, newest first."));
    assert_eq!(auth(list), ["authenticated: authenticated()"], "from requestMatchers(\"/orders/**\").authenticated()");
    assert!(line_of(&root, &list.auth[0].evidence).contains("requestMatchers(\"/orders/**\")"));

    let create = op(&api, "orders-service:POST /api/orders");
    assert_eq!((create.success_status, create.confidence), (Some(201), Confidence::Typed));
    assert_eq!(params(create), ["header Idempotency-Key: String"]);
    assert_eq!(create.params[0].code_name.as_deref(), Some("idempotencyKey"));
    assert_eq!(create.request_body.as_ref().unwrap().model.as_deref(), Some("orders-service:CreateOrderRequest"));
    assert_eq!(errors(create), [(Some(402), Some("payment declined")), (Some(409), Some("out of stock"))]);
    assert!(
        line_of(&root, &create.errors[0].evidence).contains("throw new PaymentDeclinedException"),
        "advice mapping, one call deep"
    );
    assert_eq!(
        fields(model(&api, "orders-service:CreateOrderRequest")),
        [
            "customerId: String {must not be blank; at most 64 characters}",
            "lines: List<OrderLine> {must not be empty; at most 50 items}",
            "contactEmail: String? {must be a valid email}",
            "coupon_code: String? {must match [A-Z0-9]{6}}",
        ]
    );
    let req = model(&api, "orders-service:CreateOrderRequest");
    assert_eq!(
        (req.fields[0].doc.as_deref(), req.fields[3].code_name.as_deref()),
        (Some("Customer placing the order."), Some("couponCode"))
    );
    assert_eq!(req.fields[1].model.as_deref(), Some("orders-service:OrderLine"));
    assert_eq!(
        fields(model(&api, "orders-service:OrderLine")),
        ["sku: String {must not be blank}", "quantity: int {must be ≥ 1; must be ≤ 99}"]
    );
    assert_eq!(fields(model(&api, "orders-service:OrderDto"))[2], "total_cents: long");

    let get = op(&api, "orders-service:GET /api/orders/{id}");
    assert_eq!((params(get), get.params[0].code_name.as_deref()), (vec!["path id: Long".to_string()], Some("orderId")));
    assert_eq!(errors(get), [(Some(404), Some("order not found"))], "@ResponseStatus exception thrown in the service");

    let cancel = op(&api, "orders-service:DELETE /api/orders/{id}");
    assert_eq!((cancel.success_status, cancel.response.is_none()), (Some(204), true));
    assert_eq!(auth(cancel), ["role: hasRole('ADMIN')"], "annotation beats the matcher");

    let reserve = op(&api, "inventory-service:POST /inventory/{sku}/reservations");
    assert_eq!((reserve.success_status, reserve.confidence), (Some(201), Confidence::Typed));
    assert_eq!(fields(model(&api, "inventory-service:ReserveRequest")), ["quantity: int {must be greater than 0}"]);
    let stock = op(&api, "inventory-service:GET /inventory/{sku}");
    assert_eq!(
        stock.response.as_ref().unwrap().model.as_deref(),
        Some("inventory-service:StockDto"),
        "@RequestMapping(method = GET)"
    );

    // @FeignClient(name = "inventory-service") resolves by spring.application.name, with response drift.
    assert_eq!(api.client_calls.len(), 1);
    let call = &api.client_calls[0];
    assert_eq!(
        (
            call.method.as_str(),
            call.path.as_str(),
            call.target_unit.as_deref(),
            call.operation.as_deref(),
            call.caller.as_deref()
        ),
        (
            "POST",
            "/inventory/{sku}/reservations",
            Some("inventory-service"),
            Some("inventory-service:POST /inventory/{sku}/reservations"),
            Some("InventoryClient.reserve")
        )
    );
    assert_eq!(call.drift, ["expiresAt"]);
}

#[test]
fn feign_clients_resolve_services_by_application_name() {
    // `@FeignClient(name = "statistics-service")` → the unit whose spring.application.name it is,
    // even though that unit publishes no matching operation.
    let api = api_of(&fixture("real-world/spring-cloud"));
    let calls: Vec<_> = api
        .client_calls
        .iter()
        .map(|c| (c.unit.as_str(), c.method.as_str(), c.path.as_str(), c.target_unit.as_deref(), c.caller.as_deref()))
        .collect();
    assert_eq!(calls, [("accounts", "PUT", "/statistics/{accountName}", Some("stats"), Some("StatsClient.update"))]);
    assert_eq!(ids(&api), ["accounts:GET /accounts/{name}"]);
}

#[test]
fn java_webflux_quarkus_and_micronaut() {
    let root = fixture("api-frameworks/spring-webflux-functional");
    let api = api_of(&root);
    assert_eq!(
        ids(&api),
        [
            "spring-webflux-functional:GET /api/books",
            "spring-webflux-functional:POST /api/books",
            "spring-webflux-functional:GET /api/books/{isbn}"
        ]
    );
    let list = op(&api, "spring-webflux-functional:GET /api/books");
    assert_eq!((list.framework.as_str(), list.handler.name.as_str()), ("spring-webflux", "list"));
    assert!(line_of(&root, &list.evidence).contains(".GET(\"\", handler::list)"));
    assert_eq!(
        list.response.as_ref().map(|t| (t.model.as_deref(), t.collection)),
        Some((Some("spring-webflux-functional:Book"), true))
    );
    let create = op(&api, "spring-webflux-functional:POST /api/books");
    assert_eq!(create.success_status, Some(201));
    assert_eq!(
        fields(model(&api, create.request_body.as_ref().unwrap().model.as_deref().unwrap())),
        [
            "isbn: String {must not be blank; at least 10 characters; at most 13 characters}",
            "title: String {must not be blank}"
        ]
    );
    assert_eq!(errors(op(&api, "spring-webflux-functional:GET /api/books/{isbn}")), [(Some(404), None)]);
    assert_eq!(api.excluded[0].operation, "spring-webflux-functional:GET /healthz");

    let root = fixture("api-frameworks/quarkus-jaxrs");
    let api = api_of(&root);
    assert_eq!(
        ids(&api),
        [
            "quarkus-jaxrs:GET /api/fruits",
            "quarkus-jaxrs:POST /api/fruits",
            "quarkus-jaxrs:GET /api/fruits/{id}",
            "quarkus-jaxrs:DELETE /api/fruits/{id}"
        ]
    );
    let list = op(&api, "quarkus-jaxrs:GET /api/fruits");
    assert_eq!(list.framework, "quarkus");
    assert_eq!(params(list), ["query season: Season?", "query limit: int?"]);
    assert_eq!(
        fields(model(&api, "quarkus-jaxrs:Fruit"))[2],
        "season: Season? {one of: SPRING, SUMMER, AUTUMN, WINTER}"
    );
    let create = op(&api, "quarkus-jaxrs:POST /api/fruits");
    assert_eq!(create.success_status, Some(201));
    assert_eq!(
        create.request_body.as_ref().unwrap().model.as_deref(),
        Some("quarkus-jaxrs:NewFruit"),
        "entity parameter"
    );
    assert_eq!(errors(create), [(Some(422), Some("not in season"))], "ExceptionMapper");
    assert_eq!(auth(create), ["role: admin"]);
    assert_eq!(errors(op(&api, "quarkus-jaxrs:GET /api/fruits/{id}")), [(Some(404), Some("fruit not found"))]);
    let delete = op(&api, "quarkus-jaxrs:DELETE /api/fruits/{id}");
    assert_eq!((delete.success_status, auth(delete)), (Some(204), vec!["authenticated: @Authenticated".to_string()]));
    assert_eq!(api.excluded[0].operation, "quarkus-jaxrs:GET /api/healthz");
    assert!(
        api.client_calls.is_empty(),
        "configKey `prices` names no service in this repository: {:?}",
        api.client_calls
    );

    let root = fixture("api-frameworks/micronaut");
    let api = api_of(&root);
    assert_eq!(
        ids(&api),
        [
            "micronaut:GET /v1/pets",
            "micronaut:POST /v1/pets",
            "micronaut:GET /v1/pets/{id}",
            "micronaut:DELETE /v1/pets/{id}"
        ]
    );
    let list = op(&api, "micronaut:GET /v1/pets");
    assert_eq!(
        (list.framework.as_str(), params(list)),
        ("micronaut", vec!["query max: int?".to_string(), "query species: String?".to_string()])
    );
    assert_eq!(auth(list), ["authenticated: @Secured"]);
    assert_eq!(params(op(&api, "micronaut:GET /v1/pets/{id}")), ["path id: Long"], "bound by name");
    let save = op(&api, "micronaut:POST /v1/pets");
    assert_eq!((save.success_status, save.confidence), (Some(201), Confidence::Typed));
    assert_eq!(
        fields(model(&api, "micronaut:NewPet")),
        ["name: String {must not be blank}", "species: String? {must match cat|dog}"]
    );
    let remove = op(&api, "micronaut:DELETE /v1/pets/{id}");
    assert_eq!(
        (remove.success_status, errors(remove), auth(remove)),
        (Some(204), vec![(Some(404), None)], vec!["role: ROLE_ADMIN".to_string()])
    );
}

#[test]
fn extraction_is_deterministic() {
    for name in ["polyglot-shop", "real-world/uv-workspace", "api-frameworks/nestjs", "api-frameworks/spring-mvc"] {
        let a = serde_json::to_string(&api_of(&fixture(name))).unwrap();
        let b = serde_json::to_string(&api_of(&fixture(name))).unwrap();
        assert_eq!(a, b, "{name}");
    }
}

#[test]
fn plain_scans_skip_behaviour() {
    let report = scan(&fixture("polyglot-shop"), &ScanOptions::default()).unwrap();
    assert!(report.api.is_none());
}

/// gorilla/mux builder chains put the path in its own link rather than in the
/// route call's arguments, so the registration reads as no route at all: MinIO's
/// entire S3 surface went undocumented while its metrics router did not. The
/// handler is behind a middleware wrapper, and routes that share a method and
/// path are told apart only by the query they match on.
#[test]
fn go_mux_builder_chains_register_routes() {
    let api = api_of(&fixture("real-world/go-mux-builder"));
    let ops = &api.operations;
    let by = |m: &str, p: &str, sel: Option<&str>| {
        ops.iter().find(|o| o.method == m && o.path == p && o.selector.as_deref() == sel).unwrap_or_else(|| {
            panic!(
                "{m} {p}{} missing; have {:?}",
                sel.map(|s| format!(" ?{s}")).unwrap_or_default(),
                ops.iter().map(|o| format!("{} {} {:?}", o.method, o.path, o.selector)).collect::<Vec<_>>()
            )
        })
    };

    // The subrouter prefixes resolve, so the path is whole.
    let get = by("GET", "/{bucket}/{object}", None);
    assert_eq!(get.handler.name, "GetObjectHandler", "the wrapper is not the handler");
    assert!(!get.path_partial, "both subrouter prefixes are literal");

    // Same method and path: only the query distinguishes them.
    let tagging = by("GET", "/{bucket}/{object}", Some("tagging"));
    assert_eq!(tagging.handler.name, "GetObjectTaggingHandler");
    assert_ne!(get.id, tagging.id, "routes differing only by query must not collide");

    assert_eq!(by("HEAD", "/{bucket}/{object}", None).handler.name, "HeadObjectHandler");
    assert_eq!(by("PUT", "/{bucket}/{object}", None).handler.name, "PutObjectHandler");
    assert_eq!(by("GET", "/", None).handler.name, "ListBucketsHandler");
}

/// A route under a prefix that could not be resolved is a suffix of the real
/// path, not the path. The flag `eval_path` returns was kept at the leaf but
/// discarded at every prefix site — `r.Route(os.Getenv("BASE")+"/reports", …)`
/// produced `/reports/daily` and asserted it as exact.
#[test]
fn go_routes_under_an_unresolved_prefix_are_partial() {
    let api = api_of(&fixture("api-frameworks/go-chi"));
    let under_prefix = api.operations.iter().find(|o| o.path.ends_with("/reports/daily")).unwrap_or_else(|| {
        panic!("route missing; have {:?}", api.operations.iter().map(|o| &o.path).collect::<Vec<_>>())
    });
    assert!(under_prefix.path_partial, "`{}` sits under an unresolved prefix and must say so", under_prefix.path);

    // A route whose prefix is fully known stays exact.
    let known = api.operations.iter().find(|o| o.path.contains("/tasks")).expect("the tasks routes are documented");
    assert!(!known.path_partial, "`{}` is fully resolved and should not be marked partial", known.path);
}

#[test]
fn csharp_aspnet_core_controllers_and_minimal_api() {
    let root = fixture("api-frameworks/aspnet");
    let api = api_of(&root);
    assert_eq!(
        ids(&api),
        [
            "catalog:GET /api/Products",
            "catalog:POST /api/Products",
            "catalog:GET /api/Products/{id}",
            "catalog:DELETE /api/Products/{id}",
            "catalog:GET /v1/orders/by-reference",
            "catalog:PUT /v1/orders/{reference}"
        ],
        "`[Route(\"api/[controller]\")]` expands the token from the class name"
    );
    assert_eq!(
        api.excluded.iter().map(|e| (e.operation.as_str(), e.reason.as_str())).collect::<Vec<_>>(),
        [("catalog:GET /healthz", "health / liveness probe")],
        "the minimal-API probe is read, then left out on purpose"
    );

    let list = op(&api, "catalog:GET /api/Products");
    assert_eq!(params(list), ["query limit: int?", "query category: Category?"], "defaults and `?` make them optional");
    assert_eq!(list.response.as_ref().map(|t| t.type_name.as_str()), Some("List<Product>"));
    assert!(list.response.as_ref().is_some_and(|t| t.collection), "`List<T>` is a collection");
    assert_eq!(list.summary.as_deref(), Some("List products, newest first."), "prose out of `<summary>`");
    assert!(auth(list).is_empty(), "`[AllowAnonymous]` overrides the controller's `[Authorize]`");

    let create = op(&api, "catalog:POST /api/Products");
    assert_eq!(create.request_body.as_ref().map(|t| t.type_name.as_str()), Some("NewProduct"));
    assert_eq!(create.success_status, Some(201), "from `[ProducesResponseType(StatusCodes.Status201Created)]`");
    assert_eq!(auth(create), ["session: authenticated"]);

    let get = op(&api, "catalog:GET /api/Products/{id}");
    assert_eq!(params(get), ["path id: int"], "`{{id:int}}` keeps the name, drops the constraint");
    assert_eq!(errors(get), [(Some(404), None)], "`return NotFound();`");

    let remove = op(&api, "catalog:DELETE /api/Products/{id}");
    assert_eq!(remove.success_status, Some(204), "`return NoContent();`");
    assert_eq!(auth(remove), ["session: authenticated", "role: admin"], "class `[Authorize]` plus the action's roles");

    let lookup = op(&api, "catalog:GET /v1/orders/by-reference");
    assert_eq!(params(lookup), ["query ref: string"], "`[FromQuery(Name = \"ref\")]` renames it");
    assert_eq!(lookup.params[0].code_name.as_deref(), Some("reference"), "the C# identifier is kept alongside");
    assert_eq!(errors(lookup), [(Some(404), None)], "`throw new KeyNotFoundException`");
    assert_eq!(auth(lookup), ["role: policy orders:write"]);

    let replace = op(&api, "catalog:PUT /v1/orders/{reference}");
    assert_eq!(
        params(replace),
        ["path reference: string", "header X-Idempotency-Key: string"],
        "a renamed header parameter"
    );
    assert_eq!(
        replace.params[1].rules.iter().map(|r| r.statement.as_str()).collect::<Vec<_>>(),
        ["required"],
        "`[Required]` on a parameter is a rule, not just a flag"
    );
    assert_eq!(replace.request_body.as_ref().map(|t| t.type_name.as_str()), Some("Order"));
    assert_eq!(errors(replace), [(Some(400), None)], "`throw new ArgumentException`");

    assert_eq!(
        fields(model(&api, "catalog:Product")),
        [
            "Id: int {primary key}",
            "Name: string {required; max length 120}",
            "Price: decimal {range 0–100000}",
            "Category: Category"
        ],
        "data annotations become rules"
    );
    assert_eq!(
        fields(model(&api, "catalog:NewProduct")),
        [
            "Name: string {required; length 2–120}",
            "Price: decimal {range 0–100000}",
            "SupplierEmail: string? {format: email}"
        ],
        "`[StringLength(120, MinimumLength = 2)]` is one rule, and `string?` is optional"
    );
    let order = model(&api, "catalog:Order");
    assert_eq!(
        fields(order),
        [
            "Id: int {primary key}",
            "customer_email: string {required; format: email}",
            "Currency: string {pattern \"^[A-Z]{3}$\"}",
            "Item: Product?"
        ],
        "`[JsonPropertyName]` renames the wire field"
    );
    assert_eq!(order.fields[1].code_name.as_deref(), Some("CustomerEmail"));
    assert_eq!(order.fields[3].model.as_deref(), Some("catalog:Product"), "a nested model resolves");
    assert_eq!(order.doc.as_deref(), Some("A placed order."));
}
