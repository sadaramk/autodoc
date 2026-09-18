//! Entry points that aren't HTTP operations: scheduled jobs, message and task
//! consumers, in-process event listeners and startup hooks. Each resolves to
//! the function that runs, so the tracer can walk it exactly like a request
//! handler.
//!
//! Message consumers come primarily from the scan's broker → unit "delivers"
//! relationships (consumer calls and listener annotations in every language);
//! the detectors here add what those don't carry: schedules, startup hooks,
//! in-process events, task queues and precise topics from annotations.

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::InfraCategory;
use crate::extract::{Annotation, FileFacts, SymbolKind};
use crate::lang::Language;
use crate::scan::{EvidenceRef, InfraSummary, Relationship};
use crate::source::{SourceFile, SourceIndex};
use crate::trace::TriggerKind;

/// A function started by something other than an HTTP request.
#[derive(Debug, Clone)]
pub(crate) struct Entry<'a> {
    pub kind: TriggerKind,
    pub unit: &'a str,
    /// Human wording of the trigger: `cron 0 * * * *`, `Kafka topic order.placed`, `OrderCompleted`.
    pub label: String,
    /// Infrastructure id delivering it (message entries), when known.
    pub broker: Option<String>,
    /// Topic, queue, channel or in-process event type it consumes.
    pub topic: Option<String>,
    pub handler: Handler<'a>,
    /// Where the trigger is declared (annotation, registration call, consumer call).
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone)]
pub(crate) struct Handler<'a> {
    pub path: &'a str,
    pub name: String,
    pub start: u32,
    pub end: u32,
}

/// Event types that mean "the application has started".
const STARTUP_EVENTS: &[&str] = &[
    "ApplicationReadyEvent",
    "ApplicationStartedEvent",
    "ContextRefreshedEvent",
    "StartupEvent",
    "ServerStartupEvent",
];

