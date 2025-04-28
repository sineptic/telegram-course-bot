use std::{collections::HashMap, str::FromStr};

use course_graph::{
    graph::CourseGraph,
    progress_store::{TaskProgress, TaskProgressStoreExt},
};

fn main() {
    let course_graph = CourseGraph::from_str(
        r#"
a0
a1: a0
a2: a1
a3: a2

b0
b1: b0
b2: b1
b3: b2

d0: a3, b3
d1: d0
d2: d1
d3: d2
d4: d3

c0
c1: c0
c2: c1
c3: c2, smth
c4: c3
smth: d1, c0
"#,
    )
    .unwrap_or_else(|err| {
        println!("{err}");
        panic!("parsing error");
    });

    let graph = course_graph.generate_graph();

    let mut progress_store = HashMap::new();
    course_graph.init_store(&mut progress_store);
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
    course_graph.detect_recursive_fails(&mut progress_store);
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
