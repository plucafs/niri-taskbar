use niri_ipc::socket::Socket;
use niri_ipc::{Action, Event, Request};

pub enum Cmd {
    Focus(u64),
    Close(u64),
    WorkspaceUp,
    WorkspaceDown,
}

pub fn spawn_event_thread(tx: async_channel::Sender<Event>) {
    std::thread::spawn(move || {
        let mut socket = match Socket::connect() {
            Ok(socket) => socket,
            Err(err) => {
                eprintln!("barthing: cannot connect to niri: {err}");
                return;
            }
        };
        match socket.send(Request::EventStream) {
            Ok(Ok(niri_ipc::Response::Handled)) => {}
            other => {
                eprintln!("barthing: event stream rejected: {other:?}");
                return;
            }
        }
        let mut read_event = socket.read_events();
        loop {
            match read_event() {
                Ok(event) => {
                    if tx.try_send(event).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    eprintln!("barthing: event stream ended: {err}");
                    break;
                }
            }
        }
    });
}

pub fn send(cmd: Cmd) {
    std::thread::spawn(move || {
        let request = match cmd {
            Cmd::Focus(id) => Request::Action(Action::FocusWindow { id }),
            Cmd::Close(id) => Request::Action(Action::CloseWindow { id: Some(id) }),
            Cmd::WorkspaceUp => Request::Action(Action::FocusWorkspaceUp {}),
            Cmd::WorkspaceDown => Request::Action(Action::FocusWorkspaceDown {}),
        };
        let mut socket = match Socket::connect() {
            Ok(socket) => socket,
            Err(err) => {
                eprintln!("barthing: cannot connect to niri: {err}");
                return;
            }
        };
        match socket.send(request) {
            Ok(Ok(_)) => {}
            Ok(Err(message)) => eprintln!("barthing: niri rejected action: {message}"),
            Err(err) => eprintln!("barthing: niri action failed: {err}"),
        }
    });
}