pub(crate) fn discover<'a>(
    index: &'a SourceIndex<'a>,
    infra: &[InfraSummary],
    relationships: &[Relationship],
) -> Vec<Entry<'a>> {
    let mut out: Vec<Entry<'a>> = Vec::new();
    let bus_of = |unit: &str| -> Option<String> {
        infra
            .iter()
            .filter(|i| i.category == InfraCategory::EventBus && i.used_by.iter().any(|u| u == unit))
            .map(|i| i.id.clone())
            .next()
    };
    let used = |unit: &str, id: &str| infra.iter().any(|i| i.id == id && i.used_by.iter().any(|u| u == unit));
    let mut texts: BTreeMap<&str, Option<String>> = BTreeMap::new();
    // `@Scheduled(cron = "${backup.cron}")` reads from the unit's configuration,
    // which for Spring Cloud Config lives in the config server's shared files.
    let mut properties: BTreeMap<&str, crate::jvm::Properties> = BTreeMap::new();
    if index.files.iter().any(|f| f.language.is_jvm()) {
        let docs = crate::jvm::config_documents(&index.root);
        for u in index.units {
            properties.insert(u.id.as_str(), crate::jvm::properties_for(&u.root, &u.aliases, &docs));
        }
    }
    let no_properties = crate::jvm::Properties::default();

    for f in &index.files {
        match f.language {
            // Kotlin services declare the same Spring annotations.
            Language::Java | Language::Kotlin => {
                let props = properties.get(f.unit).unwrap_or(&no_properties);
                java(f, &bus_of, &used, props, &mut out)
            }
            Language::TypeScript | Language::JavaScript => {
                let candidate =
                    f.facts.calls.iter().any(|c| matches!(c.name.as_str(), "schedule" | "scheduleJob" | "setInterval"))
                        || f.facts.imports.iter().any(|i| {
                            let s = i.specifier.as_str();
                            s == "bullmq"
                                || s.starts_with("@nestjs/schedule")
                                || s.starts_with("@nestjs/microservices")
                                || s.starts_with("@nestjs/event-emitter")
                        });
                if candidate {
                    if let Some(text) = text_of(index, f.path, &mut texts) {
                        typescript(index, f, &text, &bus_of, &used, &mut out);
                    }
                }
            }
            Language::Python => {
                let candidate = f.facts.calls.iter().any(|c| {
                    matches!(
                        c.name.as_str(),
                        "add_job" | "do" | "task" | "shared_task" | "scheduled_job" | "on_event" | "receiver"
                    )
                }) || f.facts.imports.iter().any(|i| {
                    let s = i.specifier.as_str();
                    s.starts_with("celery")
                        || s.starts_with("apscheduler")
                        || s == "schedule"
                        || s.starts_with("django.dispatch")
                        || s.starts_with("kafka")
                });
                if candidate {
                    if let Some(text) = text_of(index, f.path, &mut texts) {
                        python(index, f, &text, &bus_of, &used, &mut out);
                    }
                }
            }
            Language::Go => {
                if f.facts.calls.iter().any(|c| matches!(c.name.as_str(), "AddFunc" | "AddJob" | "NewTicker" | "Tick"))
                    || f.facts.symbols.iter().any(|s| s.name == "ConsumeClaim")
                {
                    if let Some(text) = text_of(index, f.path, &mut texts) {
                        go(index, f, &text, &bus_of, &mut out);
                    }
                }
            }
            Language::Rust => {
                if f.facts
                    .calls
                    .iter()
                    .any(|c| c.name == "interval" || c.callee.contains("Job::new") || c.name == "recv")
                {
                    if let Some(text) = text_of(index, f.path, &mut texts) {
                        rust(f, &text, &bus_of, &mut out);
                    }
                }
            }
            _ => {}
        }
    }

    // Consumers the scan found (poll loops, `consume(...)`, `Subscribe(...)`, listener annotations).
    for r in
        relationships.iter().filter(|r| r.edge_type == autodoc_ir::EdgeType::Event && r.label.starts_with("delivers"))
    {
        if !infra.iter().any(|i| i.id == r.source && i.category == InfraCategory::EventBus) {
            continue;
        }
        let topic = r.label.strip_prefix("delivers ").filter(|t| *t != "events").map(str::to_string);
        for ev in &r.evidence {
            let Some(f) = index.files.iter().find(|f| f.path == ev.file_path && f.unit == r.target) else { continue };
            let line = ev.start_line;
            let covered = out.iter().any(|e| {
                e.unit == f.unit && e.handler.path == f.path && e.handler.start <= line && line <= e.handler.end
            });
            if covered {
                continue;
            }
            let Some(handler) = enclosing(f, line).or_else(|| {
                ev.symbol_name.clone().map(|name| Handler {
                    path: f.path,
                    name,
                    start: ev.start_line,
                    end: ev.end_line,
                })
            }) else {
                continue;
            };
            let label = match &topic {
                Some(t) => format!("{} topic {t}", broker_label(infra, &r.source)),
                None => format!("{} messages", broker_label(infra, &r.source)),
            };
            out.push(Entry {
                kind: TriggerKind::Message,
                unit: f.unit,
                label,
                broker: Some(r.source.clone()),
                topic: topic.clone(),
                handler,
                evidence: ev.clone(),
            });
        }
    }

    // One entry per handler function.
    let mut seen = BTreeSet::new();
    out.retain(|e| seen.insert((e.unit, e.handler.path, e.handler.start, e.kind)));
    out
}

fn broker_label(infra: &[InfraSummary], id: &str) -> String {
    infra.iter().find(|i| i.id == id).map(|i| i.label.clone()).unwrap_or_else(|| id.to_string())
}

fn text_of<'a>(index: &SourceIndex, path: &'a str, cache: &mut BTreeMap<&'a str, Option<String>>) -> Option<String> {
    cache.entry(path).or_insert_with(|| index.read(path)).clone()
}

/// Innermost function or method containing `line`.
pub(crate) fn enclosing<'a>(f: &SourceFile<'a>, line: u32) -> Option<Handler<'a>> {
    enclosing_in(f.facts, line).map(|(name, start, end)| Handler { path: f.path, name, start, end })
}

fn enclosing_in(facts: &FileFacts, line: u32) -> Option<(String, u32, u32)> {
    facts
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .filter(|s| s.start_line <= line && line <= s.end_line)
        .min_by_key(|s| s.end_line - s.start_line)
        .map(|s| (s.name.clone(), s.start_line, s.end_line))
}

