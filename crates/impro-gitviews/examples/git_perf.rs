#[cfg(unix)]
#[path = "unix/git_perf_unix.rs"]
mod unix;
#[cfg(unix)]
fn main() {
    unix::main();
}
#[cfg(not(unix))]
fn main() {}
