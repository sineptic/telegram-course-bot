pub use dot_structures::{Edge, EdgeTy, Id, NodeId, Stmt, Vertex};

pub fn id_from_string(s: impl AsRef<str>) -> Id {
    Id::Escaped(format!("\"{}\"", s.as_ref()))
}
pub fn vertex_from_id(id: Id) -> Vertex {
    Vertex::N(NodeId(id, None))
}

pub fn edge_stmt_from_strings(name1: &str, name2: &str) -> Stmt {
    Stmt::Edge(edge_from_ids(id_from_string(name1), id_from_string(name2)))
}

pub fn edge_from_ids(id1: Id, id2: Id) -> Edge {
    let vertex1 = vertex_from_id(id1);
    let vertex2 = vertex_from_id(id2);
    Edge {
        ty: EdgeTy::Pair(vertex1, vertex2),
        attributes: vec![],
    }
}