fn function_named<'a>(f: &SourceFile<'a>, name: &str) -> Option<Handler<'a>> {
    f.facts
        .symbols
        .iter()
        .find(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method) && s.name == name)
        .map(|s| Handler { path: f.path, name: s.name.clone(), start: s.start_line, end: s.end_line })
}

fn line_ev(f: &SourceFile, start: u32, end: u32, note: String) -> EvidenceRef {
    EvidenceRef {
        file_path: f.path.to_string(),
        start_line: start,
        end_line: end.max(start),
        symbol_name: None,
        note: Some(note),
    }
}

fn line_text(text: &str, line: u32) -> &str {
    text.lines().nth(line.saturating_sub(1) as usize).unwrap_or("")
}

fn first_string(f: &SourceFile, line: u32) -> Option<String> {
    f.facts.strings.iter().find(|s| s.line == line).map(|s| s.value.clone())
}

/// Lines spanned by the call starting at `needle` on `line`, by bracket matching.
fn call_span(text: &str, line: u32, needle: &str) -> Option<(u32, u32)> {
    let lines: Vec<&str> = text.lines().collect();
    let start_idx = line.saturating_sub(1) as usize;
    let col = lines.get(start_idx)?.find(needle)?;
    let mut depth = 0i32;
    let mut opened = false;
    for (i, l) in lines.iter().enumerate().skip(start_idx) {
        let from = if i == start_idx { col } else { 0 };
        for ch in l[from..].chars() {
            match ch {
                '(' => {
                    depth += 1;
                    opened = true;
                }
                ')' => {
                    depth -= 1;
                    if opened && depth == 0 {
                        return Some((line, i as u32 + 1));
                    }
                }
                _ => {}
            }
        }
        if i > start_idx + 400 {
            break;
        }
    }
    None
}

fn identifiers(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| w.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_'))
}

/// A function of the same unit by name: this file first, else the only definition elsewhere.
fn unit_function<'a>(index: &'a SourceIndex<'a>, f: &SourceFile<'a>, name: &str) -> Option<Handler<'a>> {
    if let Some(h) = function_named(f, name) {
        return Some(h);
    }
    let mut found =
        index.files.iter().filter(|o| o.unit == f.unit && o.path != f.path).filter_map(|o| function_named(o, name));
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// A named function passed as an argument on `line`, else the inline callback's span.
fn callback<'a>(
    index: &'a SourceIndex<'a>,
    f: &SourceFile<'a>,
    text: &str,
    line: u32,
    needle: &str,
    fallback_name: &str,
) -> Option<Handler<'a>> {
    let l = line_text(text, line);
    let after = l.find(needle).map(|i| &l[i + needle.len()..]).unwrap_or(l);
    for id in identifiers(after) {
        if let Some(h) = unit_function(index, f, id) {
            return Some(h);
        }
    }
    let (s, e) = call_span(text, line, needle)?;
    Some(Handler { path: f.path, name: fallback_name.to_string(), start: s, end: e })
}

// ───────────────────────────── Java ─────────────────────────────

