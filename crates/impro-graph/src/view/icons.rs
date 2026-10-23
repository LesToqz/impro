//! The panel's small line icons, drawn with cairo on a 24-unit grid so they
//! need no icon theme and follow the CSS text colour.

use std::f64::consts::{PI, TAU};

use gtk::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Settings,
    Wand,
    Close,
    Reset,
    GraphType,
    Chevron,
}

/// Draws `icon` into a `size × size` square at the origin with the current source.
pub fn draw(cr: &cairo::Context, icon: Icon, size: f64) {
    let _ = cr.save();
    cr.scale(size / 24.0, size / 24.0);
    cr.set_line_width(2.0);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    match icon {
        Icon::Close => {
            cr.move_to(18.0, 6.0);
            cr.line_to(6.0, 18.0);
            cr.move_to(6.0, 6.0);
            cr.line_to(18.0, 18.0);
            let _ = cr.stroke();
        }
        Icon::Chevron => {
            cr.move_to(6.0, 9.0);
            cr.line_to(12.0, 15.0);
            cr.line_to(18.0, 9.0);
            let _ = cr.stroke();
        }
        Icon::Reset => {
            // an open circle with an arrow head at its upper left end
            cr.new_path();
            cr.arc(12.0, 12.0, 8.5, -2.55, 2.0 * PI - 2.0);
            let _ = cr.stroke();
            cr.move_to(3.0, 3.5);
            cr.line_to(3.0, 8.5);
            cr.line_to(8.0, 8.5);
            let _ = cr.stroke();
        }
        Icon::GraphType => {
            for (x, y) in [(6.0, 6.0), (18.0, 7.0), (12.0, 18.0)] {
                cr.new_sub_path();
                cr.arc(x, y, 3.0, 0.0, TAU);
            }
            let _ = cr.stroke();
            cr.move_to(8.2, 8.2);
            cr.line_to(10.8, 15.4);
            cr.move_to(15.6, 9.6);
            cr.line_to(13.1, 15.5);
            let _ = cr.stroke();
        }
        Icon::Settings => {
            // a gear: eight rounded teeth around a hub
            let teeth = 8;
            for i in 0..teeth {
                let a = TAU * i as f64 / teeth as f64;
                let (c, s) = (a.cos(), a.sin());
                cr.move_to(12.0 + c * 7.5, 12.0 + s * 7.5);
                cr.line_to(12.0 + c * 10.0, 12.0 + s * 10.0);
            }
            let _ = cr.stroke();
            cr.new_path();
            cr.arc(12.0, 12.0, 7.5, 0.0, TAU);
            let _ = cr.stroke();
            cr.new_path();
            cr.arc(12.0, 12.0, 3.0, 0.0, TAU);
            let _ = cr.stroke();
        }
        Icon::Wand => {
            // the drawing's ink spans 2.5..22.8 by 1.5..21.5: shifted so its
            // centre sits on the grid centre like the other icons
            cr.translate(-0.65, 0.5);
            cr.move_to(3.5, 20.5);
            cr.line_to(15.5, 8.5);
            let _ = cr.stroke();
            cr.set_line_width(3.0);
            cr.move_to(14.0, 10.0);
            cr.line_to(17.0, 13.0);
            let _ = cr.stroke();
            cr.set_line_width(2.0);
            // sparkles
            for (x, y, r) in [(18.0, 5.0, 2.5), (6.0, 6.0, 1.8), (20.0, 17.0, 1.8)] {
                cr.move_to(x - r, y);
                cr.line_to(x + r, y);
                cr.move_to(x, y - r);
                cr.line_to(x, y + r);
            }
            let _ = cr.stroke();
        }
    }
    let _ = cr.restore();
}

/// A flat button showing `icon` in the text colour of its style.
pub fn icon_button(icon: Icon, tooltip: &str, size: i32) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_focus_on_click(false);
    button.set_tooltip_text(Some(tooltip));
    button.style_context().add_class("impro-graph-icon-button");
    if let Some(a) = button.accessible() {
        a.set_name(tooltip);
    }
    let area = gtk::DrawingArea::new();
    area.set_size_request(size, size);
    area.set_halign(gtk::Align::Center);
    area.set_valign(gtk::Align::Center);
    let owner = button.downgrade();
    area.connect_draw(move |area, cr| {
        let Some(button) = owner.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let style = button.style_context();
        let c = style.color(button.state_flags());
        cr.set_source_rgba(c.red(), c.green(), c.blue(), c.alpha());
        draw(cr, icon, area.allocated_width() as f64);
        glib::Propagation::Proceed
    });
    button.add(&area);
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink(icon: Icon) -> usize {
        let mut s = cairo::ImageSurface::create(cairo::Format::ARgb32, 16, 16).expect("surface");
        {
            let cr = cairo::Context::new(&s).expect("cr");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            draw(&cr, icon, 16.0);
        }
        let stride = s.stride() as usize;
        let data = s.data().expect("data");
        (0..16)
            .flat_map(|y| (0..16).map(move |x| (x, y)))
            .filter(|(x, y)| data[y * stride + x * 4 + 3] > 40)
            .count()
    }

    #[test]
    fn every_icon_draws_something_inside_its_square() {
        for icon in [
            Icon::Settings,
            Icon::Wand,
            Icon::Close,
            Icon::Reset,
            Icon::GraphType,
            Icon::Chevron,
        ] {
            let n = ink(icon);
            assert!(n > 12 && n < 150, "{icon:?}: {n} pixels");
        }
    }
}
