use std::collections::BTreeMap;

use chumsky::{prelude::*, text::Char};
type Err<'a> = extra::Err<chumsky::error::Rich<'a, char>>;

#[cfg_attr(test, derive(Debug, PartialEq))]
struct CardPrototype {
    name: String,
    dependencies: Vec<String>,
}

impl CardPrototype {
    pub fn parser<'a>() -> impl Parser<'a, &'a str, CardPrototype, Err<'a>> {
        fn ident<'a>() -> impl Parser<'a, &'a str, String, Err<'a>> {
            none_of(":")
            .filter(|c: &char| c.is_alphanumeric() || c.is_inline_whitespace())
            .repeated()
            .at_least(1)
            .to_slice()
            .try_map(|x: &str, span| {
                let x = x.to_lowercase();
                if x.starts_with(" ") || x.ends_with(" ") {
                    return Err(Rich::custom(span, "Card name should containt whitespaces only inside, not at start or end"));
                }
                if x == "finish" {
                    return Err(Rich::custom(span, "Card shouldn't be named 'finish'"));
                }
                Ok(x)
            })
        }
        ident()
            .then(
                just(": ")
                    .ignore_then(ident().separated_by(just(", ")).at_least(1).collect())
                    .or_not(),
            )
            .map(|(name, dependencies)| CardPrototype {
                name,
                dependencies: dependencies.unwrap_or_default(),
            })
    }
}

#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct DequePrototype {
    pub cards: BTreeMap<String, Vec<String>>,
}
impl DequePrototype {
    pub fn parser<'a>() -> impl Parser<'a, &'a str, DequePrototype, Err<'a>> {
        CardPrototype::parser()
        .separated_by(just("\n").repeated().at_least(1))
        .collect::<Vec<_>>()
        .delimited_by(just("\n").repeated(), just("\n").repeated())
        .try_map(|card_prototypes, span| {
            let mut cards = BTreeMap::new();
            for card_prototype in card_prototypes {
                let prev = cards.insert(
                    card_prototype.name.clone(),
                    card_prototype.dependencies,
                );
                if prev.is_some() {
                    return Err(Rich::custom(
                        span,
                        format!(
                            "Card names should be unique, which is not true for '{}' card",
                            card_prototype.name
                        ),
                    ));
                }
            }
            for dependencie in cards.values().flatten() {
                if !cards.contains_key(dependencie) {
                    return Err(Rich::custom(
                        span,
                        format!("Each dependencie should be presented as card, but '{dependencie}' isn't"),
                    ));
                }
            }
            Ok(DequePrototype {
                cards
            })
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn card_prototype_parsing() {
        let parser = CardPrototype::parser();
        assert!(parser.parse("").has_errors());
        assert!(parser.parse("asdf: ").has_errors());
        assert!(parser.parse(": hi").has_errors());
        assert!(parser.parse("hi\n: there").has_errors());
        assert!(parser.parse("hi: there, ").has_errors());
        assert!(parser.parse(" hi: there").has_errors());
        assert_eq!(
            parser.parse("a: b").unwrap(),
            CardPrototype {
                name: "a".into(),
                dependencies: vec!["b".into()]
            }
        );
        assert_eq!(
            parser.parse("hI").unwrap(),
            CardPrototype {
                name: "hi".into(),
                dependencies: vec![]
            }
        );
        assert_eq!(
            parser
                .parse("some: long, line, should, BE, handled")
                .unwrap(),
            CardPrototype {
                name: "some".into(),
                dependencies: vec![
                    "long".into(),
                    "line".into(),
                    "should".into(),
                    "be".into(),
                    "handled".into()
                ]
            }
        );
        assert_eq!(
            parser
                .parse("spaces is allowed: a, and here too, with CASEinsensiTIVITY")
                .unwrap(),
            CardPrototype {
                name: "spaces is allowed".into(),
                dependencies: vec![
                    "a".into(),
                    "and here too".into(),
                    "with caseinsensitivity".into()
                ]
            }
        );
    }

    #[test]
    fn deque_prototype_parsing() {
        let parser = DequePrototype::parser();
        assert!(
            parser
                .parse(
                    r#"
a: b
"#
                )
                .has_errors()
        );
        assert!(
            parser
                .parse(
                    r#"
 wrong whitespace: b
b
"#
                )
                .has_errors()
        );
        assert_eq!(
            parser
                .parse(
                    r#"
a: b
b
"#
                )
                .unwrap(),
            DequePrototype {
                cards: [("a".into(), vec!["b".into()]), ("b".into(), vec![])]
                    .into_iter()
                    .collect()
            }
        );
        assert_eq!(
            parser
                .parse(
                    r#"
first multi word: some node, other node
some node
other node: some node
"#
                )
                .unwrap(),
            DequePrototype {
                cards: [
                    (
                        "first multi word".into(),
                        vec!["some node".into(), "other node".into()]
                    ),
                    ("some node".into(), vec![]),
                    ("other node".into(), vec!["some node".into()])
                ]
                .into_iter()
                .collect()
            }
        );
    }
}
