use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Box as GtkBox, DropDown, Label, Orientation};

use crate::config::{Config, IconPosition, Position};
use crate::Bar;

const TITLE: &str = "barthing settings";

pub fn open(app: &Application, bar: &std::rc::Rc<Bar>) {
    for window in app.windows() {
        if window.title().as_deref() == Some(TITLE) {
            window.present();
            return;
        }
    }

    let window = ApplicationWindow::new(app);
    window.set_title(Some(TITLE));
    window.set_default_size(340, 180);

    let root = GtkBox::new(Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    window.set_child(Some(&root));

    let position_row = row("Bar position", &Position::ALL.map(|p| p.label()));
    let icon_row = row("Icon position", &IconPosition::ALL.map(|p| p.label()));
    root.append(&position_row.container);
    root.append(&icon_row.container);

    let config = bar.config.borrow().clone();
    position_row
        .dropdown
        .set_selected(Position::ALL.iter().position(|p| *p == config.position).unwrap_or(0) as u32);
    icon_row
        .dropdown
        .set_selected(
            IconPosition::ALL
                .iter()
                .position(|p| *p == config.icon_position)
                .unwrap_or(0) as u32,
        );

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(8);

    let close = gtk::Button::with_label("Close");
    let apply = gtk::Button::with_label("Apply");
    apply.add_css_class("suggested-action");

    let window_weak = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = window_weak.upgrade() {
            window.close();
        }
    });

    let bar_apply = bar.clone();
    let position_dd = position_row.dropdown.clone();
    let icon_dd = icon_row.dropdown.clone();
    apply.connect_clicked(move |_| {
        let mut config: Config = bar_apply.config.borrow().clone();
        config.position = Position::ALL
            .get(position_dd.selected() as usize)
            .copied()
            .unwrap_or(config.position);
        config.icon_position = IconPosition::ALL
            .get(icon_dd.selected() as usize)
            .copied()
            .unwrap_or(config.icon_position);
        config.save();
        bar_apply.apply_config(config);
    });

    buttons.append(&close);
    buttons.append(&apply);
    root.append(&buttons);

    window.present();
}

struct Row {
    container: GtkBox,
    dropdown: DropDown,
}

fn row(label: &str, options: &[&str]) -> Row {
    let container = GtkBox::new(Orientation::Horizontal, 12);
    let text = Label::new(Some(label));
    text.set_halign(gtk::Align::Start);
    text.set_hexpand(true);
    container.append(&text);
    let dropdown = DropDown::from_strings(options);
    container.append(&dropdown);
    Row { container, dropdown }
}
