use std::{collections::HashMap, hash::Hash, str::FromStr};

use chumsky::prelude::*;

#[derive(Debug, Clone)]
pub struct CardName {
    pub name: String,
    pub span: SimpleSpan,
}
impl PartialEq for CardName {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}
impl Eq for CardName {}
impl Hash for CardName {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}
impl CardName {
    #[cfg(test)]
    fn new(name: &str, range: std::ops::Range<usize>) -> Self {
        assert_eq!(
            name.len(),
            range.end - range.start,
            "name is '{name}', range is {range:?}"
        );
        CardName {
            name: name.to_string(),
            span: SimpleSpan::from(range),
        }
    }
}

#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct DequePrototype {
    pub cards: HashMap<CardName, Vec<CardName>>,
}
impl FromStr for DequePrototype {
    type Err = chumsky::error::Rich<'static, char>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut cards = HashMap::new();
        enum State {
            Default,
            NameParsing {
                name: String,
                start: usize,
            },
            DependenciesParsing {
                name: CardName,
                dependencies: Vec<CardName>,
            },
            DependencyParsing {
                name: CardName,
                dependencies: Vec<CardName>,
                current_dependency: String,
                current_dependency_start: usize,
            },
        }
        let mut state = State::Default;
        for (ix, ch) in s.char_indices() {
            match state {
                State::Default => match ch {
                    '\n' => (),
                    ch if ch.is_alphanumeric() => {
                        state = State::NameParsing {
                            name: String::from(ch),
                            start: ix,
                        }
                    }
                    _ => {
                        return Err(Rich::custom(
                            SimpleSpan::from(ix..ix + 1),
                            "unexpected character, card name expected",
                        ));
                    }
                },
                State::NameParsing { mut name, start } => match ch {
                    '\n' => {
                        let name = CardName {
                            name: name.to_lowercase(),
                            span: SimpleSpan::from(start..ix),
                        };
                        let prev = cards.insert(name.clone(), Vec::new());
                        if prev.is_some() {
                            return Err(Rich::custom(
                                name.span,
                                "duplicate definition of card dependencies",
                            ));
                        }
                        state = State::Default;
                    }
                    ch if ch.is_alphanumeric() || ch == ' ' => {
                        name.push(ch);
                        state = State::NameParsing { name, start };
                    }
                    ':' => {
                        if name.ends_with(' ') {
                            let count = name.len() - name.trim_end().len();
                            assert!(count > 0);
                            return Err(Rich::custom(
                                SimpleSpan::from(ix - count..ix),
                                "space in not allowed between card name and column",
                            ));
                        }
                        let name = CardName {
                            name: name.to_lowercase(),
                            span: SimpleSpan::from(start..ix),
                        };
                        state = State::DependenciesParsing {
                            name,
                            dependencies: Vec::new(),
                        };
                    }
                    _ => {
                        return Err(Rich::custom(
                            SimpleSpan::from(ix..ix + 1),
                            "unexpected character, expected card name continuation or column",
                        ));
                    }
                },
                State::DependenciesParsing { name, dependencies } => match ch {
                    ' ' => {
                        state = State::DependenciesParsing { name, dependencies };
                    }
                    ch if ch.is_alphanumeric() => {
                        state = State::DependencyParsing {
                            name,
                            dependencies,
                            current_dependency: String::from(ch),
                            current_dependency_start: ix,
                        };
                    }
                    '\n' => {
                        return Err(Rich::custom(
                            SimpleSpan::from(name.span.start..ix),
                            "dependency name expected",
                        ));
                    }
                    _ => {
                        return Err(Rich::custom(
                            SimpleSpan::from(ix..ix + 1),
                            "unexpected character",
                        ));
                    }
                },
                State::DependencyParsing {
                    name,
                    mut dependencies,
                    mut current_dependency,
                    current_dependency_start,
                } => match ch {
                    '\n' => {
                        let spaces_at_the_end =
                            current_dependency.len() - current_dependency.trim_end().len();
                        let _ = current_dependency
                            .split_off(current_dependency.len() - spaces_at_the_end);
                        let dependency = CardName {
                            name: current_dependency.to_lowercase(),
                            span: SimpleSpan::from(
                                current_dependency_start..ix - spaces_at_the_end,
                            ),
                        };
                        dependencies.push(dependency);
                        let prev = cards.insert(name.clone(), dependencies);
                        if prev.is_some() {
                            return Err(Rich::custom(
                                name.span,
                                "duplicate definition of card dependencies",
                            ));
                        }
                        state = State::Default;
                    }
                    ch if ch.is_alphanumeric() || ch == ' ' => {
                        current_dependency.push(ch);
                        state = State::DependencyParsing {
                            name,
                            dependencies,
                            current_dependency,
                            current_dependency_start,
                        };
                    }
                    ',' => {
                        if current_dependency.ends_with(' ') {
                            let count =
                                current_dependency.len() - current_dependency.trim_end().len();
                            assert!(count > 0);
                            return Err(Rich::custom(
                                SimpleSpan::from(ix - count..ix),
                                "space in not allowed in card names",
                            ));
                        }
                        let dependency = CardName {
                            name: current_dependency.to_lowercase(),
                            span: SimpleSpan::from(current_dependency_start..ix),
                        };
                        if dependencies.contains(&dependency) {
                            return Err(Rich::custom(
                                dependency.span,
                                "duplicated dependency specified",
                            ));
                        }
                        dependencies.push(dependency);
                        state = State::DependenciesParsing { name, dependencies };
                    }
                    _ => {
                        return Err(Rich::custom(
                            SimpleSpan::from(ix..ix + 1),
                            "unexpected character",
                        ));
                    }
                },
            }
        }
        match state {
            State::Default => (),
            State::NameParsing { name, start } => {
                let name = CardName {
                    name: name.to_lowercase(),
                    span: SimpleSpan::from(start..s.len()),
                };
                let prev = cards.insert(name.clone(), Vec::new());
                if prev.is_some() {
                    return Err(Rich::custom(
                        name.span,
                        "duplicate definition of card dependencies",
                    ));
                }
            }
            State::DependenciesParsing {
                name,
                dependencies: _,
            } => {
                return Err(Rich::custom(
                    SimpleSpan::from(name.span.start..s.len()),
                    "dependency name expected",
                ));
            }
            State::DependencyParsing {
                name,
                mut dependencies,
                mut current_dependency,
                current_dependency_start,
            } => {
                let spaces_at_the_end =
                    current_dependency.len() - current_dependency.trim_end().len();
                let _ = current_dependency.split_off(current_dependency.len() - spaces_at_the_end);
                let dependency = CardName {
                    name: current_dependency.to_lowercase(),
                    span: SimpleSpan::from(current_dependency_start..s.len() - spaces_at_the_end),
                };
                dependencies.push(dependency);
                let prev = cards.insert(name.clone(), dependencies);
                if prev.is_some() {
                    return Err(Rich::custom(
                        name.span,
                        "duplicate definition of card dependencies",
                    ));
                }
            }
        }
        Ok(Self { cards })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn card_prototype_parsing() {
        assert!(DequePrototype::from_str("asdf: ").is_err());
        assert!(DequePrototype::from_str(": hi").is_err());
        assert!(DequePrototype::from_str("hi\n: there").is_err());
        assert!(DequePrototype::from_str("hi: there, ").is_err());
        assert!(DequePrototype::from_str(" hi: there").is_err());
        assert_eq!(
            DequePrototype::from_str("a: b").unwrap(),
            DequePrototype {
                cards: [(CardName::new("a", 0..1), vec![CardName::new("b", 3..4)])]
                    .into_iter()
                    .collect()
            }
        );
        assert_eq!(
            DequePrototype::from_str("hI").unwrap(),
            DequePrototype {
                cards: [(CardName::new("hi", 0..2), vec![])].into_iter().collect()
            }
        );
        assert_eq!(
            DequePrototype::from_str("some: long, line, should, BE, handled").unwrap(),
            DequePrototype {
                cards: [(
                    CardName::new("some", 0..4),
                    vec![
                        CardName::new("long", 6..10),
                        CardName::new("line", 12..16),
                        CardName::new("should", 18..24),
                        CardName::new("be", 26..28),
                        CardName::new("handled", 30..37)
                    ]
                )]
                .into_iter()
                .collect()
            }
        );
        assert_eq!(
            DequePrototype::from_str("spaces is allowed: a, and here too, with CASEinsensiTIVITY")
                .unwrap(),
            DequePrototype {
                cards: [(
                    CardName::new("spaces is allowed", 0..17),
                    vec![
                        CardName::new("a", 19..20),
                        CardName::new("and here too", 22..34),
                        CardName::new("with caseinsensitivity", 36..58)
                    ]
                )]
                .into_iter()
                .collect()
            }
        );
    }

    #[test]
    fn deque_prototype_parsing() {
        assert!(
            DequePrototype::from_str(
                r#"
 wrong whitespace: b
b
"#
            )
            .is_err()
        );
        assert_eq!(
            DequePrototype::from_str(
                r#"
a: b
b
"#
            )
            .unwrap(),
            DequePrototype {
                cards: [
                    (CardName::new("a", 1..2), vec![CardName::new("b", 4..5)]),
                    (CardName::new("b", 6..7), vec![])
                ]
                .into_iter()
                .collect()
            }
        );
        assert_eq!(
            DequePrototype::from_str(
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
                        CardName::new("first multi word", 0..16),
                        vec![
                            CardName::new("some node", 18..27),
                            CardName::new("other node", 29..39)
                        ]
                    ),
                    (CardName::new("some node", 40..49), vec![]),
                    (
                        CardName::new("other node", 50..60),
                        vec![CardName::new("some node", 62..71)]
                    )
                ]
                .into_iter()
                .collect()
            }
        );
    }
}
