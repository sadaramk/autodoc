//! Behaviour traces: what happens, in order, when an operation is invoked.
//!
//! Starting at an operation's handler, the tracer walks the handler body in
//! line order, expanding calls to functions of the same unit inline, and
//! records the effects it meets: outbound calls to other units (expanded into
//! the callee's handler when the call resolves to a known operation), entity
//! reads and writes, and event publishes. Every step keeps the source line it
//! came from, so a sequence diagram drawn from a trace is evidence-pinned
//! message by message.
//!
//! Flows also start where requests don't: scheduled jobs, message and task
//! consumers, in-process event listeners and startup hooks (see `entry`).
//! When a flow publishes an event that another handler consumes, the
//! consumer's trace continues the same flow, marked asynchronous.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::api::{ApiModel, Operation};
use crate::catalog::InfraCategory;
use crate::data::DataModel;
use crate::extract::{Receiver, SymbolKind, TypeDef};
use crate::scan::{EvidenceRef, InfraSummary, Relationship};
use crate::source::SourceIndex;
use autodoc_ir::EdgeType;

/// Participants and messages a readable sequence diagram can hold.
pub const MAX_PARTICIPANTS: usize = 8;
pub const MAX_STEPS: usize = 24;
/// Flows worth a figure per repository (the most involved ones).
pub const MAX_FLOWS: usize = 6;
const MAX_LOCAL_DEPTH: usize = 5;
const MAX_REMOTE_DEPTH: usize = 3;
/// Publish → consume hops followed from one flow.
const MAX_EVENT_HOPS: usize = 2;
/// Consumers followed per published event.
const MAX_CONSUMERS: usize = 2;

/// What starts a flow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerKind {
    /// An HTTP / gRPC operation.
    #[default]
    Http,
    /// A cron expression, fixed rate, ticker or task-beat schedule.
    Schedule,
    /// A message, queue or task consumer.
    Message,
    /// An in-process application event listener.
    Event,
    /// Runs when the application starts.
    Startup,
}

impl TriggerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TriggerKind::Http => "http",
            TriggerKind::Schedule => "schedule",
            TriggerKind::Message => "message",
            TriggerKind::Event => "event",
            TriggerKind::Startup => "startup",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub kind: TriggerKind,
    /// `cron 0 * * * *`, `Kafka topic order.placed`, `OrderCompleted`; the route for HTTP.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    /// `flow:<operation id>`.
    pub id: String,
    /// `POST /api/checkout`.
    pub title: String,
    /// Operation the flow starts at (`<unit>:<METHOD> <path>`), or
    /// `<unit>:<trigger kind> <handler>` for flows that don't start with a request.
    pub entry: String,
    #[serde(default)]
    pub trigger: Trigger,
    pub participants: Vec<Participant>,
    pub steps: Vec<Step>,
    /// Functions the trace walked through, as `unit:name`, in visit order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<String>,
    /// Ranking used to pick which flows get a figure.
    pub score: usize,
    /// What the tracer left out (depth limits, truncation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    /// Unit or infrastructure id (as in the container diagram).
    pub id: String,
    pub label: String,
    /// `unit` or `infra`.
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StepKind {
    Call,
    Reply,
    Read,
    Write,
    Publish,
    /// The scheduler, runtime or event dispatch starting a background flow.
    Trigger,
    /// A broker (or the in-process event bus) handing an event to a consumer.
    Deliver,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub from: String,
    pub to: String,
    pub kind: StepKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    /// Operation this step invokes, for calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// Function the step happens in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<String>,
    /// Happens after the originating request has moved on (event continuation).
    #[serde(default, rename = "async", skip_serializing_if = "std::ops::Not::not")]
    pub asynchronous: bool,
    pub evidence: EvidenceRef,
}

#[derive(Clone)]
struct Func<'a> {
    path: &'a str,
    name: String,
    start: u32,
    end: u32,
    /// The type this method hangs off, when it is a method.
    receiver: Option<Receiver>,
}

enum Effect<'a> {
    Local(Func<'a>),
    /// `site`: the line making the call when the client call itself is declared elsewhere (an interface).
    Remote {
        call: usize,
        site: Option<u32>,
    },
    Access {
        entity: usize,
        write: bool,
        evidence: EvidenceRef,
    },
    Relation {
        rel: usize,
        evidence: EvidenceRef,
    },
    /// An in-process application event published here.
    Event {
        event_type: String,
    },
    /// A call on a field that holds an infrastructure client.
    Store {
        infra: String,
        write: bool,
        label: String,
        evidence: EvidenceRef,
    },
}

struct Tracer<'a> {
    index: &'a SourceIndex<'a>,
    api: &'a ApiModel,
    data: Option<&'a DataModel>,
    infra: &'a [InfraSummary],
    relationships: &'a [Relationship],
    /// Function symbols per unit: name → definitions.
    functions: BTreeMap<&'a str, BTreeMap<&'a str, Vec<Func<'a>>>>,
    /// Method bodies per unit, keyed by the type they hang off and their name.
    methods: BTreeMap<&'a str, BTreeMap<(String, String), Vec<Func<'a>>>>,
    /// Named types per unit, for walking a receiver expression to its type.
    types: BTreeMap<&'a str, BTreeMap<String, TypeDef>>,
    /// Fields that hold an infrastructure client: (type, field) → infra id.
    infra_fields: BTreeMap<&'a str, BTreeMap<(String, String), String>>,
    /// Methods keyed by a parameter type, for dispatch that names the message
    /// rather than the handler: `mediatr.Send[*CreateOrder](…)`.
    handlers: BTreeMap<&'a str, BTreeMap<String, Vec<Func<'a>>>>,
    /// All symbols per file, for innermost-enclosing lookups.
    spans: BTreeMap<&'a str, Vec<(u32, u32, &'a str)>>,
    /// Interface bodies per file: methods declared there have no implementation.
    interfaces: BTreeMap<&'a str, Vec<(u32, u32)>>,
    /// Non-HTTP entry points (consumers, schedules, listeners).
    entries: Vec<crate::entry::Entry<'a>>,
}

