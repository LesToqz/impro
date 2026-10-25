fn main() {
    // The binding only exists on Unix; never probe libvte for any other target.
    if std::env::var_os("CARGO_CFG_UNIX").is_none() {
        return;
    }
    pkg_config::Config::new()
        .atleast_version("0.72")
        .probe("vte-2.91")
        .expect("libvte-2.91 development files are required");
}
