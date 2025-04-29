use std::collections::HashMap;

use progress_store::{TaskProgress, TaskProgressStoreExt};

pub mod card;
pub mod graph;
pub mod parsing;
pub mod progress_store;
mod utils;

pub fn generate_graph_chart(
    mut graph: dot_structures::Graph,
    progress_store: &HashMap<String, TaskProgress>,
) -> Vec<u8> {
    progress_store
        .generate_stmts()
        .into_iter()
        .for_each(|stmt| {
            graph.add_stmt(stmt);
        });

    let mut ctx = graphviz_rust::printer::PrinterContext::default();

    graphviz_rust::exec(
        graph,
        &mut ctx,
        vec![graphviz_rust::cmd::Format::Png.into()],
    )
    .expect("Failed to run 'dot'")
}
