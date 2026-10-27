use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, DropDown, Grid, Image, Label,
    Orientation, ScrolledWindow, SpinButton, Switch,
};

use crate::config::{BarLayer, Config, GroupBy, IconFallback, IconPosition, Position, SortMode};
use crate::Bar;

const TITLE: &str = "barthing settings";

pub fn open(app: &Application, bar: &std::rc::Rc<Bar>) {
    for window in app.windows() {
        if window.title().as_deref() == Some(TITLE) {
            window.present();
            return;
        }
    }

    let config = bar.config.borrow().clone();
    let live = bar.taskbar.borrow().ordered_entries();

    let window = ApplicationWindow::new(app);
    window.set_title(Some(TITLE));
    window.set_default_size(440, 480);

    let root = GtkBox::new(Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    window.set_child(Some(&root));

    let grid = Grid::new();
    grid.set_row_spacing(10);
    grid.set_column_spacing(16);
    root.append(&grid);

    let position = dropdown_row(&grid, 0, "Bar position", &Position::ALL.map(|p| p.label()));
    position.set_selected(
        Position::ALL
            .iter()
            .position(|p| *p == config.position)
            .unwrap_or(0) as u32,
    );

    let icon_position = dropdown_row(&grid, 1, "Icon position", &IconPosition::ALL.map(|p| p.label()));
    icon_position.set_selected(
        IconPosition::ALL
            .iter()
            .position(|p| *p == config.icon_position)
            .unwrap_or(0) as u32,
    );

    let layer = dropdown_row(&grid, 2, "Bar layer", &BarLayer::ALL.map(|l| l.label()));
    layer.set_selected(
        BarLayer::ALL
            .iter()
            .position(|l| *l == config.bar_layer)
            .unwrap_or(0) as u32,
    );

    let thickness = SpinButton::with_range(16.0, 128.0, 1.0);
    thickness.set_value(config.thickness as f64);
    attach_row(&grid, 3, "Bar height (px)", &thickness);

    let icon_size = SpinButton::with_range(12.0, 64.0, 1.0);
    icon_size.set_value(config.icon_size as f64);
    attach_row(&grid, 4, "Icon size (px)", &icon_size);

    let group = Switch::new();
    group.set_active(config.group_by == GroupBy::App);
    group.set_halign(gtk::Align::Start);
    group.set_valign(gtk::Align::Center);
    attach_row(&grid, 5, "Group windows by app", &group);

    let sort = dropdown_row(&grid, 6, "Icon order", &SortMode::ALL.map(|s| s.label()));
    sort.set_selected(
        SortMode::ALL
            .iter()
            .position(|s| *s == config.sort_mode)
            .unwrap_or(0) as u32,
    );

    let show_settings = Switch::new();
    show_settings.set_active(config.show_settings_icon);
    show_settings.set_halign(gtk::Align::Start);
    show_settings.set_valign(gtk::Align::Center);
    attach_row(&grid, 7, "Show settings icon", &show_settings);

    let show_tooltips = Switch::new();
    show_tooltips.set_active(config.show_tooltips);
    show_tooltips.set_halign(gtk::Align::Start);
    show_tooltips.set_valign(gtk::Align::Center);
    attach_row(&grid, 8, "Show hover info", &show_tooltips);

    let icon_fallback = dropdown_row(
        &grid,
        9,
        "Missing icon fallback",
        &IconFallback::ALL.map(|f| f.label()),
    );
    icon_fallback.set_selected(
        IconFallback::ALL
            .iter()
            .position(|f| *f == config.icon_fallback)
            .unwrap_or(0) as u32,
    );

    let custom_idx = SortMode::ALL
        .iter()
        .position(|s| *s == SortMode::Custom)
        .unwrap_or(0) as u32;

    let mut labels: HashMap<String, String> = live
        .iter()
        .map(|(key, label)| (key.clone(), label.clone()))
        .collect();
    let mut keys: Vec<String> = if config.sort_mode == SortMode::Custom {
        let mut keys = config.custom_order.clone();
        for (key, _) in &live {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
        keys
    } else {
        live.iter().map(|(key, _)| key.clone()).collect()
    };
    for key in &config.custom_order {
        if !labels.contains_key(key) {
            labels.insert(key.clone(), format!("{key} (not open)"));
        }
        if !keys.contains(key) {
            keys.push(key.clone());
        }
    }

    let keys: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(keys));
    let labels: Rc<HashMap<String, String>> = Rc::new(labels);

    let order_label = Label::new(Some("Custom order"));
    order_label.set_halign(gtk::Align::Start);
    order_label.set_margin_top(4);

    let order_box = GtkBox::new(Orientation::Vertical, 4);
    let scroll = ScrolledWindow::new();
    scroll.set_child(Some(&order_box));
    scroll.set_min_content_height(160);
    scroll.set_vexpand(true);
    root.append(&order_label);
    root.append(&scroll);

    rebuild_order_rows(&order_box, &keys, &labels);

    let order_visible = sort.selected() == custom_idx;
    order_label.set_visible(order_visible);
    scroll.set_visible(order_visible);
    {
        let order_label = order_label.clone();
        let scroll = scroll.clone();
        sort.connect_selected_notify(move |sort| {
            let visible = sort.selected() == custom_idx;
            order_label.set_visible(visible);
            scroll.set_visible(visible);
        });
    }

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(8);

    let close = Button::with_label("Close");
    let apply = Button::with_label("Apply");
    apply.add_css_class("suggested-action");

    let window_weak = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = window_weak.upgrade() {
            window.close();
        }
    });

    let bar_apply = bar.clone();
    let keys_apply = Rc::clone(&keys);
    apply.connect_clicked(move |_| {
        let mut config: Config = bar_apply.config.borrow().clone();
        config.position = Position::ALL
            .get(position.selected() as usize)
            .copied()
            .unwrap_or(config.position);
        config.icon_position = IconPosition::ALL
            .get(icon_position.selected() as usize)
            .copied()
            .unwrap_or(config.icon_position);
        config.bar_layer = BarLayer::ALL
            .get(layer.selected() as usize)
            .copied()
            .unwrap_or(config.bar_layer);
        config.thickness = thickness.value().round().clamp(16.0, 128.0) as u32;
        config.icon_size = icon_size.value().round().clamp(12.0, 64.0) as u32;
        config.group_by = if group.is_active() {
            GroupBy::App
        } else {
            GroupBy::Window
        };
        config.sort_mode = SortMode::ALL
            .get(sort.selected() as usize)
            .copied()
            .unwrap_or(config.sort_mode);
        if config.sort_mode == SortMode::Custom {
            config.custom_order = keys_apply.borrow().clone();
        }
        config.show_settings_icon = show_settings.is_active();
        config.show_tooltips = show_tooltips.is_active();
        config.icon_fallback = IconFallback::ALL
            .get(icon_fallback.selected() as usize)
            .copied()
            .unwrap_or(config.icon_fallback);
        config.save();
        bar_apply.apply_config(config);
    });

    buttons.append(&close);
    buttons.append(&apply);
    root.append(&buttons);

    window.present();
}