fn java<'a>(
    f: &SourceFile<'a>,
    bus_of: &dyn Fn(&str) -> Option<String>,
    used: &dyn Fn(&str, &str) -> bool,
    properties: &crate::jvm::Properties,
    out: &mut Vec<Entry<'a>>,
) {
    let method = |a: &Annotation| -> Handler<'a> {
        f.facts
            .symbols
            .iter()
            .find(|s| {
                s.kind == SymbolKind::Method
                    && s.name == a.target
                    && s.start_line <= a.target_start.max(a.line) + 3
                    && s.end_line >= a.target_start
            })
            .map(|s| Handler { path: f.path, name: s.name.clone(), start: s.start_line, end: s.end_line })
            .unwrap_or(Handler { path: f.path, name: a.target.clone(), start: a.target_start, end: a.target_end })
    };
    let arg = |a: &Annotation, names: &[&str]| crate::jvm::arg(a, names);
    for a in &f.facts.annotations {
        let handler_target = a.target_kind == "method";
        let (kind, label, broker, topic) = match a.name.as_str() {
            "Scheduled" if handler_target => {
                // `${backup.cron}` says nothing; the value the configuration gives it does.
                let spelled = |raw: String| match properties.resolve(&raw) {
                    Some(r) if r.value != raw => format!("{} ({})", r.value, r.key),
                    _ => raw,
                };
                let label = if let Some(c) = arg(a, &["cron"]) {
                    format!("cron {}", spelled(c))
                } else if let Some(r) = arg(a, &["fixedRateString", "fixedDelayString", "every", "initialDelayString"])
                {
                    format!("every {}", spelled(r))
                } else if a.arguments.contains("fixedRate") || a.arguments.contains("fixedDelay") {
                    let v =
                        a.arguments.split('=').nth(1).unwrap_or("").split(',').next().unwrap_or("").trim().to_string();
                    format!("every {v} ms")
                } else {
                    "schedule".to_string()
                };
                (TriggerKind::Schedule, label, None, None)
            }
            "KafkaListener" | "Topic" if handler_target => {
                let topic = arg(a, &["topics", "topicPattern", "value", ""]).or_else(|| {
                    f.facts
                        .annotations
                        .iter()
                        .find(|o| o.name == "Topic" && o.target == a.target && o.owner == a.owner)
                        .and_then(|o| arg(o, &["value", ""]))
                });
                if a.name == "Topic"
                    && f.facts
                        .annotations
                        .iter()
                        .any(|o| o.name == "KafkaListener" && o.target == a.target && o.target_kind == "method")
                {
                    continue;
                }
                let broker = used(f.unit, "kafka").then(|| "kafka".to_string()).or_else(|| bus_of(f.unit));
                (
                    TriggerKind::Message,
                    topic.as_ref().map(|t| format!("Kafka topic {t}")).unwrap_or("Kafka messages".into()),
                    broker,
                    topic,
                )
            }
            "RabbitListener" if handler_target => {
                let q = arg(a, &["queues", "queuesToDeclare", "value", ""]);
                let broker = used(f.unit, "rabbitmq").then(|| "rabbitmq".to_string()).or_else(|| bus_of(f.unit));
                (
                    TriggerKind::Message,
                    q.as_ref().map(|t| format!("RabbitMQ queue {t}")).unwrap_or("RabbitMQ messages".into()),
                    broker,
                    q,
                )
            }
            "JmsListener" if handler_target => {
                let d = arg(a, &["destination", "value"]);
                (
                    TriggerKind::Message,
                    d.as_ref().map(|t| format!("JMS destination {t}")).unwrap_or("JMS messages".into()),
                    bus_of(f.unit),
                    d,
                )
            }
            "SqsListener" if handler_target => {
                let q = arg(a, &["value", "queueNames", ""]);
                let broker = used(f.unit, "sqs").then(|| "sqs".to_string()).or_else(|| bus_of(f.unit));
                (
                    TriggerKind::Message,
                    q.as_ref().map(|t| format!("SQS queue {t}")).unwrap_or("SQS messages".into()),
                    broker,
                    q,
                )
            }
            "Incoming" | "StreamListener" if handler_target => {
                let c = arg(a, &["value", "target", ""]);
                (
                    TriggerKind::Message,
                    c.as_ref().map(|t| format!("channel {t}")).unwrap_or("stream messages".into()),
                    bus_of(f.unit),
                    c,
                )
            }
            "Observes" | "ObservesAsync" if a.target_kind == "parameter" => {
                // `void onStart(@Observes StartupEvent ev)`: the type follows the annotation.
                let Some(owner) = a.owner.as_deref() else { continue };
                let Some(m) = f.facts.symbols.iter().find(|s| {
                    s.kind == SymbolKind::Method && s.name == owner && s.start_line <= a.line && a.line <= s.end_line
                }) else {
                    continue;
                };
                let ty = f
                    .facts
                    .events
                    .iter()
                    .find(|e| e.kind == "listen" && e.method == owner)
                    .map(|e| e.event_type.clone())
                    .unwrap_or_else(|| a.target.clone());
                let handler = Handler { path: f.path, name: m.name.clone(), start: m.start_line, end: m.end_line };
                let kind = if STARTUP_EVENTS.iter().any(|s| ty.contains(s)) {
                    TriggerKind::Startup
                } else {
                    TriggerKind::Event
                };
                out.push(Entry {
                    kind,
                    unit: f.unit,
                    label: if kind == TriggerKind::Startup { "application start".into() } else { ty.clone() },
                    broker: None,
                    topic: (kind == TriggerKind::Event).then_some(ty),
                    handler,
                    evidence: line_ev(f, a.line, a.line, format!("`@{}`", a.name)),
                });
                continue;
            }
            _ => continue,
        };
        out.push(Entry {
            kind,
            unit: f.unit,
            label,
            broker,
            topic,
            handler: method(a),
            evidence: line_ev(f, a.line, a.target_end, format!("`@{}` {}", a.name, a.target)),
        });
    }
    // In-process listeners: `@EventListener` / `@TransactionalEventListener` / `@ApplicationModuleListener`.
    for ev in f.facts.events.iter().filter(|e| e.kind == "listen") {
        let Some(m) = f.facts.symbols.iter().find(|s| {
            s.kind == SymbolKind::Method && s.name == ev.method && s.start_line <= ev.line + 3 && ev.line <= s.end_line
        }) else {
            continue;
        };
        let startup = STARTUP_EVENTS.contains(&ev.event_type.as_str());
        let ann = f
            .facts
            .annotations
            .iter()
            .filter(|a| a.target == ev.method && a.target_kind == "method" && a.name.ends_with("Listener"))
            // Overloads share a name: the annotation on this declaration.
            .min_by_key(|a| (a.target_start != m.start_line && a.line != ev.line, a.line.abs_diff(ev.line)))
            .map(|a| (a.line, a.name.clone()))
            .unwrap_or((ev.line, "EventListener".into()));
        out.push(Entry {
            kind: if startup { TriggerKind::Startup } else { TriggerKind::Event },
            unit: f.unit,
            label: if startup { "application start".into() } else { ev.event_type.clone() },
            broker: None,
            topic: (!startup).then(|| ev.event_type.clone()),
            handler: Handler { path: f.path, name: m.name.clone(), start: m.start_line, end: m.end_line },
            evidence: line_ev(f, ann.0, ann.0, format!("`@{}` {}", ann.1, ev.method)),
        });
    }
    // An application whose `main` does work after starting (`run(...).getBean(X).start()`).
    for ep in f.facts.entry_points.iter().filter(|e| e.reason.contains("application object")) {
        if let Some(m) = f.facts.symbols.iter().find(|s| {
            s.kind == SymbolKind::Method
                && s.name == "main"
                && ep.start_line <= s.start_line
                && s.end_line <= ep.end_line
        }) {
            out.push(Entry {
                kind: TriggerKind::Startup,
                unit: f.unit,
                label: "application start (main)".into(),
                broker: None,
                topic: None,
                handler: Handler { path: f.path, name: m.name.clone(), start: m.start_line, end: m.end_line },
                evidence: line_ev(f, m.start_line, m.start_line, "`main`".into()),
            });
        }
    }
    // Startup runners and Quartz jobs are declared by the type they implement.
    let runner = f
        .facts
        .imports
        .iter()
        .any(|i| i.specifier.ends_with("CommandLineRunner") || i.specifier.ends_with("ApplicationRunner"));
    let quartz =
        f.facts.imports.iter().any(|i| i.specifier.starts_with("org.quartz") || i.specifier.ends_with("QuartzJobBean"));
    if runner || quartz {
        for s in f.facts.symbols.iter().filter(|s| s.kind == SymbolKind::Method) {
            let (kind, label) = match s.name.as_str() {
                "run" if runner => (TriggerKind::Startup, "application start (runner)".to_string()),
                "execute" | "executeInternal" if quartz => (TriggerKind::Schedule, "Quartz job".to_string()),
                _ => continue,
            };
            out.push(Entry {
                kind,
                unit: f.unit,
                label,
                broker: None,
                topic: None,
                handler: Handler { path: f.path, name: s.name.clone(), start: s.start_line, end: s.end_line },
                evidence: line_ev(f, s.start_line, s.start_line, format!("`{}`", s.name)),
            });
        }
    }
}

