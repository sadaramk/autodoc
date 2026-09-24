//! The flows page's behaviour sections: request flows, scheduled jobs and
//! message & event handlers, each drawn as a sequence diagram with a cited
//! step table, plus background requirements for the functional specification.

use std::collections::BTreeMap;

use nunki_analyzer::scan::slug;
use nunki_analyzer::trace::{worth_drawing, Flow, StepKind, TriggerKind, MAX_FLOWS};
use nunki_analyzer::{draft_sequence_ir, DraftOptions};
use nunki_validator::ValidateOptions;

use super::Builder;
use crate::authored::given;
use crate::model::*;

pub(super) fn flow_figure_id(flow: &Flow) -> String {
    format!(
        "flow-{}",
        slug(flow.entry.split_once(':').map(|(u, rest)| format!("{u}-{rest}")).as_deref().unwrap_or(&flow.entry))
    )
}

fn is_request(f: &Flow) -> bool {
    f.trigger.kind == TriggerKind::Http
}

fn is_scheduled(f: &Flow) -> bool {
    matches!(f.trigger.kind, TriggerKind::Schedule | TriggerKind::Startup)
}

fn is_handler(f: &Flow) -> bool {
    matches!(f.trigger.kind, TriggerKind::Message | TriggerKind::Event)
}

fn trigger_badge(kind: TriggerKind) -> Inline {
    match kind {
        TriggerKind::Http => Inline::badge("muted", "request"),
        TriggerKind::Schedule => Inline::badge("declared", "schedule"),
        TriggerKind::Startup => Inline::badge("declared", "startup"),
        TriggerKind::Message => Inline::badge("observed", "message"),
        TriggerKind::Event => Inline::badge("observed", "event"),
    }
}

/// A flows-page section: anchor id, heading, intro, and which flows it holds.
type Section<'s> = (&'s str, &'s str, &'s str, fn(&Flow) -> bool);

