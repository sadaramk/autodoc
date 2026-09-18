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
use crate::extract::SymbolKind;
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
}

struct Tracer<'a> {
    index: &'a SourceIndex<'a>,
    api: &'a ApiModel,
    data: Option<&'a DataModel>,
    infra: &'a [InfraSummary],
    relationships: &'a [Relationship],
    /// Function symbols per unit: name → definitions.
    functions: BTreeMap<&'a str, BTreeMap<&'a str, Vec<Func<'a>>>>,
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
    let mut spans: BTreeMap<&str, Vec<(u32, u32, &str)>> = BTreeMap::new();
    let mut interfaces: BTreeMap<&str, Vec<(u32, u32)>> = BTreeMap::new();
    for f in &index.files {
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
            });
            spans.entry(f.path).or_default().push((s.start_line, s.end_line, s.name.as_str()));
        }
    }
    let entries = crate::entry::discover(index, infra, relationships);
    let tracer = Tracer { index, api, data, infra, relationships, functions, spans, interfaces, entries };

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
        let handler =
            Func { path: e.handler.path, name: e.handler.name.clone(), start: e.handler.start, end: e.handler.end };
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
            return Func { path, name: op.handler.name.clone(), start: ev.start_line, end: ev.end_line };
        }
        // A registration line only: find the named function in that file, then in the unit.
        let by_name = self.functions.get(op.unit.as_str()).and_then(|m| m.get(op.handler.name.as_str()));
        by_name
            .and_then(|defs| defs.iter().find(|d| d.path == path).or_else(|| defs.first()))
            .cloned()
            .unwrap_or(Func { path, name: op.handler.name.clone(), start: ev.start_line, end: ev.end_line })
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
                        to: self.datastore_for(unit),
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

    fn datastore_for(&self, unit: &str) -> String {
        const DATABASES: &[&str] =
            &["postgres", "mysql", "sqlite", "mongodb", "sqlserver", "mariadb", "cockroachdb", "dynamodb", "firestore"];
        let used: Vec<&InfraSummary> = self
            .infra
            .iter()
            .filter(|i| i.category == InfraCategory::Storage && i.used_by.iter().any(|u| u == unit))
            .collect();
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

fn entry_key(e: &crate::entry::Entry) -> String {
    format!("{}:{}:{}", e.unit, e.handler.path, e.handler.start)
}

/// Merges consecutive identical steps (a loop of inserts reads as one write).
fn coalesce(steps: &mut Vec<Step>) {
    steps.dedup_by(|b, a| a.from == b.from && a.to == b.to && a.kind == b.kind && a.label == b.label);
}
