use std::collections::HashMap;

use course_graph::{
    card::Card,
    deque::Deque,
    progress_store::{TaskProgress, TaskProgressStore, TaskProgressStoreExt},
};

fn main() {
    let a0 = Card::new("a0", 0, []);
    let a1 = Card::new("a1", 1, [a0.clone()]);
    let a2 = Card::new("a2", 2, [a1.clone()]);
    let a3 = Card::new("a3", 3, [a2.clone()]);
    let b0 = Card::new("b0", 4, []);
    let b1 = Card::new("b1", 5, [b0.clone()]);
    let b2 = Card::new("b2", 6, [b1.clone()]);
    let b3 = Card::new("b3", 7, [b2.clone()]);
    let d0 = Card::new("d0", 8, [a3.clone(), b3.clone()]);
    let d1 = Card::new("d1", 9, [d0.clone()]);
    let d2 = Card::new("d2", 10, [d1.clone()]);
    let d3 = Card::new("d3", 11, [d2.clone()]);
    let d4 = Card::new("d4", 12, [d3.clone()]);
    let c0 = Card::new("c0", 13, []);
    let c1 = Card::new("c1", 14, [c0.clone()]);
    let c2 = Card::new("c2", 15, [c1.clone()]);

    let smth = Card::new("smth", 16, [d1.clone(), c0.clone()]);

    let c3 = Card::new("c3", 17, [c2.clone(), smth.clone()]);
    let c4 = Card::new("c4", 18, [c3.clone()]);

    let deque = Deque::new([d4, c4]);

    let graph = deque.generate_graph();

    let mut progress_store = HashMap::new();
    deque.for_each_repeated(&mut |x| {
        progress_store.init(x.borrow().id);
    });
    progress_store.extend([
        (0, TaskProgress::Good),
        (1, TaskProgress::Good),
        (2, TaskProgress::Good),
        (3, TaskProgress::Good), // a3
        (4, TaskProgress::Good),
        (5, TaskProgress::Good),
        (6, TaskProgress::Good),
        (7, TaskProgress::Good),
        (8, TaskProgress::Good),
        (9, TaskProgress::Good),
        (10, TaskProgress::Good),
        (13, TaskProgress::Good),
        (14, TaskProgress::Failed), // c1
        (15, TaskProgress::Good),
        (16, TaskProgress::Good),
        (17, TaskProgress::Good),
    ]);
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
