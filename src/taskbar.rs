use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use gtk::glib::Propagation;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, EventControllerScroll, EventControllerScrollFlags, Image, Label,
    Orientation, Popover, PositionType,
};
use niri_ipc::state::EventStreamState;
use niri_ipc::Workspace;

use crate::config::{Config, GroupBy, HoverInfo, IconPosition, MiddleClick, Position, SortMode};
use crate::icons;
use crate::niri::{self, Cmd};

#[derive(Clone)]
pub struct WinInfo {
    pub id: u64,
    pub title: String,
    pub ws_label: String,
    pub focused: bool,
    pub urgent: bool,
    pub mru: (u64, u32),
}

pub struct Entry {
    pub key: String,
    pub app_id: Option<String>,
    pub windows: Vec<WinInfo>,
}

type Entries = Rc<RefCell<Vec<Entry>>>;
type Cursor = Rc<RefCell<Option<usize>>>;

#[derive(Clone, Copy, PartialEq)]
enum MenuKind {
    Chooser,
    Context,
}

pub struct Taskbar {
    spacer_before: GtkBox,
    strip: GtkBox,
    spacer_after: GtkBox,
    filler: GtkBox,
    config: Rc<RefCell<Config>>,
    entries: Entries,
    buttons: HashMap<String, Button>,
    popovers: HashMap<String, Popover>,
    previews: HashMap<String, Popover>,
    count_labels: HashMap<String, Label>,
    cursors: HashMap<String, Cursor>,
    open_menu: Rc<RefCell<Option<(String, MenuKind)>>>,
    open_signature: Rc<RefCell<Vec<u64>>>,
    history: Rc<RefCell<VecDeque<u64>>>,
    config_changed: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    last_focused: Option<u64>,
    appearance: Vec<String>,
    strip_order: Vec<String>,
    structure: Vec<(String, Vec<u64>)>,
}

impl Taskbar {
    pub fn new(parent: &GtkBox, config: Rc<RefCell<Config>>) -> Self {
        let orientation = bar_orientation(&config.borrow());
        let spacer_before = GtkBox::new(orientation, 0);
        let strip = GtkBox::new(orientation, 6);
        strip.set_margin_start(4);
        strip.set_margin_end(4);
        strip.set_margin_top(2);
        strip.set_margin_bottom(2);
        let spacer_after = GtkBox::new(orientation, 0);
        let filler = GtkBox::new(orientation, 0);
        parent.append(&spacer_before);
        parent.append(&strip);
        parent.append(&spacer_after);
        parent.append(&filler);
        let taskbar = Self {
            spacer_before,
            strip,
            spacer_after,
            filler,
            config,
            entries: Rc::new(RefCell::new(Vec::new())),
            buttons: HashMap::new(),
            popovers: HashMap::new(),
            previews: HashMap::new(),
            count_labels: HashMap::new(),
            cursors: HashMap::new(),
            open_menu: Rc::new(RefCell::new(None)),
            open_signature: Rc::new(RefCell::new(Vec::new())),
            history: Rc::new(RefCell::new(VecDeque::new())),
            config_changed: Rc::new(RefCell::new(None)),
            last_focused: None,
            appearance: Vec::new(),
            strip_order: Vec::new(),
            structure: Vec::new(),
        };
        taskbar.apply_layout();
        taskbar
    }

    pub fn set_config(&mut self, config: Config) {
        *self.config.borrow_mut() = config;
        *self.open_menu.borrow_mut() = None;
        self.apply_layout();
        self.rebuild();
        self.update_classes();
    }

    fn apply_layout(&self) {
        let config = self.config.borrow().clone();
        let orientation = bar_orientation(&config);
        self.strip.set_orientation(orientation);
        let (before, after, filler_size) = match config.icon_position {
            IconPosition::Left => (false, true, 0),
            IconPosition::Center => (true, true, config.gear_size()),
            IconPosition::Right => (true, false, 0),
        };
        if config.is_horizontal() {
            self.spacer_before.set_hexpand(before);
            self.spacer_after.set_hexpand(after);
            self.filler.set_size_request(filler_size, 1);
            self.strip.set_valign(Align::Center);
        } else {
            self.spacer_before.set_vexpand(before);
            self.spacer_after.set_vexpand(after);
            self.filler.set_size_request(1, filler_size);
            self.strip.set_halign(Align::Center);
        }
    }

