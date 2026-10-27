mod config;
mod icons;
mod niri;
mod power;
mod settings;
mod taskbar;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Orientation,
    STYLE_PROVIDER_PRIORITY_APPLICATION, style_context_add_provider_for_display,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use niri_ipc::state::EventStreamStatePart;

use config::{BarLayer, Config, HoverInfo, Position};
use taskbar::Taskbar;

const BAR_THICKNESS: i32 = 32;

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
button.taskbar-button.urgent {
    background: rgba(220, 80, 80, 0.45);
}
button.taskbar-button.urgent.pulse {
    background: rgba(255, 130, 130, 0.85);
}
button.taskbar-button.inactive {
    opacity: 0.45;
}
count, .count {
    font-size: 10px;
    opacity: 0.85;
}
.window-title {
    font-size: 11px;
    min-width: 0;
}
.app-initials {
    font-size: 11px;
    font-weight: bold;
    opacity: 0.9;
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
.settings-heading {
    font-weight: bold;
    font-size: 1.05em;
}
.power-error {
    color: rgb(255, 120, 120);
    font-size: 11px;
    margin-top: 6px;
    margin-bottom: 4px;
}
";

pub struct Bar {
    window: ApplicationWindow,
    root: GtkBox,
    gear: Button,
    power: power::PowerMenu,
    taskbar: RefCell<Taskbar>,
    config: Rc<RefCell<Config>>,
    overview_open: Cell<bool>,
    css: CssProvider,
}

impl Bar {
    fn apply_config(&self, config: Config) {
        *self.config.borrow_mut() = config.clone();
        self.css.load_from_string(&build_css(&config));
        let window = &self.window;
        let layer = if config.bar_layer == BarLayer::Top || !self.overview_open.get() {
            Layer::Top
        } else {
            Layer::Bottom
        };
        window.set_layer(layer);
        for pos_class in ["pos-top", "pos-bottom", "pos-left", "pos-right"] {
            window.remove_css_class(pos_class);
        }
        window.add_css_class(match config.position {
            Position::Top => "pos-top",
            Position::Bottom => "pos-bottom",
            Position::Left => "pos-left",
            Position::Right => "pos-right",
        });
        self.apply_geometry(config.position, BAR_THICKNESS);
        window.auto_exclusive_zone_enable();
        let orientation = if config.is_horizontal() {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        };
        self.root.set_orientation(orientation);
        let gear_size = config.gear_size();
        self.gear.set_size_request(gear_size, gear_size);
        self.gear.set_visible(config.show_settings_icon);
        self.power.button.set_size_request(gear_size, gear_size);
        self.power.set_visible(config.show_power_icon);
        if config.hover_info == HoverInfo::Tooltip {
            self.gear.set_tooltip_text(Some("Settings"));
        } else {
            self.gear.set_tooltip_text(None);
        }
        self.taskbar.borrow_mut().set_config(config);
    }

    fn apply_geometry(&self, position: Position, thickness: i32) {
        let window = &self.window;
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, false);
        }
        match position {
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
    }
}

fn main() -> glib::ExitCode {
    let config = Config::load();
    let app = Application::builder()
        .application_id("dev.niri-taskbar.niri-taskbar")
        .build();
    app.connect_activate(move |app| activate(app, config.clone()));
    app.run()
}

