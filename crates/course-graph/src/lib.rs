use progress_store::TaskProgressStoreExt;

pub mod card;
pub mod graph;
pub mod parsing;
pub mod progress_store;
mod utils;

pub fn print_graph(graph: dot_structures::Graph) -> Vec<u8> {
    let mut ctx = graphviz_rust::printer::PrinterContext::default();

    graphviz_rust::exec(
        graph,
        &mut ctx,
        vec![graphviz_rust::cmd::Format::Png.into()],
    )
    .expect("Failed to run 'dot'")
}

pub fn generate_graph(
    mut graph: dot_structures::Graph,
    progress_store: &impl TaskProgressStoreExt,
) -> dot_structures::Graph {
    progress_store
        .generate_stmts()
        .into_iter()
        .for_each(|stmt| {
            graph.add_stmt(stmt);
        });

    graph
}
