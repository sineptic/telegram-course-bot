#[allow(clippy::manual_non_exhaustive)]
pub struct Card {
    pub name: String,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Default)]
pub struct CardNode {
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
}

impl Card {
    /// # Safety
    /// Should not contain cycles
    pub fn new(name: impl ToString, dependencies: impl IntoIterator<Item = String>) -> Card {
        let name = name.to_string();
        assert!(!name.contains('"'));
        let dependencies = dependencies.into_iter().collect::<Vec<_>>();
        assert!(dependencies.iter().all(|x| !x.contains('"')));
        Card { name, dependencies }
    }
}
