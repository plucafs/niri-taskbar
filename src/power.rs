use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Label, Orientation, Popover};

use crate::config::Position;
use crate::taskbar::popover_position;

const ACTIONS: [PowerAction; 5] = [
    PowerAction::Lock,
    PowerAction::Suspend,
    PowerAction::Hibernate,
    PowerAction::Reboot,
    PowerAction::Poweroff,
];

#[derive(Clone, Copy)]
pub enum PowerAction {
    Lock,
    Suspend,
    Hibernate,
    Reboot,
    Poweroff,
}

impl PowerAction {
    fn label(self) -> &'static str {
        match self {
            PowerAction::Lock => "Lock",
            PowerAction::Suspend => "Suspend",
            PowerAction::Hibernate => "Hibernate",
            PowerAction::Reboot => "Restart",
            PowerAction::Poweroff => "Shut down",
        }
    }

    fn command(self) -> (&'static str, Vec<&'static str>) {
        match self {
            PowerAction::Lock => ("swaylock", vec![]),
            PowerAction::Suspend => ("systemctl", vec!["--no-block", "suspend"]),
            PowerAction::Hibernate => ("systemctl", vec!["--no-block", "hibernate"]),
            PowerAction::Reboot => ("systemctl", vec!["--no-block", "reboot"]),
            PowerAction::Poweroff => ("systemctl", vec!["--no-block", "poweroff"]),
        }
    }

    fn supported(self, state: &str) -> bool {
        match self {
            PowerAction::Lock | PowerAction::Reboot | PowerAction::Poweroff => true,
            PowerAction::Suspend => state.split_whitespace().any(|token| token == "mem"),
            PowerAction::Hibernate => state.split_whitespace().any(|token| token == "disk"),
        }
    }
}

pub struct PowerMenu {
    pub button: Button,
    popover: Popover,
    error: Label,
    items: Rc<Vec<Button>>,
}

impl PowerMenu {
    pub fn new(position: Position) -> Self {
        let button = Button::new();
        let image = gtk::Image::from_icon_name("system-shutdown-symbolic");
        image.set_pixel_size(22);
        button.set_child(Some(&image));
        button.add_css_class("taskbar-button");
        button.set_focusable(false);
        button.set_tooltip_text(Some("Power"));

        let popover = Popover::new();
        popover.set_parent(&button);
        popover.set_position(popover_position(position));

        let error = Label::new(None);
        error.add_css_class("power-error");
        error.set_visible(false);
        error.set_wrap(true);
        error.set_max_width_chars(36);

        let list = GtkBox::new(Orientation::Vertical, 2);
        list.add_css_class("window-menu");
        let mut buttons = Vec::new();
        for action in ACTIONS {
            let item = menu_item(action.label());
            list.append(&item);
            buttons.push(item);
        }
        list.append(&error);
        popover.set_child(Some(&list));
        let items = Rc::new(buttons);

        let (tx, rx) = async_channel::unbounded::<Result<(), String>>();
        for (item, action) in items.iter().zip(ACTIONS) {
            let tx = tx.clone();
            let items = Rc::clone(&items);
            let error = error.clone();
            item.connect_clicked(move |_| {
                let (cmd, args) = action.command();
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let result = std::process::Command::new(cmd)
                        .args(&args)
                        .output()
                        .map_err(|err| format!("{cmd}: {err}"))
                        .and_then(|out| {
                            if out.status.success() {
                                Ok(())
                            } else {
                                let stderr = String::from_utf8_lossy(&out.stderr);
                                let message = stderr.trim();
                                if message.is_empty() {
                                    Err(format!("{cmd} exited with {}", out.status))
                                } else {
                                    Err(message.chars().take(160).collect())
                                }
                            }
                        });
                    let _ = tx.send_blocking(result);
                });
                for other in items.iter() {
                    other.set_sensitive(false);
                }
                error.set_visible(false);
            });
        }

        let error_for_rx = error.clone();
        let items_for_rx = Rc::clone(&items);
        let popover_for_rx = popover.clone();
        glib::MainContext::default().spawn_local(async move {
            while let Ok(result) = rx.recv().await {
                match result {
                    Ok(()) => popover_for_rx.popdown(),
                    Err(message) => {
                        error_for_rx.set_text(&message);
                        error_for_rx.set_visible(true);
                        for item in items_for_rx.iter() {
                            item.set_sensitive(true);
                        }
                    }
                }
            }
        });

        Self {
            button,
            popover,
            error,
            items,
        }
    }

    pub fn open(&self, position: Position) {
        self.popover.set_position(popover_position(position));
        self.error.set_visible(false);
        let state = std::fs::read_to_string("/sys/power/state").unwrap_or_default();
        for (item, action) in self.items.iter().zip(ACTIONS) {
            item.set_sensitive(action.supported(&state));
        }
        self.popover.popup();
    }

    pub fn set_visible(&self, visible: bool) {
        self.button.set_visible(visible);
        if !visible {
            self.popover.popdown();
        }
    }

    pub fn close(&self) {
        self.popover.popdown();
    }
}

fn menu_item(label: &str) -> Button {
    let item = Button::new();
    item.add_css_class("window-row-select");
    item.set_focusable(false);
    let text = Label::new(Some(label));
    text.set_halign(gtk::Align::Start);
    text.set_margin_top(3);
    text.set_margin_bottom(3);
    text.set_margin_start(6);
    text.set_margin_end(6);
    item.set_child(Some(&text));
    item
}
