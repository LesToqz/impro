//! Candidate B: plain cairo vector (see `cairo_scene`).
//! Flags: `--aa none|fast|default` (cairo antialias for nodes).

use cairo::Antialias;
use graph_lib_bench::cairo_scene::Plain;
use graph_lib_bench::fixture;
use graph_lib_bench::harness::{header, sweep, Opts};

fn main() {
    let o = Opts::parse();
    let fx = fixture::by_name(&o.fixture);
    let aa = match o.value("--aa") {
        Some("none") => Antialias::None,
        Some("fast") => Antialias::Fast,
        _ => Antialias::Default,
    };
    header(&format!("B plain cairo (node aa {aa:?})"), &o);
    sweep(&fx, &o, |w, h, s| Plain::new(w, h, s, aa).map(|p| Box::new(p) as _));
}