// ─────────────────────────── TypeScript / JS ───────────────────────────

fn typescript<'a>(
    index: &'a SourceIndex<'a>,
    f: &SourceFile<'a>,
    text: &str,
    bus_of: &dyn Fn(&str) -> Option<String>,
    used: &dyn Fn(&str, &str) -> bool,
    out: &mut Vec<Entry<'a>>,
) {
    for c in &f.facts.calls {
        let receiver = c.callee.to_lowercase();
        let (label, needle) = match c.name.as_str() {
            "schedule" if receiver.contains("cron") => {
                (format!("cron {}", first_string(f, c.line).unwrap_or_default()), "schedule")
            }
            "scheduleJob" => (format!("schedule {}", first_string(f, c.line).unwrap_or_default()), "scheduleJob"),
            "setInterval" if enclosing(f, c.line).is_none() => {
                let l = line_text(text, c.line);
                let ms = l.rsplit(',').next().unwrap_or("").trim().trim_end_matches([')', ';']).trim().to_string();
                (format!("every {ms} ms"), "setInterval")
            }
            _ => continue,
        };
        let Some(handler) = callback(index, f, text, c.line, needle, needle) else { continue };
        out.push(Entry {
            kind: TriggerKind::Schedule,
            unit: f.unit,
            label: label.trim().to_string(),
            broker: None,
            topic: None,
            handler,
            evidence: line_ev(f, c.line, c.line, format!("`{}()`", c.callee)),
        });
    }
    for (i, l) in text.lines().enumerate() {
        let line = i as u32 + 1;
        let t = l.trim_start();
        // BullMQ workers: `new Worker("emails", async (job) => { … })`.
        if let Some(pos) = t.find("new Worker(") {
            let q = t[pos..].split(['"', '\'', '`']).nth(1).unwrap_or("").to_string();
            if let Some(handler) = callback(index, f, text, line, "new Worker(", "worker") {
                let broker = used(f.unit, "redis").then(|| "redis".to_string());
                out.push(Entry {
                    kind: TriggerKind::Message,
                    unit: f.unit,
                    label: format!("BullMQ queue {q}"),
                    broker,
                    topic: Some(q),
                    handler,
                    evidence: line_ev(f, line, line, "`new Worker()`".into()),
                });
            }
            continue;
        }
        // NestJS decorators on the next method.
        let decorator = [
            ("@Cron(", TriggerKind::Schedule),
            ("@Interval(", TriggerKind::Schedule),
            ("@EventPattern(", TriggerKind::Message),
            ("@MessagePattern(", TriggerKind::Message),
            ("@OnEvent(", TriggerKind::Event),
            ("@Process(", TriggerKind::Message),
        ]
        .into_iter()
        .find(|(d, _)| t.starts_with(d));
        if let Some((d, kind)) = decorator {
            let arg = t[d.len()..].split(['"', '\'', '`']).nth(1).map(str::to_string);
            let Some(m) = f
                .facts
                .symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Method && s.end_line > line && s.start_line <= line + 6)
                .min_by_key(|s| s.start_line)
            else {
                continue;
            };
            let label = match kind {
                TriggerKind::Schedule => {
                    format!("{} {}", d.trim_start_matches('@').trim_end_matches('('), arg.clone().unwrap_or_default())
                }
                TriggerKind::Message => format!("pattern {}", arg.clone().unwrap_or_default()),
                _ => arg.clone().unwrap_or_else(|| "event".into()),
            };
            out.push(Entry {
                kind,
                unit: f.unit,
                label: label.trim().to_string(),
                broker: (kind == TriggerKind::Message).then(|| bus_of(f.unit)).flatten(),
                topic: (kind != TriggerKind::Schedule).then_some(arg).flatten(),
                handler: Handler { path: f.path, name: m.name.clone(), start: m.start_line, end: m.end_line },
                evidence: line_ev(f, line, line, format!("`{}`", t.trim_end())),
            });
        }
    }
}