    pub fn sync(&mut self, state: &EventStreamState) {
        let config = self.config.borrow().clone();
        let mut entries = build_entries(state, config.group_by);
        for app_id in &config.pinned_apps {
            if !entries
                .iter()
                .any(|entry| entry.app_id.as_deref() == Some(app_id.as_str()))
            {
                entries.push(Entry {
                    key: app_id.clone(),
                    app_id: Some(app_id.clone()),
                    windows: Vec::new(),
                });
            }
        }
        *self.entries.borrow_mut() = entries;
        let structure = structure_signature(&self.entries.borrow());
        if structure != self.structure {
            self.structure = structure;
            self.sync_buttons();
        }
        self.update_classes();
        self.refresh_open_menu();
        self.update_history();
    }

    pub fn set_config_changed(&self, callback: Rc<dyn Fn()>) {
        *self.config_changed.borrow_mut() = Some(callback);
    }

    pub fn close_menus(&self) {
        *self.open_menu.borrow_mut() = None;
        for popover in self.popovers.values() {
            popover.popdown();
        }
        for preview in self.previews.values() {
            preview.popdown();
        }
    }

    pub fn ordered_entries(&self) -> Vec<(String, String)> {
        self.strip_order
            .iter()
            .map(|key| {
                let label = self
                    .entries
                    .borrow()
                    .iter()
                    .find(|entry| &entry.key == key)
                    .map(|entry| {
                        entry
                            .app_id
                            .clone()
                            .unwrap_or_else(|| "(untitled)".to_string())
                    })
                    .unwrap_or_else(|| key.clone());
                (key.clone(), label)
            })
            .collect()
    }

    fn sync_buttons(&mut self) {
        let config = self.config.borrow().clone();
        let entries = self.entries.borrow();
        let keep: HashSet<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();

        let open_gone = self
            .open_menu
            .borrow()
            .as_ref()
            .is_some_and(|(key, _)| !keep.contains(key.as_str()));
        if open_gone {
            *self.open_menu.borrow_mut() = None;
        }

        let stale: Vec<String> = self
            .buttons
            .keys()
            .filter(|key| !keep.contains(key.as_str()))
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.buttons.remove(&key) {
                self.strip.remove(&button);
            }
            if let Some(popover) = self.popovers.remove(&key) {
                popover.unparent();
            }
            if let Some(preview) = self.previews.remove(&key) {
                preview.unparent();
            }
            self.count_labels.remove(&key);
            self.cursors.remove(&key);
        }

        self.appearance.retain(|key| keep.contains(key.as_str()));
        self.strip_order.retain(|key| keep.contains(key.as_str()));
        for entry in entries.iter() {
            if !self.appearance.contains(&entry.key) {
                self.appearance.push(entry.key.clone());
            }
        }

        for entry in entries.iter() {
            if !self.buttons.contains_key(&entry.key) {
                let (button, popover, preview, count, cursor) = self.build_button(entry);
                self.strip.append(&button);
                self.strip_order.push(entry.key.clone());
                self.buttons.insert(entry.key.clone(), button);
                self.popovers.insert(entry.key.clone(), popover);
                if let Some(preview) = preview {
                    self.previews.insert(entry.key.clone(), preview);
                }
                self.count_labels.insert(entry.key.clone(), count);
                self.cursors.insert(entry.key.clone(), cursor);
            }
            let windows = entry.windows.len();
            if let Some(count) = self.count_labels.get(&entry.key) {
                count.set_visible(windows > 1);
                if windows > 1 {
                    count.set_text(&windows.to_string());
                }
            }
            if let Some(button) = self.buttons.get(&entry.key) {
                set_class(button, "inactive", windows == 0);
            }
        }

