#![cfg(unix)]
use gtk::prelude::*;
use impro_vte::{Format, Terminal};
use std::{
    cell::RefCell,
    os::unix::ffi::OsStringExt,
    rc::Rc,
    time::{Duration, Instant},
};
#[test]
fn test_spawn_path_preserves_unix_directory_bytes_and_rejects_nuls() {
    gtk::init().unwrap();
    let mut name = format!("/tmp/impro-vte-{}-", std::process::id()).into_bytes();
    name.push(0xff);
    let directory = std::path::PathBuf::from(std::ffi::OsString::from_vec(name));
    std::fs::create_dir(&directory).unwrap();
    let t = Terminal::new();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.add(&t);
    window.show_all();
    let outcome = Rc::new(RefCell::new(None));
    let out = outcome.clone();
    t.spawn_async_path(
        Some(&directory),
        &["/bin/sh", "-c", "read answer; printf path-bytes-preserved"],
        &[],
        glib::SpawnFlags::DEFAULT,
        -1,
        None,
        move |r| {
            *out.borrow_mut() = Some(r);
        },
    );
    // Compare the byte-valued cwd in the child rather than UTF-8 terminal output.
    let deadline = Instant::now() + Duration::from_secs(4);
    while outcome.borrow().is_none() {
        assert!(Instant::now() < deadline);
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let pid = outcome.borrow().as_ref().unwrap().as_ref().unwrap().0;
    assert_eq!(
        std::fs::read_link(format!("/proc/{pid}/cwd")).unwrap(),
        directory
    );
    t.feed_child(b"reply\n");
    while !t
        .text_format(Format::Text)
        .is_some_and(|s| s.contains("path-bytes-preserved"))
    {
        assert!(Instant::now() < deadline);
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }

    let bad = Rc::new(RefCell::new(None));
    let out = bad.clone();
    t.spawn_async_path(
        None,
        &[],
        &[],
        glib::SpawnFlags::DEFAULT,
        -1,
        None,
        move |r| *out.borrow_mut() = Some(r),
    );
    assert!(bad.borrow().as_ref().unwrap().is_err());
    t.feed(b"supported\r\n");
    while !t
        .text_format(Format::Text)
        .is_some_and(|s| s.contains("supported"))
    {
        assert!(Instant::now() < deadline);
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }
    unsafe {
        window.destroy();
    }
    std::fs::remove_dir(directory).unwrap();
}
