mod common;
use common::*;
use gtk::prelude::*;

#[test]
fn test_application_startup_uses_saved_theme_before_show() {
    run(|| {
        for theme_id in ["impro-dark", "impro-light"] {
            let dir = tempfile::tempdir().unwrap();
            let config = dir.path().join("config");
            let store = impro_core::settings::Store::new(config.join("impro/settings.json"));
            let values = serde_json::json!({"theme":theme_id,"reduce_motion":true,"terminal_shell":"/bin/sh"});
            store.save(values.as_object().unwrap()).unwrap();
            let shot = dir.path().join("startup.png");
            let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_impro"))
                .arg(dir.path())
                .env("XDG_CONFIG_HOME", &config)
                .env("IMPRO_QA_SCREENSHOT", &shot)
                .env("IMPRO_QA_TIMING", "1")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            poll(|| {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    panic!("startup screenshot deadline exceeded");
                }
                child.try_wait().unwrap().is_some()
            });
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("first-paint-ms="));
            let pixbuf = gtk::Image::from_file(&shot).pixbuf().unwrap();
            let bytes = pixbuf.read_pixel_bytes();
            let pixel = |x: i32, y: i32| {
                let offset = (y * pixbuf.rowstride() + x * pixbuf.n_channels()) as usize;
                &bytes[offset..offset + 3]
            };
            let theme = impro_core::themes::get_theme(Some(theme_id));
            let rgb = |hex: &str| {
                let color = gdk::RGBA::parse(hex).unwrap();
                [color.red(), color.green(), color.blue()].map(|c| (c * 255.0).round() as u8)
            };
            assert_eq!(pixel(450, 200), rgb(&theme.ui["bg"]));
            // The complete first capture includes the dock at 30% of workspace height.
            // An earlier capture had a small interim dock and editor pixels here.
            assert_eq!(
                pixel(450, pixbuf.height() - 220),
                rgb(&theme.ui["sidebar_bg"])
            );
            assert_eq!(
                pixel(450, pixbuf.height() - 50),
                rgb(&theme.terminal.background)
            );
        }
    });
}