fn activate(app: &Application, config: Config) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    let css = install_css(&build_css(&config));

    let window = ApplicationWindow::new(app);
    window.init_layer_shell();
    window.set_layer(Layer::Top);
    window.set_namespace(Some("niri-taskbar"));
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
    let gear_size = config.gear_size();
    gear.set_size_request(gear_size, gear_size);
    gear.set_visible(config.show_settings_icon);
    if config.hover_info == HoverInfo::Tooltip {
        gear.set_tooltip_text(Some("Settings"));
    }
    gear.add_css_class("taskbar-button");
    gear.set_focusable(false);
    root.append(&gear);

    let power = power::PowerMenu::new(config.position);
    power.button.set_size_request(gear_size, gear_size);
    power.set_visible(config.show_power_icon);
    root.append(&power.button);

    let config_rc = Rc::new(RefCell::new(config.clone()));
    let taskbar = Taskbar::new(&root, Rc::clone(&config_rc));
    let bar = Rc::new(Bar {
        window,
        root,
        gear: gear.clone(),
        power,
        taskbar: RefCell::new(taskbar),
        config: config_rc,
        overview_open: Cell::new(false),
        css,
    });

    let bar_for_gear = Rc::clone(&bar);
    let app_for_settings = app.clone();
    gear.connect_clicked(move |_| {
        if std::env::var_os("niri-taskbar_DEBUG").is_some() {
            eprintln!("CLICK gear");
        }
        settings::open(&app_for_settings, &bar_for_gear);
    });

    let bar_for_power = Rc::clone(&bar);
    bar.power.button.connect_clicked(move |_| {
        if std::env::var_os("niri-taskbar_DEBUG").is_some() {
            eprintln!("CLICK power");
        }
        let position = bar_for_power.config.borrow().position;
        bar_for_power.power.open(position);
    });

    let bar_for_pin = Rc::clone(&bar);
    bar.taskbar.borrow().set_config_changed(Rc::new(move || {
        let config = bar_for_pin.config.borrow().clone();
        bar_for_pin.apply_config(config);
    }));

    let (tx, rx) = async_channel::unbounded();
    niri::spawn_event_thread(tx);

    let mut state = niri_ipc::state::EventStreamState::default();
    let bar_for_events = Rc::clone(&bar);
    glib::MainContext::default().spawn_local(async move {
        while let Ok(event) = rx.recv().await {
            state.apply(event);
            bar_for_events.taskbar.borrow_mut().sync(&state);

            let open = state.overview.is_open;
            let prev = bar_for_events.overview_open.get();
            if open != prev {
                bar_for_events.overview_open.set(open);
                let zoom = bar_for_events.config.borrow().bar_layer == BarLayer::Bottom;
                if zoom {
                    bar_for_events.taskbar.borrow().close_menus();
                    bar_for_events.power.close();
                    bar_for_events
                        .window
                        .set_layer(if open { Layer::Bottom } else { Layer::Top });
                }
            }
        }
    });

    bar.apply_config(config);
    bar.window.present();

    if std::env::var_os("niri-taskbar_DEBUG").is_some() {
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

fn install_css(css: &str) -> CssProvider {
    let provider = CssProvider::new();
    provider.load_from_string(css);
    if let Some(display) = gtk::gdk::Display::default() {
        style_context_add_provider_for_display(
            &display,
            &provider,
            STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    provider
}

fn build_css(config: &Config) -> String {
    let mut css = format!(
        "{CSS}\n.bar-window {{ background: {}; }}\nbutton.taskbar-button.unfocused {{ background: {}; }}\n",
        config.bar_background, config.unfocused_background
    );
    if config.indicator_mode.has_background() {
        css.push_str(&format!(
            "button.taskbar-button.focused {{ background: {}; }}\n",
            config.focused_background
        ));
    }
    if config.indicator_mode.has_underline() {
        let color = &config.focused_background;
        css.push_str(&format!(
            ".bar-window.pos-top button.taskbar-button.focused {{ box-shadow: inset 0 -3px 0 {color}; }}\n"
        ));
        css.push_str(&format!(
            ".bar-window.pos-bottom button.taskbar-button.focused {{ box-shadow: inset 0 3px 0 {color}; }}\n"
        ));
        css.push_str(&format!(
            ".bar-window.pos-left button.taskbar-button.focused {{ box-shadow: inset -3px 0 0 {color}; }}\n"
        ));
        css.push_str(&format!(
            ".bar-window.pos-right button.taskbar-button.focused {{ box-shadow: inset 3px 0 0 {color}; }}\n"
        ));
    }
    css
}
