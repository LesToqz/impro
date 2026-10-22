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
