//! Windows-only diagnostics for the `unknown-revision` failures seen on the runner.
//!
//! Run: `cargo test -p impro-gitviews --test windows_git_diag -- --nocapture`
//! Every section prints what it observed; the test fails only when the product-style
//! `rev-parse` run through the shared launcher (`impro_core::process::command`) fails.
#![cfg(windows)]
mod support;
use std::ffi::OsString;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use support::{linear, no_hooks_path, Temp, NULL_FILE};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(which: u32) -> isize;
    fn GetFileType(handle: isize) -> u32;
}

/// The three standard handles this process holds: 0 means unknown/invalid, 1 disk, 2 console, 3 pipe.
fn std_handles() -> String {
    [
        ("stdin", 0xFFFF_FFF6u32),
        ("stdout", 0xFFFF_FFF5),
        ("stderr", 0xFFFF_FFF4),
    ]
    .map(|(name, id)| {
        // SAFETY: both calls only inspect a handle value and have no preconditions.
        let (handle, kind) = unsafe {
            let h = GetStdHandle(id);
            (h, GetFileType(h))
        };
        format!("{name}: handle={handle:#x} type={kind}")
    })
    .join(", ")
}

/// Directories `CreateProcess` would search for `git.exe`, with every hit in search order.
fn git_candidates() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        dirs.extend(exe.parent().map(Path::to_path_buf));
    }
    dirs.extend(std::env::current_dir());
    dirs.extend(std::env::var_os("SystemRoot").map(|r| Path::new(&r).join("System32")));
    dirs.extend(std::env::var_os("SystemRoot").map(PathBuf::from));
    dirs.extend(
        std::env::var_os("PATH")
            .iter()
            .flat_map(std::env::split_paths),
    );
    dirs.into_iter()
        .map(|d| d.join("git.exe"))
        .filter(|p| p.is_file())
        .collect()
}

fn show(label: &str, result: std::io::Result<Output>) -> Option<Output> {
    match result {
        Ok(out) => {
            println!(
                "[{label}] status={:?} success={}\n    stdout={:?}\n    stderr={:?}",
                out.status.code(),
                out.status.success(),
                String::from_utf8_lossy(&out.stdout).trim_end(),
                String::from_utf8_lossy(&out.stderr).trim_end(),
            );
            Some(out)
        }
        Err(e) => {
            println!("[{label}] spawn error: {e:?}");
            None
        }
    }
}

/// Settings the product's runner (`log::run_with`) applies, minus the launcher itself.
fn product_env(cmd: &mut Command) {
    cmd.env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("LC_ALL", "C")
        .env("GIT_PAGER", "cat");
}

/// Settings the test fixture (`GitRepo::command`) applies; they succeed on the runner.
fn fixture_env(cmd: &mut Command) {
    cmd.env("GIT_CONFIG_GLOBAL", NULL_FILE)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C");
}

/// The product's argument vector for `diff::resolve(repo, "HEAD")`.
fn product_args(repo: &Path, tail: &[&str]) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["--no-pager", "-c", "core.quotepath=off", "-C"]
        .map(OsString::from)
        .into();
    args.push(repo.into());
    args.extend(tail.iter().map(OsString::from));
    args
}

const VERIFY: &[&str] = &["rev-parse", "--verify", "--end-of-options", "HEAD^{commit}"];

/// A plain `std` command (no shared launcher) with the chosen stdin and environment.
fn variant(repo: &Path, stdin: Stdio, env: fn(&mut Command), tail: &[&str], flags: u32) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(product_args(repo, tail))
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(flags);
    env(&mut cmd);
    cmd
}