        let desired = desired_order(&config, &entries, &self.appearance);
        drop(entries);
        self.reorder_strip(&desired);
    }

    fn reorder_strip(&mut self, desired: &[String]) {
        if self.strip_order.as_slice() == desired {
            return;
        }
        for key in desired {
            if let Some(button) = self.buttons.get(key) {
                self.strip.remove(button);
                self.strip.append(button);
            }
        }
        self.strip_order = desired.to_vec();
    }

    fn update_history(&mut self) {
        let focused = self
            .entries
            .borrow()
            .iter()
            .flat_map(|entry| entry.windows.iter())
            .find(|win| win.focused)
            .map(|win| win.id);
        if focused != self.last_focused {
            if let Some(previous) = self.last_focused {
                let mut history = self.history.borrow_mut();
                if history.front() != Some(&previous) {
                    history.push_front(previous);
                    history.truncate(16);
                }
            }
            self.last_focused = focused;
        }
    }

    fn refresh_open_menu(&self) {
        let (key, kind) = {
            let open = self.open_menu.borrow();
            let Some((key, kind)) = open.as_ref() else {
                return;
            };
            (key.clone(), *kind)
        };
        let entries = self.entries.borrow();
        let Some(entry) = entries.iter().find(|entry| entry.key == key) else {
            if let Some(popover) = self.popovers.get(&key) {
                popover.popdown();
            }
            *self.open_menu.borrow_mut() = None;
            return;
        };
        let ids: Vec<u64> = entry.windows.iter().map(|win| win.id).collect();
        if ids == *self.open_signature.borrow() {
            return;
        }
        *self.open_signature.borrow_mut() = ids;
        let (Some(popover), Some(cursor)) = (self.popovers.get(&key), self.cursors.get(&key)) else {
            return;
        };
        let content = match kind {
            MenuKind::Chooser => chooser_content(entry, popover, cursor),
            MenuKind::Context => context_content(
                entry,
                popover,
                cursor,
                &self.config,
                &self.config_changed,
            ),
        };
        popover.set_child(Some(&content));
    }

    fn rebuild(&mut self) {
        for (_, popover) in self.popovers.drain() {
            popover.unparent();
        }
        for (_, preview) in self.previews.drain() {
            preview.unparent();
        }
        while let Some(child) = self.strip.first_child() {
            self.strip.remove(&child);
        }
        self.buttons.clear();
        self.count_labels.clear();
        self.cursors.clear();
        self.strip_order.clear();

        let config = self.config.borrow().clone();
        {
            let mut entries = self.entries.borrow_mut();
            entries.retain(|entry| {
                !entry.windows.is_empty()
                    || entry
                        .app_id
                        .as_deref()
                        .is_some_and(|app| config.pinned_apps.iter().any(|p| p == app))
            });
            for app_id in &config.pinned_apps {
                if !entries
                    .iter()
                    .any(|entry| entry.app_id.as_deref() == Some(app_id.as_str()))
                {
                    entries.push(Entry {
                        key: app_id.clone(),
                        app_id: Some(app_id.clone()),
                        windows: Vec::new(),
                    });
                }
            }
            self.appearance
                .retain(|key| entries.iter().any(|entry| entry.key == *key));
            for entry in entries.iter() {
                if !self.appearance.contains(&entry.key) {
                    self.appearance.push(entry.key.clone());
                }
                let (button, popover, preview, count, cursor) = self.build_button(entry);
                self.strip.append(&button);
                self.strip_order.push(entry.key.clone());
                self.buttons.insert(entry.key.clone(), button);
                self.popovers.insert(entry.key.clone(), popover);
                if let Some(preview) = preview {
                    self.previews.insert(entry.key.clone(), preview);
                }
                self.count_labels.insert(entry.key.clone(), count);
                self.cursors.insert(entry.key.clone(), cursor);
            }
        }
        let entries = self.entries.borrow();
        let desired = desired_order(&config, &entries, &self.appearance);
        drop(entries);
        self.reorder_strip(&desired);
    }

    fn build_button(&self, entry: &Entry) -> (Button, Popover, Option<Popover>, Label, Cursor) {
        let config = self.config.borrow().clone();
        let button = Button::new();
        button.add_css_class("taskbar-button");
        button.set_focusable(false);
        if entry.windows.is_empty() {
            button.add_css_class("inactive");
        }

        let content = GtkBox::new(bar_orientation(&config), 4);
        content.set_halign(Align::Center);
        content.set_valign(Align::Center);
        content.append(&icons::image_for_app(
            entry.app_id.as_deref(),
            config.icon_size as i32,
            config.icon_fallback,
        ));
        let count = Label::new(Some(&entry.windows.len().to_string()));
        count.add_css_class("count");
        count.set_visible(entry.windows.len() > 1);
        content.append(&count);
        button.set_child(Some(&content));
        if config.hover_info == HoverInfo::Tooltip {
            button.set_tooltip_text(Some(&tooltip(entry)));
        }

        let popover = Popover::new();
        popover.set_parent(&button);
        popover.set_position(popover_position(config.position));
        let preview = if config.hover_info == HoverInfo::Preview && !entry.windows.is_empty() {
            let preview = Popover::new();
            preview.set_parent(&button);
            preview.set_position(popover_position(config.position));
            preview.set_autohide(false);
            let entries = Rc::clone(&self.entries);
            let key = entry.key.clone();
            let preview_weak = preview.downgrade();
            let motion = gtk::EventControllerMotion::new();
            motion.connect_enter(move |_, _, _| {
                let Some(preview) = preview_weak.upgrade() else {
                    return;
                };
                if preview.is_visible() {
                    return;
                }
                let entries = entries.borrow();
                let Some(entry) = entries.iter().find(|entry| entry.key == key) else {
                    return;
                };
                if entry.windows.is_empty() {
                    return;
                }
                let list = preview_content(entry);
                preview.set_child(Some(&list));
                preview.popup();
            });
            let preview_weak = preview.downgrade();
            motion.connect_leave(move |_| {
                if let Some(preview) = preview_weak.upgrade() {
                    preview.popdown();
                }
            });
            button.add_controller(motion);
            Some(preview)
        } else {
            None
        };
        let cursor: Cursor = Rc::new(RefCell::new(None));

        let key_closed = entry.key.clone();
        let open_menu_closed = Rc::clone(&self.open_menu);
        popover.connect_closed(move |_| {
            let mut open = open_menu_closed.borrow_mut();
            if open.as_ref().is_some_and(|(key, _)| *key == key_closed) {
                *open = None;
            }
        });

        let key = entry.key.clone();
        let entries = Rc::clone(&self.entries);
        let open_menu = Rc::clone(&self.open_menu);
        let open_signature = Rc::clone(&self.open_signature);
        let history = Rc::clone(&self.history);
        let cursor_click = Rc::clone(&cursor);
        let popover_weak = popover.downgrade();
        let preview_click = preview.as_ref().map(|p| p.downgrade());
        button.connect_clicked(move |_| {
            if std::env::var_os("niri-taskbar_DEBUG").is_some() {
                eprintln!("CLICK left key={key:?}");
            }
            if let Some(preview) = preview_click.as_ref().and_then(|w| w.upgrade()) {
                preview.popdown();
            }
            let Some(popover) = popover_weak.upgrade() else {
                return;
            };
            let entries = entries.borrow();
            let Some(entry) = entries.iter().find(|entry| entry.key == key) else {
                return;
            };
            if entry.windows.is_empty() {
                if let Some(app_id) = entry.app_id.clone() {
                    niri::send(Cmd::Spawn(vec![app_id]));
                }
                return;
            }
            if entry.windows.len() == 1 {
                let win = &entry.windows[0];
                if win.focused {
                    let mut history = history.borrow_mut();
                    while let Some(&previous) = history.front() {
                        if entries
                            .iter()
                            .any(|entry| entry.windows.iter().any(|win| win.id == previous))
                        {
                            niri::send(Cmd::Focus(previous));
                            break;
                        }
                        history.pop_front();
                    }
                } else {
                    niri::send(Cmd::Focus(win.id));
                }
                return;
            }
            *open_signature.borrow_mut() = entry.windows.iter().map(|win| win.id).collect();
            *open_menu.borrow_mut() = Some((key.clone(), MenuKind::Chooser));
            let content = chooser_content(entry, &popover, &cursor_click);
            open_popover(&popover, content);
        });

        let key = entry.key.clone();
        let entries = Rc::clone(&self.entries);
        let open_menu = Rc::clone(&self.open_menu);
        let open_signature = Rc::clone(&self.open_signature);
        let cursor_open = Rc::clone(&cursor);
        let config_ctx = Rc::clone(&self.config);
        let config_changed_ctx = Rc::clone(&self.config_changed);
        let popover_weak = popover.downgrade();
        let preview_context = preview.as_ref().map(|p| p.downgrade());
        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        right_click.connect_released(move |_, _, _, _| {
            if std::env::var_os("niri-taskbar_DEBUG").is_some() {
                eprintln!("CLICK right key={key:?}");
            }
            if let Some(preview) = preview_context.as_ref().and_then(|w| w.upgrade()) {
                preview.popdown();
            }
            let Some(popover) = popover_weak.upgrade() else {
                return;
            };
            let entries = entries.borrow();
            let Some(entry) = entries.iter().find(|entry| entry.key == key) else {
                return;
            };
            *open_signature.borrow_mut() = entry.windows.iter().map(|win| win.id).collect();
            *open_menu.borrow_mut() = Some((key.clone(), MenuKind::Context));
            let content = context_content(
                entry,
                &popover,
                &cursor_open,
                &config_ctx,
                &config_changed_ctx,
            );
            open_popover(&popover, content);
        });
        button.add_controller(right_click);

        let key = entry.key.clone();
        let entries = Rc::clone(&self.entries);
        let middle_action = config.middle_click;
        let middle_click = gtk::GestureClick::new();
        middle_click.set_button(2);
        middle_click.connect_pressed(move |_, _, _, _| {
            if std::env::var_os("niri-taskbar_DEBUG").is_some() {
                eprintln!("CLICK middle key={key:?}");
            }
            let entries = entries.borrow();
            let Some(entry) = entries.iter().find(|entry| entry.key == key) else {
                return;
            };
            match middle_action {
                MiddleClick::Close => {
                    if let Some(id) = mru_window_id(entry) {
                        niri::send(Cmd::Close(id));
                    }
                }
                MiddleClick::NewWindow => {
                    if let Some(app_id) = entry.app_id.clone() {
                        niri::send(Cmd::Spawn(vec![app_id]));
                    }
                }
                MiddleClick::Focus => {
                    if let Some(id) = mru_window_id(entry) {
                        niri::send(Cmd::Focus(id));
                    }
                }
            }
        });
        button.add_controller(middle_click);

        let key = entry.key.clone();
        let entries = Rc::clone(&self.entries);
        let cursor_scroll = cursor.clone();
        let accum = Rc::new(RefCell::new(0.0f64));
        let scroll = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll(move |_, _dx, dy| {
            let entries = entries.borrow();
            let Some(entry) = entries.iter().find(|entry| entry.key == key) else {
                return Propagation::Proceed;
            };
            if entry.windows.len() < 2 {
                return Propagation::Proceed;
            }
            let mut total = accum.borrow_mut();
            *total = (*total + dy).clamp(-4.0, 4.0);
            while *total >= 1.0 {
                cycle_window(entry, 1, &cursor_scroll);
                *total -= 1.0;
            }
            while *total <= -1.0 {
                cycle_window(entry, -1, &cursor_scroll);
                *total += 1.0;
            }
            Propagation::Stop
        });
        button.add_controller(scroll);

        (button, popover, preview, count, cursor)
    }

    fn update_classes(&self) {
        let entries = self.entries.borrow();
        for entry in entries.iter() {
            let Some(button) = self.buttons.get(&entry.key) else {
                continue;
            };
            set_class(button, "focused", entry.windows.iter().any(|win| win.focused));
            set_class(button, "urgent", entry.windows.iter().any(|win| win.urgent));
            if self.config.borrow().hover_info == HoverInfo::Tooltip {
                button.set_tooltip_text(Some(&tooltip(entry)));
            } else {
                button.set_tooltip_text(None);
            }
        }
    }

    #[allow(deprecated)]
    pub fn debug_layout<W: IsA<gtk::Widget>>(&self, window: &W) -> Vec<(String, String)> {
        self.buttons
            .iter()
            .map(|(key, button)| {
                let point = button
                    .compute_point(window, &gtk::graphene::Point::zero())
                    .unwrap_or_default();
                let size = button.allocation();
                (
                    key.clone(),
                    format!(
                        "x={:.0} y={:.0} w={} h={}",
                        point.x(),
                        point.y(),
                        size.width(),
                        size.height()
                    ),
                )
            })
            .collect()
    }
}

