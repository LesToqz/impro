//! The real `impro-graph` scene loaded with a fixture (positions fixed, not simulated).

use impro_graph::view::scene::{Built, Scene};
use impro_graph::Graph;

use crate::fixture::Fixture;

pub fn build_scene(fx: &Fixture) -> Scene {
    let mut scene = Scene::new();
    scene.set_size(1920.0, 1080.0);
    scene.apply(Built::global(Graph::from_pairs(fx.x.len(), &fx.edges)));
    // let the simulation settle first, then pin the fixture positions
    scene.layout.settle(600);
    for i in 0..fx.x.len() {
        scene.layout.set_position(i, fx.x[i], fx.y[i]);
    }
    for _ in 0..600 {
        if !scene.advance(1.0 / 60.0) {
            break;
        }
    }
    scene
}