impl<'a> Builder<'a> {
    /// Flows that get a figure: the most involved requests and background flows.
    fn drawn_flows(&self) -> Vec<&'a Flow> {
        let flows = &self.report.flows;
        let mut out: Vec<&Flow> = flows.iter().filter(|f| is_request(f) && worth_drawing(f)).take(MAX_FLOWS).collect();
        out.extend(flows.iter().filter(|f| !is_request(f) && worth_drawing(f)).take(MAX_FLOWS));
        out
    }

    pub(super) fn add_flow_figures(
        &mut self,
        curated: &BTreeMap<String, String>,
        validate: &ValidateOptions,
        draft: &DraftOptions,
    ) {
        let report = self.report;
        for flow in self.drawn_flows() {
            self.add_figure(&flow_figure_id(flow), draft_sequence_ir(report, flow, draft), curated, validate);
        }
    }

    /// A participant: its page when it has one, otherwise its label.
    fn party(&self, flow: &Flow, id: &str) -> Inline {
        if self.unit(id).is_some() || self.infra(id).is_some() {
            return self.element(id);
        }
        let label = flow.participants.iter().find(|p| p.id == id).map(|p| p.label.clone()).unwrap_or_else(|| id.into());
        Inline::strong(label)
    }

    pub(super) fn flow_section_blocks(&mut self) -> Vec<Block> {
        let drawn = self.drawn_flows();
        let mut blocks = Vec::new();
        let sections: [Section; 3] = [
            (
                "request-flows",
                "Request flows",
                "What happens, in order, when each of the most involved operations is called: traced from the handler through local calls, calls to other services, reads and writes, and published events, continuing into the handlers that consume those events. Every message links to the line that sends it.",
                is_request,
            ),
            (
                "scheduled-jobs",
                "Scheduled jobs",
                "Work that runs on a schedule or at startup rather than on request: the trigger as declared, then everything the job does.",
                is_scheduled,
            ),
            (
                "message-handlers",
                "Message & event handlers",
                "Handlers started by a message, task or in-process event, traced the same way. Steps after a hand-off to another handler are asynchronous.",
                is_handler,
            ),
        ];
        for (id, title, intro, belongs) in sections {
            let flows: Vec<&Flow> = drawn.iter().copied().filter(|f| belongs(f)).collect();
            let rest: Vec<&Flow> = if id == "request-flows" {
                vec![]
            } else {
                self.report.flows.iter().filter(|f| belongs(f) && !drawn.iter().any(|d| d.id == f.id)).collect()
            };
            if flows.is_empty() && rest.is_empty() {
                continue;
            }
            blocks.push(Block::Heading { level: 2, id: id.into(), text: title.into() });
            blocks.push(Block::Para { inl: vec![Inline::text(intro)] });
            for flow in flows {
                self.flow_blocks(flow, &mut blocks);
            }
            if !rest.is_empty() {
                blocks.push(Block::Heading {
                    level: 3,
                    id: format!("{id}-more"),
                    text: format!("Other {}", title.to_lowercase()),
                });
                let mut rows = Vec::new();
                for f in rest {
                    let effects = f.steps.len().saturating_sub(1);
                    let mut trigger =
                        vec![trigger_badge(f.trigger.kind), Inline::text(" "), Inline::code(f.trigger.label.clone())];
                    if let Some(ev) = &f.trigger.evidence {
                        trigger.push(self.cite(ev));
                    }
                    let handler = f.title.split(" · ").next().unwrap_or(&f.title).to_string();
                    rows.push(vec![
                        vec![self.element(f.entry.split(':').next().unwrap_or(""))],
                        trigger,
                        vec![Inline::code(handler)],
                        vec![Inline::text(if effects == 0 { "none traced".to_string() } else { effects.to_string() })],
                    ]);
                }
                blocks.push(Block::Table {
                    columns: vec!["Service".into(), "Trigger".into(), "Handler".into(), "Effects".into()],
                    rows,
                });
            }
        }
        blocks
    }

    fn flow_blocks(&mut self, flow: &Flow, blocks: &mut Vec<Block>) {
        let fig = flow_figure_id(flow);
        let op = self.report.api.as_ref().and_then(|a| a.operations.iter().find(|o| o.id == flow.entry));
        let name = self.authored.operation(&flow.entry).and_then(|i| given(&i.name)).map(str::to_string);
        let title = match name {
            Some(name) => format!("{name} · {}", flow.title),
            None => flow.title.clone(),
        };
        blocks.push(Block::Heading { level: 3, id: fig.clone(), text: title });
        let asynchronous = flow.steps.iter().filter(|s| s.asynchronous).count();
        let mut caption =
            vec![Inline::text(format!("{} participants, {} messages", flow.participants.len(), flow.steps.len()))];
        if asynchronous > 0 {
            caption.push(Inline::text(format!(" ({asynchronous} asynchronous)")));
        }
        if let Some(op) = op {
            caption.push(Inline::text(" · contract: "));
            caption.push(Inline::Link {
                page: format!("api/{}", op.unit),
                anchor: Some(format!("op-{}", slug(&format!("{} {}", op.method, op.path)))),
                v: format!("{} {}", op.method, op.path),
            });
        }
        let mut rows = Vec::new();
        let mut steps = Vec::new();
        for (i, s) in flow.steps.iter().enumerate() {
            let mut msg = vec![Inline::code(s.label.clone())];
            if let Some(p) = &s.payload {
                msg.push(Inline::text(" · "));
                msg.push(Inline::code(p.clone()));
            }
            let mut kind = vec![match s.kind {
                StepKind::Call => Inline::badge("muted", "call"),
                StepKind::Reply => Inline::badge("muted", "reply"),
                StepKind::Read => Inline::badge("read", "read"),
                StepKind::Write => Inline::badge("write", "write"),
                StepKind::Publish => Inline::badge("observed", "event"),
                StepKind::Trigger => trigger_badge(flow.trigger.kind),
                StepKind::Deliver => Inline::badge("observed", "delivered"),
            }];
            if s.asynchronous {
                kind.push(Inline::text(" "));
                kind.push(Inline::badge("muted", "async"));
            }
            let mut code = Vec::new();
            if let Some(w) = &s.within {
                code.push(Inline::code(w.clone()));
                code.push(Inline::text(" "));
            }
            code.push(self.cite(&s.evidence));
            let pair = vec![self.party(flow, &s.from), Inline::text(" → "), self.party(flow, &s.to)];
            // The same message, said twice: a row to read and a step to walk. The
            // step carries what a reader needs while the diagram holds their
            // attention — what is sent, of what kind, and the line that sends it.
            steps.push(Step {
                edge: format!("m{}", i + 1),
                title: pair.clone(),
                body: msg
                    .iter()
                    .cloned()
                    .chain([Inline::text(" ")])
                    .chain(kind.iter().cloned())
                    .chain([Inline::text(" ")])
                    .chain(code.iter().cloned())
                    .collect(),
            });
            rows.push(vec![vec![Inline::text((i + 1).to_string())], pair, msg, kind, code]);
        }
        // A sequence figure's messages are already numbered in the order they
        // happen, so this table was always a walkthrough that nothing walked
        // (#56). Playing it highlights each message on the diagram as its step
        // becomes current — what the primary path on the container diagram has
        // always done and no request flow could. The walkthrough replaces both
        // the figure and the table because it *is* them: the same messages in
        // the same order, beside the same diagram, one of them current.
        //
        // Highlighting means naming the edge that draws the message, so it is
        // only offered when the rendered figure is still the one these steps
        // describe: a hand-edited IR may have renamed or dropped messages, and a
        // step pointing at an edge that is no longer there would highlight
        // nothing. Then the table stands on its own, as it did before.
        let drawn: Vec<&str> = self
            .diagrams
            .iter()
            .find(|d| d.id == fig)
            .map(|d| d.ir.edges.iter().map(|e| e.id.as_str()).collect())
            .unwrap_or_default();
        let walk =
            steps.len() > 1 && drawn.len() == steps.len() && drawn.iter().zip(&steps).all(|(id, s)| *id == s.edge);
        if walk {
            blocks.push(Block::Steps { diagram: fig.clone(), caption, steps });
        } else {
            if let Some(f) = self.figure(&fig, caption) {
                blocks.push(f);
            }
            blocks.push(Block::Table {
                columns: vec!["#".into(), "From → to".into(), "Message".into(), "Kind".into(), "Code".into()],
                rows,
            });
        }
        if !flow.notes.is_empty() {
            blocks.push(Block::Callout {
                tone: "note".into(),
                title: "Trace limits".into(),
                inl: vec![Inline::text(flow.notes.join("; "))],
            });
        }
    }

    /// Functional requirements for work that doesn't start with a request,
    /// identified the same way, by what triggers them.
    pub(super) fn background_requirement_blocks(&mut self) -> Vec<Block> {
        let flows: Vec<&'a Flow> = self.report.flows.iter().filter(|f| !is_request(f) && worth_drawing(f)).collect();
        if flows.is_empty() {
            return vec![];
        }
        let drawn: Vec<String> = self.drawn_flows().iter().map(|f| f.id.clone()).collect();
        let mut blocks = vec![
            Block::Heading { level: 2, id: "background-processing".into(), text: "Background processing".into() },
            Block::Para {
                inl: vec![Inline::text(
                    "Requirements realised by scheduled jobs, message and event handlers and startup hooks: the system acts without a caller.",
                )],
            },
        ];
        let mut ordered = flows.clone();
        ordered.sort_by(|a, b| (&a.entry, &a.id).cmp(&(&b.entry, &b.id)));
        for flow in ordered {
            // Same rule as an HTTP requirement: the identity is the thing it
            // describes. A background flow is named by its entry point, which
            // does not move when another handler is added beside it.
            let fr = super::behavior::requirement_id(&flow.entry);
            let unit = flow.entry.split(':').next().unwrap_or("").to_string();
            let title = match self.authored.operation(&flow.entry).and_then(|x| given(&x.name)) {
                Some(n) => format!("{fr} · {n}"),
                None => format!("{fr} · {}", flow.title),
            };
            blocks.push(Block::Heading { level: 3, id: fr.to_lowercase(), text: title });
            let mut items: Vec<Vec<Inline>> = Vec::new();
            let mut trigger = vec![
                Inline::strong("Trigger "),
                trigger_badge(flow.trigger.kind),
                Inline::text(" "),
                Inline::code(flow.trigger.label.clone()),
                Inline::text(" → "),
                self.element(&unit),
            ];
            if let Some(ev) = &flow.trigger.evidence {
                trigger.push(self.cite(ev));
            }
            items.push(trigger);
            let mut changes = Vec::new();
            let mut calls = Vec::new();
            let mut emits = Vec::new();
            let mut handled = Vec::new();
            for s in flow.steps.iter().skip(1) {
                let target = match s.kind {
                    StepKind::Write => &mut changes,
                    StepKind::Call => &mut calls,
                    StepKind::Publish => &mut emits,
                    StepKind::Deliver => &mut handled,
                    _ => continue,
                };
                if !target.is_empty() {
                    target.push(Inline::text(", "));
                }
                target.push(Inline::code(s.label.clone()));
                if matches!(s.kind, StepKind::Call | StepKind::Publish | StepKind::Deliver) {
                    target.push(Inline::text(" → "));
                    target.push(self.party(flow, &s.to));
                }
                target.push(self.cite(&s.evidence));
            }
            for (label, list) in
                [("Changes state ", changes), ("Calls ", calls), ("Emits ", emits), ("Continues in ", handled)]
            {
                if !list.is_empty() {
                    let mut inl = vec![Inline::strong(label)];
                    inl.extend(list);
                    items.push(inl);
                }
            }
            if drawn.contains(&flow.id) {
                items.push(vec![
                    Inline::strong("Behaviour "),
                    Inline::Link {
                        page: "flows".into(),
                        anchor: Some(flow_figure_id(flow)),
                        v: "sequence diagram".into(),
                    },
                ]);
            }
            blocks.push(Block::List { items });
        }
        blocks
    }
}