#[test]
fn diagnose_git_on_windows() {
    let t = Temp::new();
    let repo = t.0.join("repo");
    linear(&repo, 2);
    println!("== environment ==");
    println!("handles: {}", std_handles());
    for key in [
        "PATH",
        "HOME",
        "USERPROFILE",
        "USERNAME",
        "MSYSTEM",
        "GIT_EXEC_PATH",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
        "XDG_CONFIG_HOME",
    ] {
        println!("{key}={:?}", std::env::var_os(key));
    }
    println!("git env vars: {:?}", {
        let mut keys: Vec<_> = std::env::vars()
            .map(|(k, _)| k)
            .filter(|k| k.starts_with("GIT_"))
            .collect();
        keys.sort();
        keys
    });
    println!("repo={} (temp dir of the runner)", repo.display());
    println!("git.exe candidates in CreateProcess order:");
    for c in git_candidates() {
        println!("    {}", c.display());
    }

    println!("\n== git identity ==");
    show(
        "git --version",
        Command::new("git")
            .arg("--version")
            .stdin(Stdio::null())
            .output(),
    );
    show(
        "git config --show-origin --list (as the product would see it)",
        Command::new("git")
            .args(["config", "--show-origin", "--list"])
            .current_dir(&repo)
            .stdin(Stdio::null())
            .output(),
    );
    show(
        "git config --get-all safe.directory",
        Command::new("git")
            .args(["config", "--get-all", "safe.directory"])
            .current_dir(&repo)
            .stdin(Stdio::null())
            .output(),
    );

    println!("\n== ownership / safe.directory (hypothesis 2) ==");
    // Product environment, user and system config active: "detected dubious ownership" lands in stderr.
    show(
        "rev-parse --git-dir, default config",
        variant(
            &repo,
            Stdio::null(),
            product_env,
            &["rev-parse", "--git-dir"],
            0,
        )
        .output(),
    );
    show(
        "rev-parse --git-dir, config isolated like the fixture",
        variant(
            &repo,
            Stdio::null(),
            fixture_env,
            &["rev-parse", "--git-dir"],
            0,
        )
        .output(),
    );

    println!("\n== product-style rev-parse --verify --end-of-options HEAD^{{commit}} ==");
    let a = show(
        "A stdin=inherit env=product",
        variant(&repo, Stdio::inherit(), product_env, VERIFY, 0).output(),
    );
    let b = show(
        "B stdin=null    env=product",
        variant(&repo, Stdio::null(), product_env, VERIFY, 0).output(),
    );
    let c = show(
        "C stdin=inherit env=fixture",
        variant(&repo, Stdio::inherit(), fixture_env, VERIFY, 0).output(),
    );
    let d = show(
        "D stdin=null    env=fixture",
        variant(&repo, Stdio::null(), fixture_env, VERIFY, 0).output(),
    );
    show(
        "E stdin=inherit env=product + CREATE_NO_WINDOW (the old launcher)",
        variant(
            &repo,
            Stdio::inherit(),
            product_env,
            VERIFY,
            CREATE_NO_WINDOW,
        )
        .output(),
    );
    show(
        "F stdin=null    env=product + CREATE_NO_WINDOW",
        variant(&repo, Stdio::null(), product_env, VERIFY, CREATE_NO_WINDOW).output(),
    );

    println!("\n== argument handling (hypothesis 3), stdin=null env=product ==");
    for (label, tail) in [
        (
            "no --end-of-options, with ^{commit}",
            &["rev-parse", "--verify", "HEAD^{commit}"][..],
        ),
        (
            "--end-of-options, no ^{commit}",
            &["rev-parse", "--verify", "--end-of-options", "HEAD"],
        ),
        (
            "plain rev-parse --verify HEAD",
            &["rev-parse", "--verify", "HEAD"],
        ),
        ("rev-parse HEAD", &["rev-parse", "HEAD"]),
    ] {
        show(
            label,
            variant(&repo, Stdio::null(), product_env, tail, 0).output(),
        );
    }

    println!("\n== config differences (hypothesis 4), stdin=null ==");
    let hooks = format!("core.hooksPath={}", no_hooks_path());
    show(
        "product env + GIT_CONFIG_NOSYSTEM=1 only",
        variant(&repo, Stdio::null(), product_env, VERIFY, 0)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output(),
    );
    show(
        "product env + GIT_CONFIG_GLOBAL=NUL only",
        variant(&repo, Stdio::null(), product_env, VERIFY, 0)
            .env("GIT_CONFIG_GLOBAL", NULL_FILE)
            .output(),
    );
    show(
        "product env + core.hooksPath override only",
        Command::new("git")
            .args(["-c", &hooks])
            .args(product_args(&repo, VERIFY))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs([("GIT_OPTIONAL_LOCKS", "0"), ("LC_ALL", "C")])
            .output(),
    );

    println!("\n== summary ==");
    let ok = |o: &Option<Output>| o.as_ref().is_some_and(|o| o.status.success());
    println!(
        "A(inherit/product)={} B(null/product)={} C(inherit/fixture)={} D(null/fixture)={}",
        ok(&a),
        ok(&b),
        ok(&c),
        ok(&d)
    );

    // The launcher the product uses: default null stdin, no console window.
    let mut fixed = impro_core::process::command("git");
    fixed
        .args(product_args(&repo, VERIFY))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    product_env(&mut fixed);
    let out = show(
        "FIXED launcher impro_core::process::command",
        fixed.output(),
    )
    .expect("git must start through the shared launcher");
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    assert!(
        out.status.success() && sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "the product-style rev-parse must succeed through impro_core::process::command: {out:?}"
    );
}
