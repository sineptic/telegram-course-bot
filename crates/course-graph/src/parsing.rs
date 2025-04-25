use std::collections::BTreeMap;

use chumsky::{prelude::*, text::Char};
type Err<'a> = extra::Err<chumsky::error::Rich<'a, char>>;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CardPrototype {
    name: String,
    dependencies: Vec<String>,
}

impl CardPrototype {
    pub fn parser<'a>() -> impl Parser<'a, &'a str, CardPrototype, Err<'a>> {
        fn ident<'a>() -> impl Parser<'a, &'a str, String, Err<'a>> {
            none_of(":")
                .filter(|c: &char| c.is_alphabetic() || c.is_inline_whitespace())
                .repeated()
                .at_least(1)
                .to_slice()
                .map(|x: &str| x.to_lowercase())
        }
        ident()
            .then(
                just(": ")
                    .ignore_then(ident().separated_by(just(", ")).at_least(1).collect())
                    .or_not(),
            )
            .map(|(name, dependencies)| CardPrototype {
                name: name.to_owned(),
                dependencies: dependencies.unwrap_or_default(),
            })
    }
}

#[derive(Debug)]
pub struct DequePrototype {
    cards: BTreeMap<String, Vec<String>>,
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn card_parsing() {
        let parser = CardPrototype::parser();
        assert!(parser.parse("").has_errors());
        assert!(parser.parse("asdf: ").has_errors());
        assert!(parser.parse(": hi").has_errors());
        assert!(parser.parse("hi\n: there").has_errors());
        assert!(parser.parse("hi: there, ").has_errors());
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
}
