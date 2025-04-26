use std::{cell::RefCell, collections::BTreeMap, rc::Rc, str::FromStr};

use chumsky::prelude::*;
use prototypes::DequePrototype;

use crate::{card::Card, deque::Deque};

mod prototypes;

impl FromStr for Deque {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let deque_prototype = DequePrototype::parser().parse(s);
        if deque_prototype.has_errors() {
            todo!("report deque parsing errors");
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