fn structure_signature(entries: &[Entry]) -> Vec<(String, Vec<u64>)> {
    let mut signature: Vec<(String, Vec<u64>)> = entries
        .iter()
        .map(|entry| {
            (
                entry.key.clone(),
                entry.windows.iter().map(|win| win.id).collect(),
            )
        })
        .collect();
    signature.sort();
    signature
}

fn desired_order(config: &Config, entries: &[Entry], appearance: &[String]) -> Vec<String> {
    let alive: Vec<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();
    let mut keys: Vec<String> = Vec::new();
    for pinned in &config.pinned_apps {
        if alive.contains(&pinned.as_str()) && !keys.contains(pinned) {
            keys.push(pinned.clone());
        }
    }
    let rest: Vec<String> = match config.sort_mode {
        SortMode::LastOpened => appearance
            .iter()
            .filter(|key| alive.contains(&key.as_str()))
            .cloned()
            .collect(),
        SortMode::Alphabetical => {
            let mut keys: Vec<String> = alive.iter().map(|key| key.to_string()).collect();
            keys.sort_by(|a, b| {
                if a.is_empty() {
                    std::cmp::Ordering::Greater
                } else if b.is_empty() {
                    std::cmp::Ordering::Less
                } else {
                    a.to_lowercase().cmp(&b.to_lowercase())
                }
            });
            keys
        }
        SortMode::Custom => {
            let mut keys: Vec<String> = Vec::new();
            for key in &config.custom_order {
                if alive.contains(&key.as_str()) && !keys.contains(key) {
                    keys.push(key.clone());
                }
            }
            for key in appearance {
                if alive.contains(&key.as_str()) && !keys.contains(key) {
                    keys.push(key.clone());
                }
            }
            keys
        }
    };
    for key in rest {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

fn cycle_window(entry: &Entry, step: i32, cursor: &Cursor) {
    let windows = entry.windows.len();
    if windows == 0 {
        return;
    }
    let mut cursor = cursor.borrow_mut();
    let current = match *cursor {
        Some(index) if index < windows => index,
        _ => entry
            .windows
            .iter()
            .position(|win| win.focused)
            .or_else(|| {
                entry
                    .windows
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, win)| win.mru)
                    .map(|(index, _)| index)
            })
            .unwrap_or(0),
    };
    let next = (current as i32 + step).rem_euclid(windows as i32) as usize;
    *cursor = Some(next);
    niri::send(Cmd::Focus(entry.windows[next].id));
}

