use gtk::pango;
use gtk::prelude::*;
use gtk::{Align, IconLookupFlags, IconTheme, Image, Label, TextDirection, Widget, gdk};

use crate::config::IconFallback;

pub fn image_for_app(app_id: Option<&str>, icon_size: i32, fallback: IconFallback) -> Widget {
    if let Some(display) = gdk::Display::default() {
        let theme = IconTheme::for_display(&display);
        if let Some(paintable) = lookup(&theme, app_id, icon_size) {
            let image = Image::new();
            image.set_pixel_size(icon_size);
            image.set_paintable(Some(&paintable));
            return image.upcast();
        }
    }
    match fallback {
        IconFallback::Default => {
            let image = Image::from_icon_name("application-x-executable");
            image.set_pixel_size(icon_size);
            image.upcast()
        }
        IconFallback::Initials => initials_label(app_id, icon_size).upcast(),
    }
}

fn initials_label(app_id: Option<&str>, icon_size: i32) -> Label {
    let label = Label::new(Some(&initials(app_id)));
    label.add_css_class("app-initials");
    label.set_width_chars(2);
    label.set_justify(gtk::Justification::Center);
    label.set_ellipsize(pango::EllipsizeMode::None);
    label.set_size_request(icon_size, icon_size);
    label.set_halign(Align::Center);
    label.set_valign(Align::Center);
    label
}

fn initials(app_id: Option<&str>) -> String {
    let name = app_id.unwrap_or("");
    let word = name
        .rsplit('.')
        .find(|part| !part.is_empty())
        .unwrap_or(name);
    let letters: String = word.chars().filter(|c| c.is_alphabetic()).take(2).collect();
    if letters.is_empty() {
        "?".to_string()
    } else {
        letters.to_uppercase()
    }
}

fn lookup(theme: &IconTheme, app_id: Option<&str>, size: i32) -> Option<gdk::Paintable> {
    let app_id = app_id?;
    for name in icon_name_candidates(app_id) {
        if theme.has_icon(&name) {
            return Some(
                theme
                    .lookup_icon(
                        &name,
                        &[],
                        size,
                        1,
                        TextDirection::None,
                        IconLookupFlags::empty(),
                    )
                    .into(),
            );
        }
    }
    None
}

fn icon_name_candidates(app_id: &str) -> Vec<String> {
    let last = app_id.rsplit('.').next().unwrap_or(app_id);
    let mut names = vec![
        app_id.to_string(),
        app_id.to_lowercase(),
        last.to_string(),
        last.to_lowercase(),
    ];
    names.dedup();
    names
}
