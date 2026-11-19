//! Per-surface backdrop blur on GTK's existing Wayland connection.
use gtk::{cairo, gdk, glib, prelude::*};
use std::{cell::RefCell, ffi::c_void, rc::Rc, time::Duration};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    backend::{Backend, ObjectId},
    delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_compositor::WlCompositor, wl_region::WlRegion, wl_registry, wl_surface::WlSurface,
    },
};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::{self as manager, ExtBackgroundEffectManagerV1 as Manager},
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1 as Effect,
};

// GDK 3's Wayland interop functions return borrowed libwayland pointers.
#[link(name = "gdk-3")]
unsafe extern "C" {
    fn gdk_wayland_display_get_wl_display(display: *mut gdk::ffi::GdkDisplay) -> *mut c_void;
    fn gdk_wayland_display_get_wl_compositor(display: *mut gdk::ffi::GdkDisplay) -> *mut c_void;
    fn gdk_wayland_window_get_wl_surface(window: *mut gdk::ffi::GdkWindow) -> *mut c_void;
    fn gdk_wayland_display_query_registry(
        display: *mut gdk::ffi::GdkDisplay,
        name: *const std::ffi::c_char,
    ) -> i32;
}

#[derive(Default)]
struct Capabilities {
    blur: bool,
}

impl Dispatch<Manager, ()> for Capabilities {
    fn event(
        state: &mut Self,
        _: &Manager,
        event: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let manager::Event::Capabilities { flags } = event {
            // Test just the known bit, including when newer flags accompany it.
            state.blur = u32::from(flags) & manager::Capability::Blur.bits() != 0;
        }
    }
}

delegate_noop!(Capabilities: ignore Effect);
delegate_noop!(Capabilities: ignore WlRegion);

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Capabilities {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

struct Events {
    queue: EventQueue<Capabilities>,
    capabilities: Capabilities,
}

impl Events {
    fn dispatch(&mut self) {
        if self.queue.dispatch_pending(&mut self.capabilities).is_err() {
            self.capabilities.blur = false;
        }
    }
}

pub struct Blur {
    events: Rc<RefCell<Events>>,
    watch: Option<glib::SourceId>,
    manager: Manager,
    effect: Effect,
    compositor: WlCompositor,
    window: gdk::Window,
    surface_ptr: *mut c_void,
    previous: Option<cairo::Region>,
}

impl Blur {
    /// Attach once per mapped GTK toplevel. Drop the effect on unmap.
    pub fn attach(widget: &gtk::Window) -> Option<Self> {
        let window = widget.window()?;
        let display = window.display();
        let display_ptr = display.as_ptr();
        // The picker enables only GDK's Wayland backend. These handles belong to
        // the same display. The retained GdkWindow keeps the GdkDisplay alive
        // until after our guest backend and its private queue are released.
        let (native, compositor_ptr, surface_ptr) = unsafe {
            if gdk_wayland_display_query_registry(
                display_ptr,
                c"ext_background_effect_manager_v1".as_ptr(),
            ) == 0
            {
                return None;
            }
            (
                gdk_wayland_display_get_wl_display(display_ptr),
                gdk_wayland_display_get_wl_compositor(display_ptr),
                gdk_wayland_window_get_wl_surface(window.as_ptr()),
            )
        };
        if native.is_null() || compositor_ptr.is_null() || surface_ptr.is_null() {
            return None;
        }
        // Guest mode borrows the connection; GDK alone owns and disconnects it.
        // The surface proxy is borrowed only to create the extension, never destroyed.
        let (conn, compositor, surface) = unsafe {
            let conn = Connection::from_backend(Backend::from_foreign_display(native.cast()));
            let compositor = WlCompositor::from_id(
                &conn,
                ObjectId::from_ptr(WlCompositor::interface(), compositor_ptr.cast()).ok()?,
            )
            .ok()?;
            let surface = WlSurface::from_id(
                &conn,
                ObjectId::from_ptr(WlSurface::interface(), surface_ptr.cast()).ok()?,
            )
            .ok()?;
            (conn, compositor, surface)
        };
        let (globals, mut queue) = registry_queue_init(&conn).ok()?;
        let qh = queue.handle();
        let manager = globals.bind::<Manager, _, _>(&qh, 1..=1, ()).ok();
        globals.destroy();
        let manager = manager?;
        let mut capabilities = Capabilities::default();
        queue.roundtrip(&mut capabilities).ok()?;
        let effect = manager.get_background_effect(&surface, &qh, ());
        let events = Rc::new(RefCell::new(Events {
            queue,
            capabilities,
        }));
        let pending = Rc::clone(&events);
        let weak_widget = widget.downgrade();
        // Never attach a second fd watch to GDK's Wayland socket: a level-triggered
        // watch that doesn't read the fd can starve GDK's frame clock / event loop.
        // GDK reads the shared socket; we just drain our private queue periodically
        // (and on every draw in `available()`).
        let watch = glib::timeout_add_local(Duration::from_millis(250), move || {
            let mut events = pending.borrow_mut();
            let previous = events.capabilities.blur;
            events.dispatch();
            if previous != events.capabilities.blur
                && let Some(widget) = weak_widget.upgrade()
            {
                widget.queue_draw();
            }
            glib::ControlFlow::Continue
        });
        Some(Self {
            events,
            watch: Some(watch),
            manager,
            effect,
            compositor,
            window,
            surface_ptr,
            previous: None,
        })
    }

    pub fn available(&self) -> bool {
        let mut events = self.events.borrow_mut();
        events.dispatch();
        events.capabilities.blur && self.current_surface()
    }

    fn current_surface(&self) -> bool {
        // GTK may recreate its wl_surface across hide/show. Never set state on
        // an effect whose wl_surface has already been destroyed.
        !self.window.is_destroyed()
            && unsafe {
                gdk_wayland_window_get_wl_surface(self.window.as_ptr()) == self.surface_ptr
            }
    }

    /// Rectangles are in logical surface coordinates. GTK commits the region
    /// together with the frame whose geometry produced it.
    pub fn update(&mut self, region: &cairo::Region) {
        if self.previous.as_ref() == Some(region) || !self.available() {
            return;
        }
        if region.is_empty() {
            self.effect.set_blur_region(None);
        } else {
            let native = self
                .compositor
                .create_region(&self.events.borrow().queue.handle(), ());
            for i in 0..region.num_rectangles() {
                let rect = region.rectangle(i);
                native.add(rect.x(), rect.y(), rect.width(), rect.height());
            }
            self.effect.set_blur_region(Some(&native));
            native.destroy(); // set_blur_region copies the server-side region
        }
        self.previous = Some(region.copy());
    }
}

impl Drop for Blur {
    fn drop(&mut self) {
        if let Some(watch) = self.watch.take() {
            watch.remove();
        }
        self.effect.destroy();
        self.manager.destroy();
    }
}
