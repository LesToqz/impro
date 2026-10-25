//! Native Windows resource embedding; Linux builds do no resource work.
fn main() {
    println!("cargo:rerun-if-env-changed=IMPRO_SKIP_WIN_RESOURCES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    for file in [
        "windows/impro.rc",
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
    let version = std::env::var("CARGO_PKG_VERSION").expect("package version");
    let components: Vec<_> = version
        .split(['.', '-', '+'])
        .take(3)
        .map(|s| s.parse::<u16>().expect("numeric version component"))
        .collect();
    assert_eq!(components.len(), 3, "expected major.minor.patch");
    let definitions = [
        format!("IMPRO_VERSION=\"{version}\""),
        format!(
            "IMPRO_VERSION_NUM={},{},{},0",
            components[0], components[1], components[2]
        ),
    ];
    embed_resource::compile_for("windows/impro.rc", ["impro"], &definitions)
        .manifest_required()
        .expect("Windows resources must compile");
}

#[cfg(not(windows))]
fn compile_resources() {
    panic!("build Windows resources on a Windows host, or set IMPRO_SKIP_WIN_RESOURCES=1 for type checking");
}