pub fn trace(
    index: &SourceIndex,
    api: &ApiModel,
    data: Option<&DataModel>,
    infra: &[InfraSummary],
    relationships: &[Relationship],
) -> Vec<Flow> {
    let mut functions: BTreeMap<&str, BTreeMap<&str, Vec<Func>>> = BTreeMap::new();
    let mut methods: BTreeMap<&str, BTreeMap<(String, String), Vec<Func>>> = BTreeMap::new();
    let mut types: BTreeMap<&str, BTreeMap<String, TypeDef>> = BTreeMap::new();
    let mut infra_fields: BTreeMap<&str, BTreeMap<(String, String), String>> = BTreeMap::new();
    let mut handlers: BTreeMap<&str, BTreeMap<String, Vec<Func>>> = BTreeMap::new();
    let mut spans: BTreeMap<&str, Vec<(u32, u32, &str)>> = BTreeMap::new();
    let mut interfaces: BTreeMap<&str, Vec<(u32, u32)>> = BTreeMap::new();
    for f in &index.files {
        for t in &f.facts.types {
            types.entry(f.unit).or_default().entry(t.name.clone()).or_insert_with(|| t.clone());
            // A field typed from an infrastructure package is a handle on it:
            // `client *firestore.Client` makes `r.client.…` a Firestore call.
            for field in t.fields.iter().filter(|x| !x.name.is_empty()) {
                let Some(q) = &field.qualifier else { continue };
                let Some(imp) = f.facts.imports.iter().find(|i| {
                    let pkg = crate::catalog::import_package(&i.specifier);
                    pkg == q || i.specifier.rsplit('/').next() == Some(q.as_str())
                }) else {
                    continue;
                };
                let pkg = crate::catalog::import_package(&imp.specifier);
                if let Some(kind) = crate::catalog::infra_for_package(pkg) {
                    if infra.iter().any(|i| i.id == kind.id()) {
                        infra_fields
                            .entry(f.unit)
                            .or_default()
                            .insert((t.name.clone(), field.name.clone()), kind.id().to_string());
                    }
                }
            }
        }
        for s in f.facts.symbols.iter().filter(|s| s.kind == SymbolKind::Interface) {
            interfaces.entry(f.path).or_default().push((s.start_line, s.end_line));
        }
        for s in &f.facts.symbols {
            if !matches!(s.kind, SymbolKind::Function | SymbolKind::Method) {
                continue;
            }
            functions.entry(f.unit).or_default().entry(s.name.as_str()).or_default().push(Func {
                path: f.path,
                name: s.name.clone(),
                start: s.start_line,
                end: s.end_line,
                receiver: s.receiver.clone(),
            });
            // A method that takes a message type is a candidate handler for it.
            if s.receiver.is_some() {
                for t in s.params.iter().filter(|t| !GENERIC_PARAM_TYPES.contains(&t.as_str())) {
                    handlers.entry(f.unit).or_default().entry(t.clone()).or_default().push(Func {
                        path: f.path,
                        name: s.name.clone(),
                        start: s.start_line,
                        end: s.end_line,
                        receiver: s.receiver.clone(),
                    });
                }
            }
            if let Some(r) = &s.receiver {
                methods.entry(f.unit).or_default().entry((r.type_name.clone(), s.name.clone())).or_default().push(
                    Func {
                        path: f.path,
                        name: s.name.clone(),
                        start: s.start_line,
                        end: s.end_line,
                        receiver: s.receiver.clone(),
                    },
                );
            }
            spans.entry(f.path).or_default().push((s.start_line, s.end_line, s.name.as_str()));
        }
    }
    let entries = crate::entry::discover(index, infra, relationships);
    let tracer = Tracer {
        index,
        api,
        data,
        infra,
        relationships,
        functions,
        methods,
        types,
        infra_fields,
        handlers,
        spans,
        interfaces,
        entries,
    };

    let called: BTreeSet<&str> = api.client_calls.iter().filter_map(|c| c.operation.as_deref()).collect();
    let mut flows: Vec<Flow> = api
        .operations
        .iter()
        .filter(|op| op.protocol == "http" || op.protocol == "grpc")
        .map(|op| {
            let mut flow = tracer.flow(op);
            let callers = called.contains(op.id.as_str()) as usize;
            flow.score = flow.participants.len() * 4 + flow.steps.len().min(MAX_STEPS) + callers * 6;
            flow
        })
        .collect();
    let mut ids: BTreeSet<String> = flows.iter().map(|f| f.id.clone()).collect();
    for entry in &tracer.entries {
        let mut flow = tracer.background_flow(entry);
        // Starting the runtime is only worth documenting when startup does something.
        if entry.kind == TriggerKind::Startup && flow.steps.len() <= 1 {
            continue;
        }
        let base = flow.id.clone();
        let mut n = 1;
        while !ids.insert(flow.id.clone()) {
            n += 1;
            flow.id = format!("{base}-{n}");
            flow.entry = format!("{} ({n})", flow.entry.trim_end_matches(&format!(" ({})", n - 1)));
        }
        flow.score = flow.participants.len() * 4 + flow.steps.len().min(MAX_STEPS);
        flows.push(flow);
    }
    flows.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    flows
}

/// A request and its reply alone say nothing a table doesn't; a background
/// flow needs at least one effect beyond its trigger.
pub fn worth_drawing(flow: &Flow) -> bool {
    if flow.trigger.kind != TriggerKind::Http {
        return flow.steps.len() >= 2;
    }
    let effects = flow.steps.iter().filter(|s| s.kind != StepKind::Reply).count().saturating_sub(1);
    effects >= 2 || (effects >= 1 && flow.participants.len() >= 3)
}

