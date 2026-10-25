mod common;
use common::*;
use gtk::prelude::*;

#[test]
fn test_default_icon_list_is_set_after_first_paint() {
    run(|| {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        impro::app_icon::install_after_first_paint(&window);
        window.show_all();
        poll(|| gtk::Window::default_icon_list().len() >= 5);
        let sizes: Vec<i32> = gtk::Window::default_icon_list()
            .iter()
            .map(|pixbuf| pixbuf.width())
            .collect();
        assert!(sizes.contains(&16) && sizes.contains(&256), "{sizes:?}");
        assert!(sizes.iter().all(|&size| size > 0));
        window.hide();
    });
}
