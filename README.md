# niri-taskbar
Taskbar for the niri Wayland compositor

![screenshot taskbar and settings](assets/screenshot.png)

## Features
- Vertical or horizontal bar (top, bottom, left, right)
- Group windows by app or show each window separately
- Icon order: last opened, alphabetical, or custom (reorder in settings)
- Hover info: tooltip, window-list preview, or off
- Middle-click action: close window, open new window, or focus
- Pin apps via right-click menu — pinned apps stay visible (greyed out) when closed, click to launch
- Right-click menu: open, pin/unpin, close
- Scroll on the bar to switch workspaces
- Multiple windows per app: click to choose, scroll to cycle
- Icon fallback for apps without icons: default icon or first two letters
- Configurable icon size, icon spacing, and three colors (bar, focused, unfocused)
- Focused-window indicator: background, underline, or both
- Optional focused window title (horizontal bars)
- Pulsing urgent-window animation (respects reduced motion)
- Power menu: lock (swaylock), suspend, hibernate, restart, shut down (unsupported actions disabled)
- Sectioned settings page: Appearance, Windows, Behavior, Notifications, Power Menu

## Requirements
- GTK 4 (v4.22+) and gtk4-layer-shell

## Download
[Linux](https://github.com/plucafs/niri-taskbar/releases)

## Autostart
niri's default systemd service starts XDG autostart entries, so:
```sh
mkdir -p ~/.config/autostart
cp data/niri-taskbar.desktop ~/.config/autostart/
```
If you run the AppImage, edit the copied file and point `Exec` at the AppImage path instead.

## Credits
- [OpenCode](https://github.com/anomalyco/opencode)

## License
MIT