fn open_popover(popover: &Popover, content: GtkBox) {
    popover.set_child(Some(&content));
    popover.popup();
}

fn chooser_content(entry: &Entry, popover: &Popover, cursor: &Cursor) -> GtkBox {
    let list = GtkBox::new(Orientation::Vertical, 2);
    list.add_css_class("window-menu");
    for (index, win) in entry.windows.iter().enumerate() {
        let row = GtkBox::new(Orientation::Horizontal, 0);

        let select = Button::new();
        select.add_css_class("window-row-select");
        select.set_focusable(false);
        select.set_hexpand(true);

        let text = GtkBox::new(Orientation::Horizontal, 10);
        text.set_margin_top(3);
        text.set_margin_bottom(3);
        text.set_margin_start(6);
        text.set_margin_end(6);

        let title = Label::new(Some(&win.title));
        title.set_halign(Align::Start);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(42);
        text.append(&title);

        if !win.ws_label.is_empty() {
            let ws = Label::new(Some(&win.ws_label));
            ws.add_css_class("dim-label");
            text.append(&ws);
        }
        select.set_child(Some(&text));

        let id = win.id;
        let cursor_click = Rc::clone(cursor);
        let popover_weak = popover.downgrade();
        select.connect_clicked(move |_| {
            *cursor_click.borrow_mut() = Some(index);
            niri::send(Cmd::Focus(id));
            if let Some(popover) = popover_weak.upgrade() {
                popover.popdown();
            }
        });

        let close = Button::new();
        close.add_css_class("window-row-close");
        close.set_focusable(false);
        close.set_tooltip_text(Some("Close window"));
        let x_image = Image::from_icon_name("window-close-symbolic");
        x_image.set_pixel_size(12);
        close.set_child(Some(&x_image));
        close.connect_clicked(move |_| {
            niri::send(Cmd::Close(id));
        });

        row.append(&select);
        row.append(&close);
        list.append(&row);
    }
    list
}

