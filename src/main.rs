mod config;
mod icons;
mod niri;
mod settings;
mod taskbar;

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk::{
    style_context_add_provider_for_display, Application, ApplicationWindow, Box as GtkBox,
    Button, CssProvider, Orientation, STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use niri_ipc::state::EventStreamStatePart;

use config::{Config, Position};
use taskbar::Taskbar;

pub const GEAR_SIZE: i32 = 36;

const CSS: &str = "
.bar-window {
    background: transparent;
}
button.taskbar-button {
    padding: 2px 6px;
    border-radius: 6px;
    border: none;
    background: transparent;
    min-height: 0;
    min-width: 0;
}
button.taskbar-button:hover {
    background: rgba(255, 255, 255, 0.10);
}
button.taskbar-button.focused {
    background: rgba(255, 255, 255, 0.18);
}
button.taskbar-button.urgent {
    background: rgba(220, 80, 80, 0.45);
}
count, .count {
    font-size: 10px;
    opacity: 0.85;
}
.window-menu {
    min-width: 240px;
}
button.window-row-select, button.window-row-close {
    padding: 0;
    border: none;
    border-radius: 4px;
    background: transparent;
}
button.window-row-select:hover, button.window-row-close:hover {
    background: rgba(255, 255, 255, 0.10);
}
";

pub struct Bar {
    window: ApplicationWindow,
    root: GtkBox,
    taskbar: RefCell<Taskbar>,
    config: RefCell<Config>,
}

impl Bar {
    fn apply_config(&self, config: Config) {
        *self.config.borrow_mut() = config.clone();
        let window = &self.window;
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, false);
        }
        let thickness = config.thickness as i32;
        match config.position {
            Position::Top => {
                window.set_anchor(Edge::Top, true);
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Right, true);
                window.set_default_size(0, thickness);
            }
            Position::Bottom => {
                window.set_anchor(Edge::Bottom, true);
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Right, true);
                window.set_default_size(0, thickness);
            }
            Position::Left => {
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Top, true);
                window.set_anchor(Edge::Bottom, true);
                window.set_default_size(thickness, 0);
            }
            Position::Right => {
                window.set_anchor(Edge::Right, true);
                window.set_anchor(Edge::Top, true);
                window.set_anchor(Edge::Bottom, true);
                window.set_default_size(thickness, 0);
            }
        }
        let orientation = if config.is_horizontal() {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        };
        self.root.set_orientation(orientation);
        self.taskbar.borrow_mut().set_config(config);
    }
}

fn main() -> glib::ExitCode {
    let config = Config::load();
    let app = Application::builder()
        .application_id("dev.barthing.Barthing")
        .build();
    app.connect_activate(move |app| activate(app, config.clone()));
    app.run()
}

fn activate(app: &Application, config: Config) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    install_css();

    let window = ApplicationWindow::new(app);
    window.init_layer_shell();
    window.set_layer(Layer::Top);
    window.set_namespace(Some("barthing"));
    window.set_keyboard_mode(KeyboardMode::None);
    window.auto_exclusive_zone_enable();
    window.add_css_class("bar-window");

    let orientation = if config.is_horizontal() {
        Orientation::Horizontal
    } else {
        Orientation::Vertical
    };
    let root = GtkBox::new(orientation, 0);
    window.set_child(Some(&root));

    let ws_accum = std::cell::Cell::new(0.0f64);
    let ws_scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    ws_scroll.connect_scroll(move |_, _dx, dy| {
        if dy == 0.0 {
            return glib::Propagation::Proceed;
        }
        let mut total = ws_accum.get() + dy;
        total = total.clamp(-4.0, 4.0);
        while total >= 1.0 {
            niri::send(niri::Cmd::WorkspaceDown);
            total -= 1.0;
        }
        while total <= -1.0 {
            niri::send(niri::Cmd::WorkspaceUp);
            total += 1.0;
        }
        ws_accum.set(total);
        glib::Propagation::Stop
    });
    root.add_controller(ws_scroll);

    let gear_image = gtk::Image::from_icon_name("preferences-system");
    gear_image.set_pixel_size(22);
    let gear = Button::new();
    gear.set_child(Some(&gear_image));
    gear.set_size_request(GEAR_SIZE, GEAR_SIZE);
    gear.set_tooltip_text(Some("Settings"));
    gear.add_css_class("taskbar-button");
    gear.set_focusable(false);
    root.append(&gear);

    let taskbar = Taskbar::new(&root, config.clone());
    let bar = Rc::new(Bar {
        window,
        root,
        taskbar: RefCell::new(taskbar),
        config: RefCell::new(config.clone()),
    });

    let bar_for_gear = Rc::clone(&bar);
    let app_for_settings = app.clone();
    gear.connect_clicked(move |_| {
        if std::env::var_os("BARTHING_DEBUG").is_some() {
            eprintln!("CLICK gear");
        }
        settings::open(&app_for_settings, &bar_for_gear);
    });

    let (tx, rx) = async_channel::unbounded();
    niri::spawn_event_thread(tx);

    let mut state = niri_ipc::state::EventStreamState::default();
    let bar_for_events = Rc::clone(&bar);
    glib::MainContext::default().spawn_local(async move {
        while let Ok(event) = rx.recv().await {
            state.apply(event);
            bar_for_events.taskbar.borrow_mut().sync(&state);
        }
    });

    bar.apply_config(config);
    bar.window.present();

    if std::env::var_os("BARTHING_DEBUG").is_some() {
        let bar_debug = Rc::clone(&bar);
        glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
            for (key, rect) in bar_debug.taskbar.borrow().debug_layout(&bar_debug.window) {
                eprintln!("LAYOUT {key} -> {rect}");
            }
        });
        let bar_debug = Rc::clone(&bar);
        let app_debug = app.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
            eprintln!("DEBUG opening settings");
            settings::open(&app_debug, &bar_debug);
        });
    }
}

fn install_css() {
    let provider = CssProvider::new();
    provider.load_from_string(CSS);
    if let Some(display) = gtk::gdk::Display::default() {
        style_context_add_provider_for_display(
            &display,
            &provider,
            STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
