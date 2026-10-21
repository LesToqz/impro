fn main() {
    pkg_config::Config::new()
        .atleast_version("0.72")
        .probe("vte-2.91")
        .expect("libvte-2.91 development files are required");
}
