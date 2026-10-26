use gtk::{gdk, IconLookupFlags, IconTheme, Image, TextDirection};

pub fn image_for_app(app_id: Option<&str>, icon_size: i32) -> Image {
    let image = Image::new();
    image.set_pixel_size(icon_size);
    if let Some(display) = gdk::Display::default() {
        let theme = IconTheme::for_display(&display);
        if let Some(paintable) = lookup(&theme, app_id, icon_size) {
            image.set_paintable(Some(&paintable));
            return image;
        }
    }
    image.set_icon_name(Some("application-x-executable"));
    image
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