fn context_content(
    entry: &Entry,
    popover: &Popover,
    cursor: &Cursor,
    config: &Rc<RefCell<Config>>,
    config_changed: &Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) -> GtkBox {
    let list = GtkBox::new(Orientation::Vertical, 2);
    list.add_css_class("window-menu");

    let open = menu_item("Open");
    let open_id = mru_window_id(entry);
    let open_app = if entry.windows.is_empty() {
        entry.app_id.clone()
    } else {
        None
    };
    let open_index = entry.windows.iter().position(|win| Some(win.id) == open_id);
    let cursor_open = Rc::clone(cursor);
    let popover_weak = popover.downgrade();
    open.connect_clicked(move |_| {
        if let Some(id) = open_id {
            if let Some(index) = open_index {
                *cursor_open.borrow_mut() = Some(index);
            }
            niri::send(Cmd::Focus(id));
        } else if let Some(app_id) = open_app.clone() {
            niri::send(Cmd::Spawn(vec![app_id]));
        }
        if let Some(popover) = popover_weak.upgrade() {
            popover.popdown();
        }
    });
    list.append(&open);

    if let Some(app_id) = entry.app_id.clone() {
        let pinned = config.borrow().pinned_apps.contains(&app_id);
        let pin = menu_item(if pinned { "Unpin" } else { "Pin" });
        let config = Rc::clone(config);
        let config_changed = Rc::clone(config_changed);
        let popover_weak = popover.downgrade();
        pin.connect_clicked(move |_| {
            if let Some(popover) = popover_weak.upgrade() {
                popover.popdown();
            }
            {
                let mut config = config.borrow_mut();
                if let Some(pos) = config.pinned_apps.iter().position(|p| *p == app_id) {
                    config.pinned_apps.remove(pos);
                } else {
                    config.pinned_apps.push(app_id.clone());
                }
                config.save();
            }
            if let Some(callback) = config_changed.borrow().as_ref() {
                callback();
            }
        });
        list.append(&pin);
    }

    let close = menu_item(if entry.windows.len() > 1 {
        "Close all"
    } else {
        "Close"
    });
    close.set_sensitive(!entry.windows.is_empty());
    let close_ids: Vec<u64> = entry.windows.iter().map(|win| win.id).collect();
    let popover_weak = popover.downgrade();
    close.connect_clicked(move |_| {
        for id in &close_ids {
            niri::send(Cmd::Close(*id));
        }
        if let Some(popover) = popover_weak.upgrade() {
            popover.popdown();
        }
    });
    list.append(&close);

    list
}

