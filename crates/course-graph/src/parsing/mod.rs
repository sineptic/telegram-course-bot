use std::{cell::RefCell, collections::BTreeMap, rc::Rc, str::FromStr};

use ariadne::{Color, Label, Report, ReportKind, Source};
use chumsky::prelude::*;
use prototypes::DequePrototype;

use crate::{card::Card, deque::Deque};

mod prototypes;

impl FromStr for Deque {
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
        let mut cards = BTreeMap::<String, Rc<RefCell<Card>>>::new();
        while !card_prototypes.is_empty() {
            let (name, card) = {
                let Some((name, dependencies)) = card_prototypes
                    .iter()
                    .find(|(_, dependencies)| dependencies.iter().all(|d| cards.contains_key(d)))
                else {
                    todo!("report cycle detection")
                };
                // Safety: there is no cycles, because all dependencies already added, which don't have cycles
                let card = unsafe {
                    Card::new(
                        name,
                        dependencies
                            .iter()
                            .map(|dependencie| cards.get(dependencie).unwrap().clone()),
                    )
                };
                for dependencie in &card.borrow().dependencies {
                    dependencie
                        .borrow_mut()
                        .dependents
                        .push(Rc::downgrade(&card));
                }
                (name.to_owned(), card)
            };
            let prev = cards.insert(name.clone(), card);
            debug_assert!(prev.is_none());
            card_prototypes.remove(&name).unwrap();
        }

        let top_level_cards = cards
            .into_values()
            .filter(|c| c.borrow().dependents.is_empty());
        // Safety: all card checked for unique name when parsed
        Ok(unsafe { Deque::new(top_level_cards) })
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