// ───────────────────────────── Python ─────────────────────────────

fn python<'a>(
    index: &'a SourceIndex<'a>,
    f: &SourceFile<'a>,
    text: &str,
    bus_of: &dyn Fn(&str) -> Option<String>,
    used: &dyn Fn(&str, &str) -> bool,
    out: &mut Vec<Entry<'a>>,
) {
    let lines: Vec<&str> = text.lines().collect();
    let function_at = |line: u32| -> Option<Handler<'a>> {
        // Decorated definitions start at their first decorator.
        f.facts
            .symbols
            .iter()
            .filter(|s| {
                matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
                    && s.start_line <= line
                    && line <= s.end_line
            })
            .min_by_key(|s| s.end_line - s.start_line)
            .map(|s| Handler { path: f.path, name: s.name.clone(), start: s.start_line, end: s.end_line })
    };
    let beat_tasks: BTreeSet<String> = if text.contains("beat_schedule") || text.contains("CELERYBEAT_SCHEDULE") {
        f.facts.strings.iter().filter_map(|s| s.value.rsplit('.').next().map(str::to_string)).collect()
    } else {
        BTreeSet::new()
    };
    for (i, l) in lines.iter().enumerate() {
        let line = i as u32 + 1;
        let t = l.trim_start();
        if t.starts_with('@') {
            let deco = t.trim_start_matches('@');
            let arg = deco.split(['"', '\'']).nth(1).map(str::to_string);
            let (kind, label, broker, topic) =
                if deco.starts_with("shared_task") || deco.split('(').next().is_some_and(|h| h.ends_with(".task")) {
                    let broker = ["rabbitmq", "redis"].iter().find(|b| used(f.unit, b)).map(|b| b.to_string());
                    (TriggerKind::Message, "Celery task".to_string(), broker, None)
                } else if deco.contains("scheduled_job(") {
                    (TriggerKind::Schedule, format!("scheduled job {}", arg.clone().unwrap_or_default()), None, None)
                } else if deco.contains("on_event(") && arg.as_deref() == Some("startup") {
                    (TriggerKind::Startup, "application start".to_string(), None, None)
                } else if deco.starts_with("receiver(") {
                    let signal =
                        deco.trim_start_matches("receiver(").split([',', ')']).next().unwrap_or("").trim().to_string();
                    (TriggerKind::Event, signal.clone(), None, Some(signal))
                } else {
                    continue;
                };
            // The decorated function is the next definition.
            let Some(h) = lines
                .iter()
                .enumerate()
                .skip(i + 1)
                .take(6)
                .find(|(_, x)| x.trim_start().starts_with("def ") || x.trim_start().starts_with("async def "))
                .and_then(|(j, _)| function_at(j as u32 + 1))
            else {
                continue;
            };
            let kind =
                if kind == TriggerKind::Message && beat_tasks.contains(&h.name) { TriggerKind::Schedule } else { kind };
            let label =
                if kind == TriggerKind::Schedule && label == "Celery task" { "Celery beat".to_string() } else { label };
            out.push(Entry {
                kind,
                unit: f.unit,
                label,
                broker,
                topic,
                handler: h,
                evidence: line_ev(f, line, line, format!("`{}`", t.trim_end())),
            });
            continue;
        }
        // `for message in consumer:` (kafka-python).
        if (t.starts_with("for ") || t.starts_with("async for ")) && t.trim_end().ends_with("consumer:") {
            if let Some(h) = function_at(line) {
                let broker = used(f.unit, "kafka").then(|| "kafka".to_string()).or_else(|| bus_of(f.unit));
                out.push(Entry {
                    kind: TriggerKind::Message,
                    unit: f.unit,
                    label: "consumer loop".into(),
                    broker,
                    topic: None,
                    handler: h,
                    evidence: line_ev(f, line, line, "consumer loop".into()),
                });
            }
        }
    }
    for c in &f.facts.calls {
        let (label, needle) = match c.name.as_str() {
            "add_job" => (format!("{} job", first_string(f, c.line).unwrap_or_else(|| "scheduled".into())), "add_job("),
            "do" if c.callee.contains("every") => (c.callee.trim_end_matches(".do").replace("schedule.", ""), ".do("),
            _ => continue,
        };
        let l = line_text(text, c.line);
        let after = l.find(needle).map(|p| &l[p + needle.len()..]).unwrap_or("");
        let Some(h) = identifiers(after).find_map(|id| unit_function(index, f, id)) else { continue };
        out.push(Entry {
            kind: TriggerKind::Schedule,
            unit: f.unit,
            label,
            broker: None,
            topic: None,
            handler: h,
            evidence: line_ev(f, c.line, c.line, format!("`{}()`", c.callee)),
        });
    }
}

