use std::{collections::HashMap, str::FromStr};

use ariadne::{Color, Label, Report, ReportKind, Source};
use chumsky::prelude::*;
use prototypes::DequePrototype;

use crate::{card::CardNode, graph::CourseGraph};

mod prototypes;

impl FromStr for CourseGraph {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let deque_prototype = DequePrototype::parser().parse(s);
        if deque_prototype.has_errors() {
            let mut errors = Vec::new();
            for err in deque_prototype.errors() {
                report_error(s, &mut errors, err);
            }
            return Err(String::from_utf8(errors).unwrap());
        }
        let mut card_prototypes = deque_prototype.unwrap().cards;
        let mut graph_cards = HashMap::<String, CardNode>::new();
        while !card_prototypes.is_empty() {
            let Some((name, _)) = card_prototypes
                .iter()
                .find(|(_, dependencies)| dependencies.iter().all(|d| graph_cards.contains_key(d)))
            else {
                todo!("report cycle detection")
            };
            let (name, dependencies) = card_prototypes.remove_entry(&name.to_owned()).unwrap();
            for dependencie in &dependencies {
                graph_cards
                    .get_mut(dependencie)
                    .unwrap()
                    .dependents
                    .push(name.clone());
            }
            // Safety: there is no cycles, because all dependencies already added, which don't have cycles
            graph_cards.insert(
                name,
                CardNode {
                    dependencies,
                    dependents: vec![],
                },
            );
        }
        Ok(CourseGraph {
            text: s.to_owned(),
            cards: graph_cards,
        })
    }
}

fn report_error(input: &str, output: &mut Vec<u8>, err: &Rich<'_, char>) {
    let span = err.span();
    let span = span.start()..span.end();
    let mut labels = vec![
        Label::new(span.clone())
            .with_message(err.reason())
            .with_color(Color::Red),
    ];
    for (rich_context, span) in err.contexts() {
        let span = span.start()..span.end();
        labels.push(Label::new(span).with_message(rich_context));
    }
    Report::build(ReportKind::Error, span)
        .with_labels(labels)
        .finish()
        .write_for_stdout(Source::from(input), output)
        .unwrap();
}