fn rebuild_order_rows(
    container: &GtkBox,
    keys: &Rc<RefCell<Vec<String>>>,
    labels: &Rc<HashMap<String, String>>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    let count = keys.borrow().len();
    for index in 0..count {
        let key = keys.borrow()[index].clone();
        let row = GtkBox::new(Orientation::Horizontal, 6);

        let text = Label::new(Some(labels.get(&key).map(String::as_str).unwrap_or(&key)));
        text.set_halign(gtk::Align::Start);
        text.set_hexpand(true);
        text.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&text);

        let up = move_button("go-up-symbolic", "Move up");
        if index == 0 {
            up.set_sensitive(false);
        } else {
            let keys = Rc::clone(keys);
            let labels = Rc::clone(labels);
            let container = container.clone();
            up.connect_clicked(move |_| {
                swap_rows(&keys, index, index - 1);
                rebuild_order_rows(&container, &keys, &labels);
            });
        }
        row.append(&up);

        let down = move_button("go-down-symbolic", "Move down");
        if index + 1 >= count {
            down.set_sensitive(false);
        } else {
            let keys = Rc::clone(keys);
            let labels = Rc::clone(labels);
            let container = container.clone();
            down.connect_clicked(move |_| {
                swap_rows(&keys, index, index + 1);
                rebuild_order_rows(&container, &keys, &labels);
            });
        }
        row.append(&down);

        container.append(&row);
    }
}

fn swap_rows(keys: &RefCell<Vec<String>>, a: usize, b: usize) {
    let mut keys = keys.borrow_mut();
    if a < keys.len() && b < keys.len() {
        keys.swap(a, b);
    }
}

fn move_button(icon: &str, tooltip: &str) -> Button {
    let button = Button::new();
    let image = Image::from_icon_name(icon);
    image.set_pixel_size(12);
    button.set_child(Some(&image));
    button.set_tooltip_text(Some(tooltip));
    button.set_focusable(false);
    button
}

fn label(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_halign(gtk::Align::Start);
    label
}

fn attach_row<W: IsA<gtk::Widget>>(grid: &Grid, row: i32, text: &str, control: &W) {
    grid.attach(&label(text), 0, row, 1, 1);
    grid.attach(control, 1, row, 1, 1);
}

fn dropdown_row(grid: &Grid, row: i32, text: &str, options: &[&str]) -> DropDown {
    let dropdown = DropDown::from_strings(options);
    dropdown.set_halign(gtk::Align::Start);
    dropdown.set_valign(gtk::Align::Center);
    attach_row(grid, row, text, &dropdown);
    dropdown
}