// ───────────────────────────── Go ─────────────────────────────

fn go<'a>(
    index: &'a SourceIndex<'a>,
    f: &SourceFile<'a>,
    text: &str,
    bus_of: &dyn Fn(&str) -> Option<String>,
    out: &mut Vec<Entry<'a>>,
) {
    for c in &f.facts.calls {
        match c.name.as_str() {
            "AddFunc" | "AddJob" => {
                let spec = first_string(f, c.line).unwrap_or_default();
                let Some(h) = callback(index, f, text, c.line, c.name.as_str(), "cron job") else { continue };
                out.push(Entry {
                    kind: TriggerKind::Schedule,
                    unit: f.unit,
                    label: format!("cron {spec}"),
                    broker: None,
                    topic: None,
                    handler: h,
                    evidence: line_ev(f, c.line, c.line, format!("`{}()`", c.callee)),
                });
            }
            "NewTicker" | "Tick" if c.callee.starts_with("time.") => {
                let Some(h) = enclosing(f, c.line) else { continue };
                let l = line_text(text, c.line);
                let every = l
                    .split(c.name.as_str())
                    .nth(1)
                    .unwrap_or("")
                    .trim_start_matches('(')
                    .split(')')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                out.push(Entry {
                    kind: TriggerKind::Schedule,
                    unit: f.unit,
                    label: format!("ticker every {every}"),
                    broker: None,
                    topic: None,
                    handler: h,
                    evidence: line_ev(f, c.line, c.line, format!("`{}()`", c.callee)),
                });
            }
            _ => {}
        }
    }
    for s in f.facts.symbols.iter().filter(|s| s.kind == SymbolKind::Method && s.name == "ConsumeClaim") {
        out.push(Entry {
            kind: TriggerKind::Message,
            unit: f.unit,
            label: "Kafka consumer group".into(),
            broker: bus_of(f.unit),
            topic: None,
            handler: Handler { path: f.path, name: s.name.clone(), start: s.start_line, end: s.end_line },
            evidence: line_ev(f, s.start_line, s.start_line, "`ConsumeClaim`".into()),
        });
    }
}