fn menu_item(label: &str) -> Button {
    let item = Button::new();
    item.add_css_class("window-row-select");
    item.set_focusable(false);
    let text = Label::new(Some(label));
    text.set_halign(Align::Start);
    text.set_margin_top(3);
    text.set_margin_bottom(3);
    text.set_margin_start(6);
    text.set_margin_end(6);
    item.set_child(Some(&text));
    item
}

fn mru_window_id(entry: &Entry) -> Option<u64> {
    entry
        .windows
        .iter()
        .max_by_key(|win| win.mru)
        .map(|win| win.id)
}

fn bar_orientation(config: &Config) -> Orientation {
    if config.is_horizontal() {
        Orientation::Horizontal
    } else {
        Orientation::Vertical
    }
}

fn popover_position(position: Position) -> PositionType {
    match position {
        Position::Top => PositionType::Bottom,
        Position::Bottom => PositionType::Top,
        Position::Left => PositionType::Right,
        Position::Right => PositionType::Left,
    }
}

fn build_entries(state: &EventStreamState, group_by: GroupBy) -> Vec<Entry> {
    let workspaces: &HashMap<u64, Workspace> = &state.workspaces.workspaces;
    let mut entries: Vec<Entry> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();

    for window in state.windows.windows.values() {
        let title = window
            .title
            .clone()
            .or_else(|| window.app_id.clone())
            .unwrap_or_else(|| "(untitled)".to_string());
        let win = WinInfo {
            id: window.id,
            title,
            ws_label: workspace_label(window.workspace_id, workspaces),
            focused: window.is_focused,
            urgent: window.is_urgent,
            mru: window
                .focus_timestamp
                .map(|stamp| (stamp.secs, stamp.nanos))
                .unwrap_or((0, 0)),
        };
        let key = match group_by {
            GroupBy::App => window.app_id.clone().unwrap_or_default(),
            GroupBy::Window => format!("win:{}", window.id),
        };
        match index.get(&key) {
            Some(&i) => entries[i].windows.push(win),
            None => {
                index.insert(key.clone(), entries.len());
                entries.push(Entry {
                    key,
                    app_id: window.app_id.clone(),
                    windows: vec![win],
                });
            }
        }
    }

    for entry in &mut entries {
        entry.windows.sort_by_key(|win| win.id);
    }
    entries
}

