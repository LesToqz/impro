//! Native Windows resource embedding; Linux builds do no resource work.
#[path = "windows/rcgen.rs"]
mod rcgen;

fn main() {
    println!("cargo:rerun-if-env-changed=IMPRO_SKIP_WIN_RESOURCES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    for file in [
        "windows/impro.rc.in",
        "windows/rcgen.rs",
        "windows/impro.manifest",
        "../../packaging/icons/windows/impro.ico",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    if std::env::var("IMPRO_SKIP_WIN_RESOURCES").as_deref() == Ok("1") {
        // Plain build output: visible with cargo -vv, never a cargo warning.
        println!("impro: Windows resources skipped for type checking");
        return;
    }
    compile_resources();
}

// Cargo selects platform-specific build dependencies for the build-script host.
// The supported resource build runs natively in MSYS2 UCRT64; Linux target checks
// return above before requiring the Windows host's embed-resource dependency.
#[cfg(windows)]
fn compile_resources() {
    use std::path::PathBuf;
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("out dir"));
    let version = std::env::var("CARGO_PKG_VERSION").expect("package version");
    let template = std::fs::read_to_string(dir.join("windows/impro.rc.in")).expect("rc template");
    let icon = dir.join("../../packaging/icons/windows/impro.ico");
    let manifest = dir.join("windows/impro.manifest");
    let rc = rcgen::render(
        &template,
        &version,
        icon.to_str().expect("icon path"),
        manifest.to_str().expect("manifest path"),
    );
    let rc_path = out.join("impro-res.rc");
    std::fs::write(&rc_path, rc).expect("write generated rc");
    // No definitions: the script is fully expanded, nothing is passed with -D.
    embed_resource::compile_for(&rc_path, ["impro"], embed_resource::NONE)
        .manifest_required()
        .expect("Windows resources must compile");
}

#[cfg(not(windows))]
fn compile_resources() {
    panic!("build Windows resources on a Windows host, or set IMPRO_SKIP_WIN_RESOURCES=1 for type checking");
}
