use std::sync::{Arc, Mutex};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop, event_created_child,
    protocol::{wl_registry, wl_seat},
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self as toplevel, ZwlrForeignToplevelHandleV1 as Handle},
    zwlr_foreign_toplevel_manager_v1::{self as manager, ZwlrForeignToplevelManagerV1 as Manager},
};

#[derive(Debug, Clone)]
pub struct Window {
    pub title: String,
    pub app_id: String,
    handle: Handle,
    session: Arc<Mutex<Session>>,
}

impl Window {
    pub fn focus(&self) -> Result<(), String> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Window connection was interrupted.")?;
        session.sync()?;
        if !session
            .state
            .windows
            .iter()
            .any(|win| win.handle == self.handle)
        {
            return Err("That window has closed.".into());
        }
        let seat = session
            .state
            .seat
            .as_ref()
            .ok_or("The input seat is unavailable.")?;
        self.handle.activate(&seat.1);
        // Ensure the request reaches the compositor before the picker and its connection exit.
        session.sync()
    }
}

#[derive(Debug)]
struct WindowInfo {
    handle: Handle,
    title: String,
    app_id: String,
    active: bool,
}

#[derive(Debug, Default)]
struct State {
    manager: Option<Manager>,
    seat: Option<(u32, wl_seat::WlSeat)>,
    windows: Vec<WindowInfo>,
}

#[derive(Debug)]
struct Session {
    queue: EventQueue<State>,
    state: State,
}

impl Session {
    fn sync(&mut self) -> Result<(), String> {
        self.queue
            .roundtrip(&mut self.state)
            .map(|_| ())
            .map_err(|err| format!("Window switcher lost its Wayland connection. {err}"))
    }
}

pub fn list() -> Result<Vec<Window>, String> {
    let connection = Connection::connect_to_env()
        .map_err(|err| format!("Couldn't connect the window switcher to Wayland. {err}"))?;
    let queue = connection.new_event_queue();
    connection.display().get_registry(&queue.handle(), ());
    let mut session = Session {
        queue,
        state: State::default(),
    };
    session.sync()?; // Discover the manager and seat, and send their bind requests.
    if session.state.manager.is_none() {
        return Err(
            "This compositor does not provide window switching (wlr-foreign-toplevel-management)."
                .into(),
        );
    }
    session.sync()?; // Receive the initial window handles and their titles.
    session.state.windows.sort_by_cached_key(|window| {
        (
            window.active,
            window.app_id.to_lowercase(),
            window.title.to_lowercase(),
        )
    });
    let session = Arc::new(Mutex::new(session));
    let guard = session
        .lock()
        .map_err(|_| "Window connection was interrupted.")?;
    Ok(guard
        .state
        .windows
        .iter()
        .map(|win| Window {
            title: if win.title.is_empty() {
                if win.app_id.is_empty() {
                    "Untitled window".into()
                } else {
                    win.app_id.clone()
                }
            } else {
                win.title.clone()
            },
            app_id: win.app_id.clone(),
            handle: win.handle.clone(),
            session: session.clone(),
        })
        .collect())
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        model: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name, interface, ..
            } if interface == Manager::interface().name && model.manager.is_none() => {
                model.manager = Some(registry.bind(name, 1, qh, ()));
            }
            wl_registry::Event::Global {
                name, interface, ..
            } if interface == wl_seat::WlSeat::interface().name && model.seat.is_none() => {
                model.seat = Some((name, registry.bind(name, 1, qh, ())));
            }
            wl_registry::Event::GlobalRemove { name }
                if model.seat.as_ref().is_some_and(|seat| seat.0 == name) =>
            {
                model.seat = None;
            }
            _ => {}
        }
    }
}

delegate_noop!(State: ignore wl_seat::WlSeat);

impl Dispatch<Manager, ()> for State {
    fn event(
        model: &mut Self,
        _: &Manager,
        event: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            manager::Event::Toplevel { toplevel } => model.windows.push(WindowInfo {
                handle: toplevel,
                title: String::new(),
                app_id: String::new(),
                active: false,
            }),
            manager::Event::Finished => model.manager = None,
            _ => {}
        }
    }
    event_created_child!(State, Manager, [manager::EVT_TOPLEVEL_OPCODE => (Handle, ())]);
}

impl Dispatch<Handle, ()> for State {
    fn event(
        model: &mut Self,
        handle: &Handle,
        event: toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(index) = model.windows.iter().position(|win| win.handle == *handle) else {
            return;
        };
        let window = &mut model.windows[index];
        match event {
            toplevel::Event::Title { title } => window.title = title,
            toplevel::Event::AppId { app_id } => window.app_id = app_id,
            toplevel::Event::State { state } => {
                window.active = state
                    .chunks_exact(4)
                    .any(|v| v == u32::from(toplevel::State::Activated).to_ne_bytes());
            }
            toplevel::Event::Closed => {
                model.windows.remove(index);
                handle.destroy();
            }
            _ => {}
        }
    }
}
