#[path = "../tests/common/commit.rs"]
mod commit;
#[path = "../tests/common/mod.rs"]
mod common;
#[path = "../tests/common/graph.rs"]
mod graph;
#[path = "../tests/common/host.rs"]
mod host;
fn main() {
    gtk::init().expect("GTK display");
    graph::tests();
    commit::tests();
    host::tests();
}