/// Topic or event type named by a publish step (`publishes order.placed`).
pub fn published_topic(step: &Step) -> Option<&str> {
    step.label.strip_prefix("publishes ").map(str::trim).filter(|t| !t.is_empty())
}

impl<'a> Tracer<'a> {
    fn flow(&self, op: &'a Operation) -> Flow {
        let mut steps = Vec::new();
        let mut notes = Vec::new();
        // Who calls it: the first client call resolved to this operation.
        let caller =
            self.api.client_calls.iter().find(|c| c.operation.as_deref() == Some(op.id.as_str()) && c.unit != op.unit);
        let origin = match caller {
            Some(c) => {
                steps.push(Step {
                    from: c.unit.clone(),
                    to: op.unit.clone(),
                    kind: StepKind::Call,
                    label: format!("{} {}", op.method, op.path),
                    payload: op.request_body.as_ref().map(|t| t.type_name.clone()),
                    operation: Some(op.id.clone()),
                    within: c.caller.clone(),
                    asynchronous: false,
                    evidence: c.evidence.clone(),
                });
                c.unit.clone()
            }
            None => "client".to_string(),
        };
        if caller.is_none() {
            steps.push(Step {
                from: origin.clone(),
                to: op.unit.clone(),
                kind: StepKind::Call,
                label: format!("{} {}", op.method, op.path),
                payload: op.request_body.as_ref().map(|t| t.type_name.clone()),
                operation: Some(op.id.clone()),
                within: None,
                asynchronous: false,
                evidence: op.evidence.clone(),
            });
        }
        let mut visiting = BTreeSet::new();
        let mut functions = Vec::new();
        self.expand_operation(op, &mut steps, &mut visiting, 0, &mut notes, &mut functions);
        steps.push(self.reply(op, &origin));
        let mut followed = BTreeSet::new();
        self.continue_events(&mut steps, 0, 0, &mut followed, &mut notes, &mut functions);
        let (participants, steps) = self.finish(steps, &mut notes);
        Flow {
            id: format!("flow:{}", op.id),
            title: format!("{} {}", op.method, op.path),
            entry: op.id.clone(),
            trigger: Trigger {
                kind: TriggerKind::Http,
                label: format!("{} {}", op.method, op.path),
                evidence: Some(op.evidence.clone()),
            },
            participants,
            steps,
            functions,
            score: 0,
            notes,
        }
    }

    /// A flow started by a schedule, message, event or startup hook.
    fn background_flow(&self, e: &crate::entry::Entry<'a>) -> Flow {
        let mut notes = Vec::new();
        let mut functions = Vec::new();
        let (from, kind, label) = match e.kind {
            TriggerKind::Message => (
                e.broker.clone().unwrap_or_else(|| "trigger:broker".into()),
                StepKind::Deliver,
                e.topic.as_ref().map(|t| format!("delivers {t}")).unwrap_or_else(|| e.label.clone()),
            ),
            TriggerKind::Schedule => ("trigger:schedule".into(), StepKind::Trigger, e.label.clone()),
            TriggerKind::Startup => ("trigger:startup".into(), StepKind::Trigger, e.label.clone()),
            TriggerKind::Event => ("trigger:event".into(), StepKind::Trigger, format!("on {}", e.label)),
            TriggerKind::Http => ("client".into(), StepKind::Call, e.label.clone()),
        };
        let mut steps = vec![Step {
            from,
            to: e.unit.to_string(),
            kind,
            label,
            payload: None,
            operation: None,
            within: Some(e.handler.name.clone()),
            asynchronous: false,
            evidence: e.evidence.clone(),
        }];
        let key = entry_key(e);
        let mut followed = BTreeSet::from([key]);
        let dispatched = self.walk_entry(e, &mut steps, &mut notes, &mut functions);
        self.continue_events(&mut steps, 1, 0, &mut followed, &mut notes, &mut functions);
        let (participants, steps) = self.finish(steps, &mut notes);
        // `run` dispatching to `handle_order` is known by what it runs.
        let handler = dispatched.unwrap_or_else(|| e.handler.name.clone());
        Flow {
            id: format!("flow:{}:{}:{}", e.unit, e.kind.as_str(), handler),
            title: format!("{handler} · {}", e.label),
            entry: format!("{}:{} {handler}", e.unit, e.kind.as_str()),
            trigger: Trigger { kind: e.kind, label: e.label.clone(), evidence: Some(e.evidence.clone()) },
            participants,
            steps,
            functions,
            score: 0,
            notes,
        }
    }

    /// Walks an entry's handler, then the function it dispatches to when the
    /// handler only calls a callback it was given (`consumer.run(handle_order)`).
    fn walk_entry(
        &self,
        e: &crate::entry::Entry<'a>,
        steps: &mut Vec<Step>,
        notes: &mut Vec<String>,
        functions: &mut Vec<String>,
    ) -> Option<String> {
        let handler = Func {
            path: e.handler.path,
            name: e.handler.name.clone(),
            start: e.handler.start,
            end: e.handler.end,
            receiver: self.receiver_at(e.handler.path, e.handler.start),
        };
        let mut visiting = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let mut w = Walk { visiting: &mut visiting, remote_depth: 0, seen: &mut seen, notes, functions };
        self.expand_func(e.unit, &handler, steps, 0, &mut w);
        let cb = self.dispatched_callback(e.unit, &handler)?;
        self.expand_func(e.unit, &cb, steps, 1, &mut w);
        Some(cb.name)
    }