// ───────────────────────────── Rust ─────────────────────────────

fn rust<'a>(f: &SourceFile<'a>, text: &str, bus_of: &dyn Fn(&str) -> Option<String>, out: &mut Vec<Entry<'a>>) {
    for c in &f.facts.calls {
        let (kind, label) = if c.name == "interval" && c.callee.contains("interval") {
            let l = line_text(text, c.line);
            let every = l.split("interval(").nth(1).unwrap_or("").split(')').next().unwrap_or("").trim().to_string();
            (TriggerKind::Schedule, format!("interval {every})"))
        } else if c.callee.contains("Job::new") {
            (TriggerKind::Schedule, format!("cron {}", first_string(f, c.line).unwrap_or_default()))
        } else if c.name == "recv" && c.callee.to_lowercase().contains("consumer") {
            (TriggerKind::Message, "consumer loop".to_string())
        } else {
            continue;
        };
        let Some(h) = enclosing(f, c.line) else { continue };
        out.push(Entry {
            kind,
            unit: f.unit,
            label: label.replace(")", "").trim().to_string(),
            broker: (kind == TriggerKind::Message).then(|| bus_of(f.unit)).flatten(),
            topic: None,
            handler: h,
            evidence: line_ev(f, c.line, c.line, format!("`{}()`", c.callee)),
        });
    }
}
