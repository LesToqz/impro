mod common;
use common::*;
use gtk::prelude::*;

#[test]
fn raw_icons_match_the_desktop_renderer_at_both_scales() {
    run(|| {
        impro_ui::icons::init();
        pump(30);
        let image = gtk::Image::new();
        let window = window(&image);
        let context = image.style_context();
        let theme = gtk::IconTheme::default().unwrap();
        for color in ["#a3b5b8", "#20352e", "#ffffff"] {
            let provider = gtk::CssProvider::new();
            provider
                .load_from_data(format!("* {{ color: {color}; }}").as_bytes())
                .unwrap();
            context.add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
            for (name, size) in [
                ("preferences-system-symbolic", 16),
                ("folder-symbolic", 16),
                ("text-x-generic-symbolic", 16),
                ("sidebar-show-symbolic", 16),
                ("window-close-symbolic", 32),
            ] {
                for scale in [1, 2] {
                    let Some(actual) = impro_ui::icons::symbolic_surface(
                        name,
                        size,
                        scale,
                        &context,
                        image.state_flags(),
                    ) else {
                        // Different desktop themes keep their own source and renderer.
                        continue;
                    };
                    let reference = theme
                        .lookup_icon_for_scale(name, size, scale, gtk::IconLookupFlags::empty())
                        .unwrap()
                        .load_symbolic_for_context(&context)
                        .unwrap()
                        .0
                        .create_surface(scale, None::<&gdk::Window>)
                        .unwrap();
                    let physical = size * scale;
                    let actual =
                        gdk::pixbuf_get_from_surface(&actual, 0, 0, physical, physical).unwrap();
                    let reference =
                        gdk::pixbuf_get_from_surface(&reference, 0, 0, physical, physical).unwrap();
                    assert_eq!(
                        actual.read_pixel_bytes(),
                        reference.read_pixel_bytes(),
                        "{name}, {color}, scale {scale}"
                    );
                }
            }
            context.remove_provider(&provider);
        }
        unsafe {
            window.destroy();
        }
    });
}

#[test]
fn unknown_icons_and_sizes_keep_the_normal_theme_lookup() {
    run(|| {
        impro_ui::icons::init();
        let image =
            gtk::Image::from_icon_name(Some("dialog-warning-symbolic"), gtk::IconSize::Menu);
        impro_ui::icons::prepare_image(&image);
        assert_eq!(
            image.icon_name().as_deref(),
            Some("dialog-warning-symbolic")
        );
        assert!(impro_ui::icons::symbolic_surface(
            "window-close-symbolic",
            23,
            1,
            &image.style_context(),
            image.state_flags()
        )
        .is_none());
        let provider = gtk::CssProvider::new();
        provider
            .load_from_data(b"* { color: rgba(18, 52, 86, 0.5); }")
            .unwrap();
        image
            .style_context()
            .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
        assert!(impro_ui::icons::symbolic_surface(
            "window-close-symbolic",
            32,
            1,
            &image.style_context(),
            image.state_flags()
        )
        .is_none());
        provider.load_from_data(b"* { color: #123456; }").unwrap();
        assert!(impro_ui::icons::symbolic_surface(
            "window-close-symbolic",
            32,
            1,
            &image.style_context(),
            image.state_flags()
        )
        .is_none());
        for (size, scale) in [(i32::MAX, 2), (-16, -1), (16, 0)] {
            assert!(impro_ui::icons::symbolic_surface(
                "window-close-symbolic",
                size,
                scale,
                &image.style_context(),
                image.state_flags()
            )
            .is_none());
        }
    });
}

#[test]
fn prepared_images_match_native_state_rendering() {
    run(|| {
        impro_ui::icons::init();
        pump(30);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let reference = gtk::Image::new();
        let actual = gtk::Image::new();
        let reference_button = gtk::Button::new();
        let actual_button = gtk::Button::new();
        reference_button.add(&reference);
        actual_button.add(&actual);
        row.pack_start(&reference_button, false, false, 0);
        row.pack_start(&actual_button, false, false, 0);
        let window = window(&row);
        for dark in [true, false] {
            impro_ui::apply_style(Some(impro_core::themes::get_theme(Some(if dark {
                "impro-dark"
            } else {
                "impro-light"
            }))))
            .unwrap();
            for (name, size) in [
                ("edit-find-symbolic", 16),
                ("folder-symbolic", 16),
                ("text-x-generic-symbolic", 16),
                ("preferences-system-symbolic", 16),
                ("window-close-symbolic", 32),
            ] {
                reference_button.set_sensitive(true);
                actual_button.set_sensitive(true);
                reference_button.set_state_flags(gtk::StateFlags::NORMAL, true);
                actual_button.set_state_flags(gtk::StateFlags::NORMAL, true);
                reference.set_pixel_size(size);
                actual.set_pixel_size(size);
                reference.set_from_icon_name(Some(name), gtk::IconSize::Menu);
                actual.set_from_icon_name(Some(name), gtk::IconSize::Menu);
                impro_ui::icons::prepare_image(&actual);
                // This environment must exercise the optimized source, not pass
                // silently through the native fallback on a different desktop.
                pump(30);
                assert_eq!(actual.storage_type(), gtk::ImageType::Surface);
                for state in [
                    gtk::StateFlags::NORMAL,
                    gtk::StateFlags::PRELIGHT,
                    gtk::StateFlags::ACTIVE,
                    gtk::StateFlags::INSENSITIVE,
                    gtk::StateFlags::SELECTED,
                    gtk::StateFlags::BACKDROP,
                ] {
                    reference_button.set_sensitive(state != gtk::StateFlags::INSENSITIVE);
                    actual_button.set_sensitive(state != gtk::StateFlags::INSENSITIVE);
                    reference_button.set_state_flags(state, true);
                    actual_button.set_state_flags(state, true);
                    pump(220); // Let GTK's CSS state transition finish.
                    let render = |image: &gtk::Image| {
                        let scale = image.scale_factor();
                        let mut surface = gtk::cairo::ImageSurface::create(
                            gtk::cairo::Format::ARgb32,
                            image.allocated_width() * scale,
                            image.allocated_height() * scale,
                        )
                        .unwrap();
                        surface.set_device_scale(scale as f64, scale as f64);
                        let cr = gtk::cairo::Context::new(&surface).unwrap();
                        image.draw(&cr);
                        drop(cr);
                        let pixels = surface.data().unwrap().to_vec();
                        pixels
                    };
                    assert!(
                        render(&actual) == render(&reference),
                        "{name}, dark {dark}, state {state:?}, scale {}",
                        actual.scale_factor()
                    );
                }
            }
        }
        unsafe {
            window.destroy();
        }
    });
}
