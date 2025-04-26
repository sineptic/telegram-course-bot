use std::collections::HashMap;

use course_graph::{
    card::Card,
    deque::Deque,
    progress_store::{TaskProgress, TaskProgressStore, TaskProgressStoreExt},
};

fn main() {
    let a0 = Card::new("a0", []);
    let a1 = Card::new("a1", [a0.clone()]);
    let a2 = Card::new("a2", [a1.clone()]);
    let a3 = Card::new("a3", [a2.clone()]);
    let b0 = Card::new("b0", []);
    let b1 = Card::new("b1", [b0.clone()]);
    let b2 = Card::new("b2", [b1.clone()]);
    let b3 = Card::new("b3", [b2.clone()]);
    let d0 = Card::new("d0", [a3.clone(), b3.clone()]);
    let d1 = Card::new("d1", [d0.clone()]);
    let d2 = Card::new("d2", [d1.clone()]);
    let d3 = Card::new("d3", [d2.clone()]);
    let d4 = Card::new("d4", [d3.clone()]);
    let c0 = Card::new("c0", []);
    let c1 = Card::new("c1", [c0.clone()]);
    let c2 = Card::new("c2", [c1.clone()]);

    let smth = Card::new("smth", [d1.clone(), c0.clone()]);

    let c3 = Card::new("c3", [c2.clone(), smth.clone()]);
    let c4 = Card::new("c4", [c3.clone()]);

    let deque = Deque::new([d4, c4]);

    let graph = deque.generate_graph();

    let mut progress_store = HashMap::new();
    deque.for_each_repeated(&mut |x| {
        progress_store.init(&x.borrow().name);
    });
    progress_store.extend(
        [
            ("a0", TaskProgress::Good),
            ("a1", TaskProgress::Good),
            ("a2", TaskProgress::Good),
            ("a3", TaskProgress::Good), // a3
            ("b0", TaskProgress::Good),
            ("b1", TaskProgress::Good),
            ("b2", TaskProgress::Good),
            ("b3", TaskProgress::Good),
            ("d0", TaskProgress::Good),
            ("d1", TaskProgress::Good),
            ("d2", TaskProgress::Good),
            ("c0", TaskProgress::Good),
            ("c1", TaskProgress::Failed), // c1
            ("c2", TaskProgress::Good),
            ("c3", TaskProgress::Good),
            ("smth", TaskProgress::Good),
        ]
        .into_iter()
        .map(|(id, progress)| (id.to_owned(), progress)),
    );
    progress_store.detect_recursive_fails(&deque);
    let mut graph = graph.clone();
    progress_store
        .generate_stmts()
        .into_iter()
        .for_each(|stmt| {
            graph.add_stmt(stmt);
        });

    let mut ctx = graphviz_rust::printer::PrinterContext::default();
    let output = graphviz_rust::exec(
        graph,
        &mut ctx,
        vec![graphviz_rust::cmd::Format::Png.into()],
    )
    .expect("Failed to run 'dot'");

    std::fs::write("a.png", output).expect("failed to write to 'a.png' file");
}
