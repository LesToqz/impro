//! The settled picture is the reference quality: pixel hashes of fit, zoom 1
//! and a zoomed-in view guard it against changes of the renderer, and the moving
//! level of detail must hand back to exactly that picture.

use impro_graph::geometry::Camera;
use impro_graph::layout::Rng;
use impro_graph::view::draw::{PaintOptions, Painter};
use impro_graph::view::scene::{Built, Scene};
use impro_graph::Graph;

const W: f64 = 1280.0;
const H: f64 = 800.0;

fn pairs(n: usize, edges: usize) -> Vec<(u32, u32)> {
    let mut rng = Rng::new(7);
    let mut out = Vec::new();
    let mut ends = vec![0u32];
    for i in 1..n as u32 {
        let target = ends[(rng.unit() * ends.len() as f32) as usize % ends.len()];
        out.push((i, target));
        ends.push(target);
        ends.push(i);
    }
    while out.len() < edges {
        let a = (rng.unit() * n as f32) as u32 % n as u32;
        let b = ends[(rng.unit() * ends.len() as f32) as usize % ends.len()];
        if a != b {
            out.push((a, b));
        }
    }
    out
}

fn hash(painter: &mut Painter, scene: &Scene) -> u64 {
    let surface = painter
        .render(scene, W, H, PaintOptions::default())
        .expect("surface");
    let mut png = Vec::new();
    surface.write_to_png(&mut png).expect("PNG");
    png.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The hash after the label bitmaps are warm (they render within a budget
/// per frame, so the first frames are still incomplete).
fn warm_hash(painter: &mut Painter, scene: &Scene) -> u64 {
    for _ in 0..4 {
        hash(painter, scene);
    }
    hash(painter, scene)
}

fn scene() -> Scene {
    let (n, edges) = (1200, 3200);
    let mut scene = Scene::new();
    scene.set_size(W, H);
    scene.apply(Built::global(Graph::from_pairs(n, &pairs(n, edges))));
    while scene.advance(1.0 / 60.0) {}
    scene
}

fn cameras(scene: &Scene) -> [(&'static str, Camera); 3] {
    let mut fit = Camera::default();
    fit.fit(scene.layout.bounds().expect("bounds"), W, H, 40.0);
    let centred = |zoom: f64| Camera {
        pan_x: W / 2.0,
        pan_y: H / 2.0,
        zoom,
    };
    [
        ("fit", fit),
        ("zoom 1", centred(1.0)),
        ("zoomed in", centred((fit.zoom * 3.0).min(6.0))),
    ]
}

/// Hashes of the settled picture at each camera, recorded from the renderer
/// before the moving level of detail existed.
// Exact settled masks preserve the original picture after every zoom path.
const SETTLED: [(&str, u64); 3] = [
    ("fit", 6855316917292932711),
    ("zoom 1", 10625964012002035238),
    ("zoomed in", 11118940497073692799),
];

#[test]
fn settled_pictures_keep_their_pixels() {
    let mut scene = scene();
    let mut painter = Painter::new();
    let mut got = Vec::new();
    for (name, cam) in cameras(&scene) {
        scene.cam.cam = cam;
        scene.cam.target_zoom = cam.zoom;
        // the later frames are served from the connection cache
        let first = warm_hash(&mut painter, &scene);
        assert!(painter.stats.connections_reused, "{name}");
        assert_eq!(first, hash(&mut painter, &scene), "{name} cached");
        assert_eq!(
            first,
            warm_hash(&mut Painter::new(), &scene),
            "{name} fresh"
        );
        got.push((name, first));
    }
    println!("{got:?}");
    assert_eq!(got, SETTLED);
}

#[test]
fn moving_frames_differ_and_the_next_settled_frame_is_exact() {
    let mut scene = scene();
    let (_, cam) = cameras(&scene)[2];
    scene.cam.cam = cam;
    scene.cam.target_zoom = cam.zoom;
    let mut painter = Painter::new();
    let settled = warm_hash(&mut painter, &scene);
    painter.motion = true;
    let moving = warm_hash(&mut painter, &scene);
    assert!(painter.stats.lines > 0 && painter.stats.nodes > 0);
    assert_ne!(moving, settled, "moving frames use the cheaper level");
    assert!(!painter.stats.connections_reused);
    painter.motion = false;
    assert_eq!(warm_hash(&mut painter, &scene), settled);
    assert_eq!(warm_hash(&mut Painter::new(), &scene), settled);
}

#[test]
fn scene_reports_geometry_motion_only() {
    let mut scene = scene();
    assert!(!scene.moving());
    scene.cam.target_zoom = scene.cam.cam.zoom * 1.5;
    assert!(scene.moving(), "zoom easing");
    scene.cam.target_zoom = scene.cam.cam.zoom;
    assert!(!scene.moving());
    scene.cam.vx = 3.0;
    assert!(scene.moving(), "fling");
    scene.cam.vx = 0.0;
    scene.layout.restart();
    assert!(scene.moving(), "layout running");
}

#[test]
fn kept_group_coverage_paints_exactly_what_a_fresh_painter_does() {
    let mut scene = scene();
    let (_, cam) = cameras(&scene)[2];
    scene.cam.cam = cam;
    scene.cam.target_zoom = cam.zoom;
    let options = PaintOptions {
        hide_labels: true,
        scale: 1.0,
    };
    let png = |painter: &mut Painter, scene: &Scene| {
        let mut out = Vec::new();
        painter
            .render(scene, W, H, options)
            .expect("surface")
            .write_to_png(&mut out)
            .expect("PNG");
        out
    };
    let mut kept = Painter::new();
    png(&mut kept, &scene);
    let (mut hits, mut frames) = (0, 0);
    for target in [Some(0), None] {
        scene.set_hover(target);
        while scene.advance(1.0 / 60.0) {
            assert!(!scene.moving(), "only appearance eases");
            // the pictures are compared on a sample of the frames
            kept.render(&scene, W, H, options);
            hits += kept.stats.group_hits;
            frames += 1;
            if frames % 6 == 0 {
                let frame = png(&mut kept, &scene);
                assert!(frame == png(&mut Painter::new(), &scene), "{target:?}");
            }
        }
    }
    assert!(hits > 0, "easing frames reuse kept groups");
}

#[test]
fn settled_label_pixels_do_not_depend_on_the_previous_zoom() {
    let mut scene = Scene::new();
    scene.set_size(W, H);
    scene.apply(Built::global(Graph::from_pairs(1, &[])));
    while scene.advance(1.0 / 60.0) {}
    let mut painter = Painter::new();
    for zoom in [0.96, 0.8, 1.05, 2.0] {
        scene.cam.set_zoom_now(zoom, W, H);
        warm_hash(&mut painter, &scene);
        scene.cam.set_zoom_now(1.0, W, H);
        let from_history = warm_hash(&mut painter, &scene);
        let fresh = warm_hash(&mut Painter::new(), &scene);
        assert_eq!(from_history, fresh, "settled after zoom {zoom}");
    }
}

#[test]
fn a_bounded_label_warmup_finishes_and_stops_requesting_frames() {
    let mut scene = Scene::new();
    scene.set_size(W, H);
    scene.apply(Built::global(Graph::from_pairs(360, &[])));
    for i in 0..360 {
        scene.layout.pin(
            i,
            (i % 24) as f32 * 30.0 - 360.0,
            (i / 24) as f32 * 40.0 - 280.0,
        );
    }
    while scene.advance(1.0 / 60.0) {}
    let mut painter = Painter::new();
    hash(&mut painter, &scene);
    assert!(painter.stats.labels > 0 && painter.stats.labels < 360);
    assert!(painter.needs_frames());
    for _ in 0..5 {
        hash(&mut painter, &scene);
        if !painter.needs_frames() {
            break;
        }
    }
    assert_eq!(painter.stats.labels, 360);
    assert!(!painter.needs_frames());
    let finished = hash(&mut painter, &scene);
    assert_eq!(finished, warm_hash(&mut Painter::new(), &scene));
    assert!(!painter.needs_frames());
}