    fn dispatched_callback(&self, unit: &str, handler: &Func<'a>) -> Option<Func<'a>> {
        let unit_fns = self.functions.get(unit)?;
        let file = self.index.files.iter().find(|f| f.path == handler.path)?;
        // The handler invokes something it didn't define: a parameter.
        let calls_parameter = file.facts.calls.iter().any(|c| {
            c.line >= handler.start
                && c.line <= handler.end
                && !c.callee.contains(['.', ':'])
                && !unit_fns.contains_key(c.name.as_str())
                && c.name.chars().next().is_some_and(|ch| ch.is_lowercase())
        });
        if !calls_parameter {
            return None;
        }
        for f in self.index.files.iter().filter(|f| f.unit == unit) {
            let sites: Vec<u32> = f.facts.calls.iter().filter(|c| c.name == handler.name).map(|c| c.line).collect();
            if sites.is_empty() {
                continue;
            }
            let text = self.index.read(f.path)?;
            for line in sites {
                let l = text.lines().nth(line.saturating_sub(1) as usize).unwrap_or("");
                let args = l.split_once(&format!("{}(", handler.name)).map(|(_, a)| a).unwrap_or("");
                for id in args.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()) {
                    if id == handler.name {
                        continue;
                    }
                    if let Some(defs) = unit_fns.get(id) {
                        if let Some(d) = defs.iter().find(|d| !self.in_interface(d.path, d.start)) {
                            return Some(d.clone());
                        }
                    }
                }
            }
        }
        None
    }