fn workspace_label(workspace_id: Option<u64>, workspaces: &HashMap<u64, Workspace>) -> String {
    let Some(id) = workspace_id else {
        return String::new();
    };
    let Some(workspace) = workspaces.get(&id) else {
        return String::new();
    };
    match &workspace.name {
        Some(name) => name.clone(),
        None => format!("WS {}", workspace.idx),
    }
}

fn tooltip(entry: &Entry) -> String {
    let name = entry
        .app_id
        .clone()
        .unwrap_or_else(|| "(untitled)".to_string());
    if entry.windows.is_empty() {
        return format!("{name}\n(not running)");
    }
    match entry.windows.len() {
        1 => format!("{name}\n{}", entry.windows[0].title),
        n => format!("{name}\n{n} windows"),
    }
}

fn preview_content(entry: &Entry) -> GtkBox {
    let list = GtkBox::new(Orientation::Vertical, 2);
    list.add_css_class("window-menu");
    for win in &entry.windows {
        let row = GtkBox::new(Orientation::Horizontal, 6);
        row.set_margin_top(3);
        row.set_margin_bottom(3);
        row.set_margin_start(6);
        row.set_margin_end(6);

        let title = Label::new(Some(&win.title));
        title.set_halign(Align::Start);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(42);
        row.append(&title);

        if !win.ws_label.is_empty() {
            let ws = Label::new(Some(&win.ws_label));
            ws.add_css_class("dim-label");
            row.append(&ws);
        }
        list.append(&row);
    }
    list
}

fn set_class(widget: &impl IsA<gtk::Widget>, class: &str, enabled: bool) {
    if enabled {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}