    /// Appends, for each event published in `steps[from..]`, the traces of the
    /// handlers that consume it (asynchronously), up to `MAX_EVENT_HOPS` deep.
    fn continue_events(
        &self,
        steps: &mut Vec<Step>,
        from: usize,
        depth: usize,
        followed: &mut BTreeSet<String>,
        notes: &mut Vec<String>,
        functions: &mut Vec<String>,
    ) {
        if depth >= MAX_EVENT_HOPS {
            return;
        }
        let publishes: Vec<Step> =
            steps[from.min(steps.len())..].iter().filter(|s| s.kind == StepKind::Publish).cloned().collect();
        for p in publishes {
            let topic = published_topic(&p).map(str::to_string);
            let in_process = p.from == p.to;
            let mut consumers: Vec<&crate::entry::Entry<'a>> = if in_process {
                self.entries
                    .iter()
                    .filter(|e| {
                        e.kind == TriggerKind::Event && e.unit == p.from && e.topic.as_deref() == topic.as_deref()
                    })
                    .collect()
            } else {
                self.entries
                    .iter()
                    .filter(|e| e.kind == TriggerKind::Message && e.broker.as_deref() == Some(p.to.as_str()))
                    .filter(|e| topic.is_some() && e.topic == topic)
                    .collect()
            };
            let mut inferred = false;
            if consumers.is_empty() && !in_process {
                // The topic isn't statically known on one side: a lone consumer of that broker.
                let on_broker: Vec<&crate::entry::Entry<'a>> = self
                    .entries
                    .iter()
                    .filter(|e| e.kind == TriggerKind::Message && e.broker.as_deref() == Some(p.to.as_str()))
                    .filter(|e| e.topic.is_none() || topic.is_none())
                    .collect();
                if on_broker.len() == 1 {
                    consumers = on_broker;
                    inferred = true;
                }
            }
            for c in consumers.into_iter().take(MAX_CONSUMERS) {
                if !followed.insert(entry_key(c)) {
                    continue;
                }
                if inferred {
                    notes.push(format!("{} → {} matched by broker only (topic not statically known)", p.from, c.unit));
                }
                let deliver = Step {
                    from: if in_process { p.from.clone() } else { p.to.clone() },
                    to: c.unit.to_string(),
                    kind: StepKind::Deliver,
                    label: match (&topic, in_process) {
                        (Some(t), true) => format!("{t} → {}", c.handler.name),
                        (Some(t), false) => format!("delivers {t}"),
                        (None, _) => "delivers".into(),
                    },
                    payload: None,
                    operation: None,
                    within: Some(c.handler.name.clone()),
                    asynchronous: true,
                    evidence: c.evidence.clone(),
                };
                let mut sub = vec![deliver];
                self.walk_entry(c, &mut sub, notes, functions);
                self.continue_events(&mut sub, 1, depth + 1, followed, notes, functions);
                for s in &mut sub {
                    s.asynchronous = true;
                }
                steps.extend(sub);
            }
        }
    }

    /// Coalesces, names participants and applies the diagram budget.
    fn finish(&self, mut steps: Vec<Step>, notes: &mut Vec<String>) -> (Vec<Participant>, Vec<Step>) {
        coalesce(&mut steps);
        let mut participants: Vec<Participant> = Vec::new();
        for s in &steps {
            for id in [&s.from, &s.to] {
                if !participants.iter().any(|p| &p.id == id) {
                    participants.push(self.participant(id));
                }
            }
        }
        if participants.len() > MAX_PARTICIPANTS {
            let keep: BTreeSet<String> = participants.iter().take(MAX_PARTICIPANTS).map(|p| p.id.clone()).collect();
            let before = steps.len();
            steps.retain(|s| keep.contains(&s.from) && keep.contains(&s.to));
            participants.truncate(MAX_PARTICIPANTS);
            notes.push(format!(
                "{} step(s) with participants beyond the first {MAX_PARTICIPANTS} left out",
                before - steps.len()
            ));
        }
        if steps.len() > MAX_STEPS {
            notes.push(format!("{} later step(s) left out", steps.len() - MAX_STEPS));
            // Keep the synchronous reply when there is one.
            let reply =
                steps.iter().rposition(|s| s.kind == StepKind::Reply && !s.asynchronous).map(|i| steps.remove(i));
            steps.truncate(MAX_STEPS - reply.is_some() as usize);
            steps.extend(reply);
        }
        (participants, steps)
    }

    fn reply(&self, op: &Operation, to: &str) -> Step {
        let status = op.success_status.map(|s| s.to_string());
        let body =
            op.response.as_ref().map(|t| if t.collection { format!("{}[]", t.type_name) } else { t.type_name.clone() });
        let label = match (&status, &body) {
            (Some(s), Some(b)) => format!("{s} {b}"),
            (Some(s), None) => s.clone(),
            (None, Some(b)) => b.clone(),
            (None, None) => "response".into(),
        };
        Step {
            from: op.unit.clone(),
            to: to.to_string(),
            kind: StepKind::Reply,
            label,
            payload: None,
            operation: Some(op.id.clone()),
            within: Some(op.handler.name.clone()),
            asynchronous: false,
            evidence: op.handler.evidence.clone(),
        }
    }

    fn participant(&self, id: &str) -> Participant {
        // Infrastructure first: a directory of database init scripts can share the store's id.
        if let Some(i) = self.infra.iter().find(|i| i.id == id) {
            return Participant { id: id.into(), label: i.label.clone(), kind: "infra".into() };
        }
        if let Some(u) = self.index.unit(id) {
            return Participant { id: id.into(), label: u.name.clone(), kind: "unit".into() };
        }
        let (label, kind) = match id {
            "client" => ("Client", "actor"),
            "trigger:schedule" => ("Scheduler", "trigger"),
            "trigger:startup" => ("Application start", "trigger"),
            "trigger:event" => ("Application events", "trigger"),
            "trigger:broker" => ("Message broker", "trigger"),
            other => (other, "infra"),
        };
        Participant { id: id.into(), label: label.into(), kind: kind.into() }
    }

    fn handler_func(&self, op: &'a Operation) -> Func<'a> {
        let ev = &op.handler.evidence;
        let path = self.index.files.iter().find(|f| f.path == ev.file_path).map(|f| f.path).unwrap_or("");
        if ev.end_line > ev.start_line {
            return Func {
                path,
                name: op.handler.name.clone(),
                start: ev.start_line,
                end: ev.end_line,
                receiver: self.receiver_at(path, ev.start_line),
            };
        }
        // A registration line only: find the named function in that file, then in the unit.
        let by_name = self.functions.get(op.unit.as_str()).and_then(|m| m.get(op.handler.name.as_str()));
        by_name.and_then(|defs| defs.iter().find(|d| d.path == path).or_else(|| defs.first())).cloned().unwrap_or(
            Func {
                path,
                name: op.handler.name.clone(),
                start: ev.start_line,
                end: ev.end_line,
                receiver: self.receiver_at(path, ev.start_line),
            },
        )
    }

    fn expand_operation(
        &self,
        op: &'a Operation,
        steps: &mut Vec<Step>,
        visiting: &mut BTreeSet<String>,
        remote_depth: usize,
        notes: &mut Vec<String>,
        functions: &mut Vec<String>,
    ) {
        if !visiting.insert(op.id.clone()) {
            return;
        }
        let handler = self.handler_func(op);
        let mut seen_funcs = BTreeSet::new();
        let mut ctx = Walk { visiting, remote_depth, seen: &mut seen_funcs, notes, functions };
        self.expand_func(&op.unit, &handler, steps, 0, &mut ctx);
        visiting.remove(&op.id);
    }

    fn expand_func(&self, unit: &str, func: &Func<'a>, steps: &mut Vec<Step>, local_depth: usize, w: &mut Walk) {
        if !w.seen.insert((func.path.to_string(), func.start)) {
            return;
        }
        let qualified = format!("{unit}:{}", func.name);
        if !w.functions.contains(&qualified) {
            w.functions.push(qualified);
        }
        let mut effects: Vec<(u32, Effect)> = Vec::new();
        let owns = |path: &str, line: u32| path == func.path && self.innermost(path, line, func);

        // Local calls, expanded inline.
        if local_depth < MAX_LOCAL_DEPTH {
            if let Some(file) = self.index.files.iter().find(|f| f.path == func.path) {
                for call in &file.facts.calls {
                    if !owns(func.path, call.line) {
                        continue;
                    }
                    // A call to a declarative HTTP client method (Feign, MicroProfile,
                    // Micronaut) is the remote call, made here.
                    let client = self.api.client_calls.iter().position(|c| {
                        c.unit == unit
                            && c.caller.as_deref().is_some_and(|k| k.rsplit('.').next() == Some(call.name.as_str()))
                            && self.in_interface(&c.evidence.file_path, c.evidence.start_line)
                    });
                    if let Some(ci) = client {
                        effects.push((call.line, Effect::Remote { call: ci, site: Some(call.line) }));
                        continue;
                    }
                    if let Some(e) = self.store_call(unit, func, call) {
                        let write = matches!(e, Effect::Store { write: true, .. });
                        let same =
                            effects.iter().position(|(l, x)| *l == call.line && matches!(x, Effect::Store { .. }));
                        match same {
                            // `…Collection("x").Set(v)` reads as a lookup and a
                            // write on one line; the write is what happened.
                            Some(i) if write => effects[i] = (call.line, e),
                            Some(_) => {}
                            None => effects.push((call.line, e)),
                        }
                        continue;
                    }
                    // A bus dispatches by message type: `Send[*CreateOrder](…)`
                    // names the command, and the handler is the method that
                    // takes it. The registration site never has to be read.
                    if let Some(t) = self.dispatched_handler(unit, func, call) {
                        effects.push((call.line, Effect::Local(t.clone())));
                        continue;
                    }
                    // A call through a field chain names the type that owns the body.
                    if let Some(t) = self.resolve_method(unit, func, &call.callee) {
                        if !(t.path == func.path && t.start == func.start) {
                            effects.push((call.line, Effect::Local(t.clone())));
                            continue;
                        }
                    }
                    let Some(defs) = self.functions.get(unit).and_then(|m| m.get(call.name.as_str())) else { continue };
                    // Interface declarations have no body: `service.create()` runs the implementation.
                    let bodies: Vec<&Func> = defs.iter().filter(|d| !self.in_interface(d.path, d.start)).collect();
                    let target = bodies
                        .iter()
                        .find(|d| d.path == func.path)
                        .or_else(|| (bodies.len() == 1).then(|| &bodies[0]))
                        .copied();
                    if let Some(t) = target.filter(|t| !(t.path == func.path && t.start == func.start)) {
                        effects.push((call.line, Effect::Local(t.clone())));
                    }
                }
            }
        }
        for (ci, c) in self.api.client_calls.iter().enumerate() {
            if c.unit == unit && owns(&c.evidence.file_path, c.evidence.start_line) {
                effects.push((c.evidence.start_line, Effect::Remote { call: ci, site: None }));
            }
        }
        let mut accessed = false;
        if let Some(data) = self.data {
            for (ei, e) in data.entities.iter().enumerate() {
                for (write, list) in [(false, &e.reads), (true, &e.writes)] {
                    for a in
                        list.iter().filter(|a| a.unit == unit && owns(&a.evidence.file_path, a.evidence.start_line))
                    {
                        accessed = true;
                        effects.push((
                            a.evidence.start_line,
                            Effect::Access { entity: ei, write, evidence: a.evidence.clone() },
                        ));
                    }
                }
            }
        }
        for (ri, r) in self.relationships.iter().enumerate().filter(|(_, r)| r.source == unit) {
            let Some(target) = self.infra.iter().find(|i| i.id == r.target) else { continue };
            let storage = target.category == InfraCategory::Storage;
            if storage && accessed {
                continue;
            }
            if storage && !matches!(r.edge_type, EdgeType::Read | EdgeType::Write) {
                continue;
            }
            for ev in r.evidence.iter().filter(|ev| owns(&ev.file_path, ev.start_line)) {
                effects.push((ev.start_line, Effect::Relation { rel: ri, evidence: ev.clone() }));
            }
        }
        if let Some(file) = self.index.files.iter().find(|f| f.path == func.path) {
            for ev in file.facts.events.iter().filter(|ev| ev.kind == "publish" && owns(func.path, ev.line)) {
                effects.push((ev.line, Effect::Event { event_type: ev.event_type.clone() }));
            }
        }
        // The data model reads the query itself, so it names the table and the
        // operation exactly. Where it already speaks for this function, the
        // weaker reading of the client call would only repeat it, less precisely.
        if accessed {
            effects.retain(|(_, e)| !matches!(e, Effect::Store { .. }));
        }
        effects.sort_by_key(|(line, e)| (*line, matches!(e, Effect::Local(_)) as u8));

        for (_, effect) in effects {
            match effect {
                Effect::Local(t) => self.expand_func(unit, &t, steps, local_depth + 1, w),
                Effect::Remote { call, site } => {
                    let c = &self.api.client_calls[call];
                    let op = c.operation.as_deref().and_then(|id| self.api.operations.iter().find(|o| o.id == id));
                    let Some(target) = c.target_unit.clone().or_else(|| op.map(|o| o.unit.clone())) else { continue };
                    if target == unit {
                        continue;
                    }
                    steps.push(Step {
                        from: unit.to_string(),
                        to: target.clone(),
                        kind: StepKind::Call,
                        label: format!("{} {}", c.method, op.map(|o| o.path.as_str()).unwrap_or(&c.path)),
                        payload: op.and_then(|o| o.request_body.as_ref()).map(|t| t.type_name.clone()),
                        operation: op.map(|o| o.id.clone()),
                        within: Some(func.name.clone()),
                        asynchronous: false,
                        evidence: match site {
                            Some(line) => EvidenceRef {
                                file_path: func.path.to_string(),
                                start_line: line,
                                end_line: line,
                                symbol_name: None,
                                note: c.caller.as_ref().map(|k| format!("calls `{k}`")),
                            },
                            None => c.evidence.clone(),
                        },
                    });
                    if let Some(op) = op {
                        if w.remote_depth < MAX_REMOTE_DEPTH {
                            self.expand_operation(op, steps, w.visiting, w.remote_depth + 1, w.notes, w.functions);
                        } else {
                            w.notes.push(format!("{} not expanded (call depth {MAX_REMOTE_DEPTH})", op.id));
                        }
                        steps.push(self.reply(op, unit));
                    }
                }
                Effect::Access { entity, write, evidence } => {
                    let e = &self.data.unwrap().entities[entity];
                    steps.push(Step {
                        from: unit.to_string(),
                        to: self.datastore_for(unit, Some(e)),
                        kind: if write { StepKind::Write } else { StepKind::Read },
                        label: format!("{} {}", if write { "write" } else { "read" }, e.table),
                        payload: None,
                        operation: None,
                        within: Some(func.name.clone()),
                        asynchronous: false,
                        evidence,
                    });
                }
                Effect::Event { event_type } => {
                    let line = self
                        .index
                        .files
                        .iter()
                        .find(|f| f.path == func.path)
                        .and_then(|f| {
                            f.facts.events.iter().find(|ev| {
                                ev.kind == "publish" && ev.event_type == event_type && owns(func.path, ev.line)
                            })
                        })
                        .map(|ev| ev.line)
                        .unwrap_or(func.start);
                    steps.push(Step {
                        from: unit.to_string(),
                        to: unit.to_string(),
                        kind: StepKind::Publish,
                        label: format!("publishes {event_type}"),
                        payload: None,
                        operation: None,
                        within: Some(func.name.clone()),
                        asynchronous: false,
                        evidence: EvidenceRef {
                            file_path: func.path.to_string(),
                            start_line: line,
                            end_line: line,
                            symbol_name: None,
                            note: Some(format!("publishes `{event_type}`")),
                        },
                    });
                }
                Effect::Store { infra, write, label, evidence } => {
                    steps.push(Step {
                        from: unit.to_string(),
                        to: infra,
                        kind: if write { StepKind::Write } else { StepKind::Read },
                        label,
                        payload: None,
                        operation: None,
                        within: Some(func.name.clone()),
                        asynchronous: false,
                        evidence,
                    });
                }
                Effect::Relation { rel, evidence } => {
                    let r = &self.relationships[rel];
                    let kind = match r.edge_type {
                        EdgeType::Event | EdgeType::Async => StepKind::Publish,
                        EdgeType::Read => StepKind::Read,
                        EdgeType::Write => StepKind::Write,
                        EdgeType::Sync => StepKind::Call,
                    };
                    steps.push(Step {
                        from: unit.to_string(),
                        to: r.target.clone(),
                        kind,
                        label: r.label.clone(),
                        payload: None,
                        operation: None,
                        within: Some(func.name.clone()),
                        asynchronous: false,
                        evidence,
                    });
                }
            }
        }
    }

    /// The receiver of the method whose body starts at `start` in `path`.
    fn receiver_at(&self, path: &str, start: u32) -> Option<Receiver> {
        let f = self.index.files.iter().find(|f| f.path == path)?;
        f.facts.symbols.iter().find(|s| s.start_line == start && s.receiver.is_some())?.receiver.clone()
    }

    /// The handler a message-typed dispatch reaches, when exactly one method
    /// takes that message.
    fn dispatched_handler(&self, unit: &str, from: &Func<'a>, call: &crate::extract::Call) -> Option<&Func<'a>> {
        let by_message = self.handlers.get(unit)?;
        for arg in &call.type_args {
            let Some(candidates) = by_message.get(arg) else { continue };
            let bodies: Vec<&Func> = candidates
                .iter()
                .filter(|f| !self.in_interface(f.path, f.start))
                .filter(|f| !(f.path == from.path && f.start == from.start))
                .collect();
            if let [only] = bodies[..] {
                return Some(only);
            }
        }
        None
    }

    /// Resolve a call written through a field chain to the body that runs.
    ///
    /// Go's CQRS and hexagonal layouts dispatch through struct fields —
    /// `h.app.Queries.AllTrainings.Handle(ctx, …)` — and through ports, which are
    /// interfaces implemented by one adapter. Both defeat resolution by bare name:
    /// `Handle` is declared on every handler in the service. Walk the receiver
    /// expression to a type instead, then take that type's method.
    fn resolve_method(&self, unit: &str, from: &Func<'a>, callee: &str) -> Option<&Func<'a>> {
        let mut segs = callee.split('.').collect::<Vec<_>>();
        let method = segs.pop()?;
        if segs.is_empty() {
            return None;
        }
        let types = self.types.get(unit)?;

        // The chain is rooted in the enclosing method's receiver (`h` in `h.app…`).
        let recv = from.receiver.as_ref()?;
        if recv.var.as_deref() != Some(segs[0]) {
            return None;
        }
        let mut current = recv.type_name.clone();
        for seg in &segs[1..] {
            current = self.field_type(types, &current, seg)?;
        }
        self.method_on(unit, types, &current, method)
    }

    /// The type of `field` on `ty`, following embedded fields.
    ///
    /// A type may embed itself (`type Node struct { *Node }`) or embed in a
    /// cycle, so the search is bounded and never revisits a type.
    fn field_type(&self, types: &BTreeMap<String, TypeDef>, ty: &str, field: &str) -> Option<String> {
        let mut seen = BTreeSet::new();
        field_type_seen(types, ty, field, &mut seen, 0)
    }

    /// The body of `method` on `ty`, resolving a port to its single adapter.
    fn method_on(&self, unit: &str, types: &BTreeMap<String, TypeDef>, ty: &str, method: &str) -> Option<&Func<'a>> {
        let by_type = self.methods.get(unit)?;
        if let Some(fs) = by_type.get(&(ty.to_string(), method.to_string())) {
            if let Some(f) = fs.iter().find(|f| !self.in_interface(f.path, f.start)) {
                return Some(f);
            }
        }
        // Go pairs an exported handler type with the unexported struct that
        // implements it (`AllTrainingsHandler` ↔ `allTrainingsHandler`), often
        // through a decorator, so the exported name carries no body of its own.
        let twin = by_type
            .iter()
            .filter(|((t, m), _)| m == method && t != ty && t.eq_ignore_ascii_case(ty))
            .map(|(_, fs)| fs)
            .collect::<Vec<_>>();
        if let [fs] = twin[..] {
            if let Some(f) = fs.iter().find(|f| !self.in_interface(f.path, f.start)) {
                return Some(f);
            }
        }
        // A port: usable only when exactly one type in the unit implements it.
        let def = types.get(ty)?;
        if !def.is_interface || def.methods.is_empty() {
            return None;
        }
        let mut impls = types.values().filter(|c| {
            !c.is_interface && def.methods.iter().all(|m| by_type.contains_key(&(c.name.clone(), m.clone())))
        });
        let only = impls.next()?;
        if impls.next().is_some() {
            return None;
        }
        by_type.get(&(only.name.clone(), method.to_string()))?.iter().find(|f| !self.in_interface(f.path, f.start))
    }

    /// A call whose receiver chain roots in a field holding an infrastructure
    /// client, cited at the line that makes it rather than at the dependency
    /// that declares it.
    fn store_call(&self, unit: &str, from: &Func<'a>, call: &crate::extract::Call) -> Option<Effect<'a>> {
        let fields = self.infra_fields.get(unit)?;
        let recv = from.receiver.as_ref()?;
        let mut segs = call.callee.split('.');
        if segs.next()? != recv.var.as_deref()? {
            return None;
        }
        let field = segs.next()?;
        let infra = fields.get(&(recv.type_name.clone(), field.to_string()))?;
        let method = call.callee.rsplit('.').next()?;
        // The collection or bucket named on the same line says what is touched.
        let target = self
            .index
            .files
            .iter()
            .find(|f| f.path == from.path)
            .and_then(|f| f.facts.strings.iter().find(|s| s.line == call.line))
            .map(|s| s.value.clone());
        let write = is_store_write(method);
        // `Collection("trainings")` only names a handle; whether it is read or
        // written is decided by the caller, so the step states what it reaches
        // rather than claiming an operation the line does not show.
        let label = match (is_store_handle(method), &target) {
            (true, Some(t)) => t.clone(),
            (true, None) => method.to_string(),
            (false, t) => {
                format!("{} {}", if write { "write" } else { "read" }, t.clone().unwrap_or_else(|| method.to_string()))
            }
        };
        Some(Effect::Store {
            infra: infra.clone(),
            write,
            label,
            evidence: EvidenceRef {
                file_path: from.path.to_string(),
                start_line: call.line,
                end_line: call.line,
                // The call line is the evidence; naming the enclosing function
                // here would claim the symbol sits on this line, which it need
                // not. The step already records what it is `within`.
                symbol_name: None,
                note: None,
            },
        })
    }

    fn in_interface(&self, path: &str, line: u32) -> bool {
        self.interfaces.get(path).is_some_and(|spans| spans.iter().any(|&(s, e)| s <= line && line <= e))
    }

    /// True when `line` belongs to `func` itself, not to a function nested inside it.
    fn innermost(&self, path: &str, line: u32, func: &Func) -> bool {
        if line < func.start || line > func.end {
            return false;
        }
        !self.spans.get(path).is_some_and(|spans| {
            spans.iter().any(|&(s, e, _)| {
                s >= func.start && e <= func.end && (s, e) != (func.start, func.end) && s <= line && line <= e
            })
        })
    }

    /// The store a write lands in, preferring the one the entity was declared
    /// against.
    ///
    /// Picking the unit's first storage in enum order drew every write to the
    /// same place: a service with JPA on Postgres and a `@Document` saved
    /// through a Mongo repository had its Mongo writes drawn to Postgres, with
    /// the real Mongo call site cited beside them.
    fn datastore_for(&self, unit: &str, entity: Option<&crate::data::Entity>) -> String {
        const DATABASES: &[&str] =
            &["postgres", "mysql", "sqlite", "mongodb", "sqlserver", "mariadb", "cockroachdb", "dynamodb", "firestore"];
        let used: Vec<&InfraSummary> = self
            .infra
            .iter()
            .filter(|i| i.category == InfraCategory::Storage && i.used_by.iter().any(|u| u == unit))
            .collect();
        if used.len() > 1 {
            if let Some(source) = entity.map(|e| e.source.to_lowercase()) {
                let wanted =
                    ["mongo", "dynamo", "firestore", "elasticsearch", "redis"].into_iter().find(|k| source.contains(k));
                let matches = |i: &InfraSummary| match wanted {
                    Some(k) => i.id.contains(k),
                    // Every other mapper declares a table in a SQL store.
                    None => i.kind.is_sql(),
                };
                if let Some(i) = used.iter().find(|i| matches(i)) {
                    return i.id.clone();
                }
            }
        }
        used.iter()
            .find(|i| DATABASES.iter().any(|d| i.id.contains(d)))
            .or(used.first())
            .map(|i| i.id.clone())
            .unwrap_or_else(|| "database".into())
    }
}

struct Walk<'w> {
    visiting: &'w mut BTreeSet<String>,
    remote_depth: usize,
    seen: &'w mut BTreeSet<(String, u32)>,
    notes: &'w mut Vec<String>,
    functions: &'w mut Vec<String>,
}

/// The type of `field` on `ty`, following embedded fields without revisiting one.
fn field_type_seen(
    types: &BTreeMap<String, TypeDef>,
    ty: &str,
    field: &str,
    seen: &mut BTreeSet<String>,
    depth: usize,
) -> Option<String> {
    if depth > MAX_EMBED_DEPTH || !seen.insert(ty.to_string()) {
        return None;
    }
    let def = types.get(ty)?;
    if let Some(f) = def.fields.iter().find(|f| f.name == field) {
        return Some(f.type_name.clone());
    }
    // An embedded field promotes its own fields onto the outer type.
    def.fields
        .iter()
        .filter(|f| f.name.is_empty())
        .find_map(|f| field_type_seen(types, &f.type_name, field, seen, depth + 1))
}

/// Types that say nothing about which message a handler takes.
const GENERIC_PARAM_TYPES: &[&str] =
    &["Context", "context", "error", "string", "int", "int64", "bool", "T", "R", "any", "interface"];

/// Embedded types nest a few levels in practice; beyond that a chain is a cycle.
const MAX_EMBED_DEPTH: usize = 8;

/// Store SDKs name their mutations consistently enough to tell a write from a read.
fn is_store_write(method: &str) -> bool {
    const WRITES: &[&str] = &[
        "set",
        "put",
        "add",
        "create",
        "insert",
        "update",
        "delete",
        "remove",
        "write",
        "save",
        "commit",
        "upsert",
        "apply",
        "push",
        "send",
        "store",
        "runtransaction",
        "batch",
        "bulk",
        "flush",
    ];
    let m = method.to_ascii_lowercase();
    WRITES.iter().any(|w| m == *w || m.starts_with(w))
}

/// Methods that return a handle rather than perform an operation.
fn is_store_handle(method: &str) -> bool {
    const HANDLES: &[&str] =
        &["collection", "doc", "document", "ref", "database", "bucket", "table", "index", "key", "topic", "queue"];
    let m = method.to_ascii_lowercase();
    HANDLES.contains(&m.as_str())
}

fn entry_key(e: &crate::entry::Entry) -> String {
    format!("{}:{}:{}", e.unit, e.handler.path, e.handler.start)
}

/// Merges consecutive identical steps (a loop of inserts reads as one write).
fn coalesce(steps: &mut Vec<Step>) {
    steps.dedup_by(|b, a| a.from == b.from && a.to == b.to && a.kind == b.kind && a.label == b.label);
}
