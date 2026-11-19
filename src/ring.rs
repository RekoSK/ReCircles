//! The ring uses logical pixels. Rendering and input share the same center,
//! scale and clockwise sector boundaries, including during the opening spring.
use crate::motion::{LoadingPose, Motion};
pub mod explorer;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};
use std::sync::LazyLock;

use gtk::cairo::{
    self, Context, FillRule, Format, ImageSurface, LineCap, LineJoin, LinearGradient, Matrix,
    RadialGradient, RectangleInt, Region,
};
use gtk::gdk::prelude::GdkContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::glib::{self, object::ObjectType, prelude::ObjectExt};
use gtk::pango::{
    Alignment, AttrColor, AttrList, EllipsizeMode, FontDescription, SCALE, Weight, WrapMode,
};

const OUTER: f64 = 172.0;
const INNER: f64 = 75.0;
const EMPTY_OUTER: f64 = 98.0;
const EMPTY_INNER: f64 = 56.0;
const EMPTY_OUTER_PULSE: f64 = 11.0;
const EMPTY_INNER_PULSE: f64 = 6.0;
const ITEM_RADIUS: f64 = (OUTER + INNER) / 2.0;
const SIDE: f64 = OUTER + 12.0;
const TOP: f64 = OUTER + 44.0;
const BOTTOM: f64 = OUTER + 82.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Icon {
    Files,
    Applications,
    Windows,
    Wifi,
    Bluetooth,
    CustomMenu,
    Folder,
    Home,
    File,
    App,
    Window,
    Submenu,
    Action,
    WifiNetwork,
    WifiConnected,
    WifiLocked,
    WifiOff,
    BluetoothDevice,
    BluetoothConnected,
    BluetoothOff,
    Scan,
    Settings,
    ToggleOn,
    ToggleOff,
    Add,
    Disconnect,
    Forget,
    Palette,
    Keyboard,
    Appearance,
    MoveUp,
    MoveDown,
    Reset,
    ColorSwatch(u8, u8, u8),
}

#[derive(Clone, PartialEq)]
pub struct Item {
    pub label: String,
    pub icon: Icon,
    pub pixbuf: Option<Pixbuf>,
    pub active: bool,
}

pub type Rgb = (f64, f64, f64);

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub ring_top: Rgb,
    pub ring_mid: Rgb,
    pub ring_bottom: Rgb,
    pub rim_top: Rgb,
    pub rim_bottom: Rgb,
    pub divider: Rgb,
    pub shadow: Rgb,
    pub capsule_top: Rgb,
    pub capsule_bottom: Rgb,
    pub capsule_border: Rgb,
    pub text_primary: Rgb,
    pub text_capsule: Rgb,
    pub text_suggestion: Rgb,
    pub text_shadow: Rgb,
    pub accent_files: Rgb,
    pub accent_apps: Rgb,
    pub accent_windows: Rgb,
    pub accent_wifi: Rgb,
    pub accent_bluetooth: Rgb,
    pub accent_custom: Rgb,
    pub accent_action: Rgb,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            ring_top: (0.26, 0.15, 0.08),
            ring_mid: (0.15, 0.085, 0.05),
            ring_bottom: (0.18, 0.10, 0.06),
            rim_top: (1.0, 0.82, 0.62),
            rim_bottom: (0.96, 0.58, 0.28),
            divider: (1.0, 0.84, 0.68),
            shadow: (0.01, 0.004, 0.0),
            capsule_top: (0.21, 0.13, 0.08),
            capsule_bottom: (0.13, 0.075, 0.05),
            capsule_border: (1.0, 0.82, 0.66),
            text_primary: (1.0, 0.97, 0.94),
            text_capsule: (1.0, 0.96, 0.92),
            text_suggestion: (55_000.0 / 65_535.0, 47_500.0 / 65_535.0, 42_000.0 / 65_535.0),
            text_shadow: (0.035, 0.015, 0.008),
            accent_files: (1.0, 0.62, 0.28),
            accent_apps: (1.0, 0.78, 0.36),
            accent_windows: (1.0, 0.48, 0.34),
            accent_wifi: (0.36, 0.84, 0.70),
            accent_bluetooth: (0.42, 0.66, 1.0),
            accent_custom: (1.0, 0.72, 0.42),
            accent_action: (0.98, 0.56, 0.36),
        }
    }
}

impl Palette {
    fn accent(&self, icon: Icon) -> Rgb {
        match icon {
            Icon::Files | Icon::Folder | Icon::Home | Icon::File => self.accent_files,
            Icon::Applications | Icon::App => self.accent_apps,
            Icon::Windows | Icon::Window | Icon::Disconnect | Icon::Forget => self.accent_windows,
            Icon::Wifi
            | Icon::WifiNetwork
            | Icon::WifiConnected
            | Icon::WifiLocked
            | Icon::WifiOff
            | Icon::ToggleOn => self.accent_wifi,
            Icon::Bluetooth
            | Icon::BluetoothDevice
            | Icon::BluetoothConnected
            | Icon::BluetoothOff => self.accent_bluetooth,
            Icon::CustomMenu
            | Icon::Submenu
            | Icon::Settings
            | Icon::Scan
            | Icon::Add
            | Icon::Palette
            | Icon::Keyboard
            | Icon::Appearance
            | Icon::MoveUp
            | Icon::MoveDown => self.accent_custom,
            Icon::Action | Icon::ToggleOff | Icon::Reset => self.accent_action,
            Icon::ColorSwatch(r, g, b) => (
                f64::from(r) / 255.0,
                f64::from(g) / 255.0,
                f64::from(b) / 255.0,
            ),
        }
    }
}

pub struct View<'a> {
    pub motion: &'a Motion,
    pub palette: &'a Palette,
    pub count: usize,
    pub pressed: Option<usize>,
    pub title: &'a str,
    pub query: &'a str,
    pub suggestion: &'a str,
    pub status: &'a str,
    /// Current page and page count, both one-based.
    pub page: Option<(usize, usize)>,
    pub high_contrast: bool,
    pub solid: bool,
    pub blur_available: bool,
    pub loading_accent: Option<Rgb>,
}

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    center_x: f64,
    center_y: f64,
    scale: f64,
}

impl Geometry {
    pub fn new(width: f64, height: f64, anchor_x: f64, anchor_y: f64) -> Self {
        let scale = (width / (2.0 * SIDE))
            .min(height / (TOP + BOTTOM))
            .clamp(0.0, 1.0);
        // min/max tolerate the subpixel rounding of a perfectly fitted surface.
        Self {
            center_x: anchor_x.max(SIDE * scale).min(width - SIDE * scale),
            center_y: anchor_y.max(TOP * scale).min(height - BOTTOM * scale),
            scale,
        }
    }

    pub fn angle(&self, x: f64, y: f64, motion_scale: f64) -> Option<f64> {
        let scale = self.scale * motion_scale;
        if scale <= 0.0 {
            return None;
        }
        let dx = (x - self.center_x) / scale;
        let dy = (y - self.center_y) / scale;
        if !(INNER..=OUTER).contains(&dx.hypot(dy)) {
            return None;
        }
        Some(dy.atan2(dx))
    }

    /// Conservative logical-pixel bounding rectangle covering the painted ring,
    /// its drop shadow, captions, and (when loading) 3D perspective expansion.
    pub fn damage_rect(&self, motion: &Motion) -> RectangleInt {
        let scale = self.scale * motion.scale();
        if scale <= 0.0 {
            return RectangleInt::new(0, 0, 0, 0);
        }
        let (side, top, bottom) = if motion.loading_pose().is_some() {
            // 3D perspective scales the front edge up to ~1.20× OUTER plus shadow.
            (SIDE.max(224.0), TOP.max(224.0), BOTTOM.max(224.0))
        } else {
            (SIDE + 4.0, TOP + 4.0, BOTTOM + 4.0)
        };
        let x0 = (self.center_x - side * scale).floor() as i32 - 2;
        let y0 = (self.center_y - top * scale).floor() as i32 - 2;
        let x1 = (self.center_x + side * scale).ceil() as i32 + 2;
        let y1 = (self.center_y + bottom * scale).ceil() as i32 + 2;
        RectangleInt::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }
}

fn rgb_bits((r, g, b): Rgb) -> [u64; 3] {
    [r.to_bits(), g.to_bits(), b.to_bits()]
}

fn surface_device_scale(cr: &Context) -> (f64, f64) {
    let (sx, sy) = cr.target().device_scale();
    (
        if sx.is_finite() && sx > 0.0 { sx } else { 1.0 },
        if sy.is_finite() && sy > 0.0 { sy } else { 1.0 },
    )
}

fn pixbuf_surface(pixbuf: &Pixbuf) -> Result<ImageSurface, cairo::Error> {
    thread_local! {
        static CACHE: RefCell<HashMap<usize, (glib::WeakRef<Pixbuf>, ImageSurface)>> =
            RefCell::new(HashMap::new());
    }
    let key = pixbuf.as_ptr() as usize;
    CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if let Some((weak, surface)) = map.get(&key)
            && weak.upgrade().is_some_and(|alive| alive == *pixbuf)
        {
            return Ok(surface.clone());
        }
        let surface = ImageSurface::create(Format::ARgb32, pixbuf.width(), pixbuf.height())?;
        {
            let ctx = Context::new(&surface)?;
            ctx.set_operator(cairo::Operator::Source);
            ctx.set_source_pixbuf(pixbuf, 0.0, 0.0);
            ctx.paint()?;
        }
        if map.len() >= 64 {
            map.retain(|_, (weak, _)| weak.upgrade().is_some());
        }
        map.insert(key, (pixbuf.downgrade(), surface.clone()));
        Ok(surface)
    })
}

fn color(cr: &Context, (r, g, b): Rgb, alpha: f64) {
    cr.set_source_rgba(r, g, b, alpha);
}

fn sector_at(cr: &Context, start: f64, end: f64, inner: f64, outer: f64) {
    cr.new_path();
    cr.arc(0.0, 0.0, outer, start, end);
    cr.arc_negative(0.0, 0.0, inner, end, start);
    cr.close_path();
}

fn sector(cr: &Context, start: f64, end: f64) {
    sector_at(cr, start, end, INNER, OUTER);
}

fn ring_radii(motion: &Motion) -> Option<(f64, f64)> {
    let empty = motion.empty_ring()?;
    let target_outer = EMPTY_OUTER + EMPTY_OUTER_PULSE * empty.wave;
    let target_inner = EMPTY_INNER - EMPTY_INNER_PULSE * empty.wave;
    let outer = OUTER + (target_outer - OUTER) * empty.mix;
    let inner = INNER + (target_inner - INNER) * empty.mix;
    Some((inner, outer))
}

fn caption_outer_radius(motion: &Motion) -> f64 {
    if motion.loading_pose().is_some() || motion.panel_mix() > 0.001 {
        return OUTER;
    }
    match motion.empty_ring() {
        Some(empty) => {
            let envelope = EMPTY_OUTER + EMPTY_OUTER_PULSE * empty.pulse;
            OUTER + (envelope - OUTER) * empty.mix
        }
        None => OUTER,
    }
}

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, PI + FRAC_PI_2);
    cr.close_path();
}

/// Pixel centers inside a horizontal capsule, in surface-local logical units.
/// Equal width and height give a disk. Cairo coalesces identical scanlines.
fn capsule_region(cx: f64, cy: f64, width: f64, height: f64) -> Region {
    let radius = height * 0.5;
    let straight = (width - height) * 0.5;
    let first_pixel = |edge: f64| (edge - 0.5).ceil() as i32;
    let rows: Vec<_> = (first_pixel(cy - radius)..first_pixel(cy + radius))
        .filter_map(|y| {
            let dy = f64::from(y) + 0.5 - cy;
            let reach = straight + (radius * radius - dy * dy).max(0.0).sqrt();
            let left = first_pixel(cx - reach);
            let right = first_pixel(cx + reach);
            (right > left).then(|| RectangleInt::new(left, y, right - left, 1))
        })
        .collect();
    Region::create_rectangles(&rows)
}

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
enum Material {
    Frosted,
    Translucent,
    Solid,
}

impl Material {
    fn for_view(view: &View<'_>) -> Self {
        if view.solid || view.high_contrast {
            Self::Solid
        } else if view.blur_available {
            Self::Frosted
        } else {
            Self::Translucent
        }
    }

    fn opacity(self, frosted: f64, translucent: f64) -> f64 {
        match self {
            Self::Frosted => frosted,
            Self::Translucent => translucent,
            Self::Solid => 1.0,
        }
    }
}

/// Icon paths are drawn in a 24 × 24 grid.
fn glyph(cr: &Context, icon: Icon) -> Result<(), cairo::Error> {
    cr.new_path();
    cr.set_line_width(1.6);
    cr.set_line_cap(LineCap::Round);
    cr.set_line_join(LineJoin::Round);
    match icon {
        Icon::Files | Icon::Folder => {
            cr.move_to(3.0, 8.5);
            cr.line_to(3.0, 6.3);
            cr.curve_to(3.0, 5.1, 3.9, 4.3, 5.2, 4.3);
            cr.line_to(9.1, 4.3);
            cr.line_to(11.8, 7.0);
            cr.line_to(18.6, 7.0);
            cr.curve_to(20.1, 7.0, 21.0, 7.9, 21.0, 9.4);
            cr.line_to(21.0, 17.3);
            cr.curve_to(21.0, 18.8, 20.0, 19.7, 18.7, 19.7);
            cr.line_to(5.3, 19.7);
            cr.curve_to(3.9, 19.7, 3.0, 18.8, 3.0, 17.4);
            cr.close_path();
            cr.move_to(3.5, 10.0);
            cr.line_to(20.5, 10.0);
        }
        Icon::Home => {
            cr.move_to(3.2, 10.6);
            cr.line_to(12.0, 3.4);
            cr.line_to(20.8, 10.6);
            cr.move_to(5.4, 8.8);
            cr.line_to(5.4, 18.2);
            cr.curve_to(5.4, 19.4, 6.2, 20.1, 7.4, 20.1);
            cr.line_to(16.6, 20.1);
            cr.curve_to(17.8, 20.1, 18.6, 19.4, 18.6, 18.2);
            cr.line_to(18.6, 8.8);
            cr.move_to(9.8, 20.1);
            cr.line_to(9.8, 14.8);
            cr.curve_to(9.8, 13.7, 10.5, 13.1, 11.5, 13.1);
            cr.line_to(12.5, 13.1);
            cr.curve_to(13.5, 13.1, 14.2, 13.7, 14.2, 14.8);
            cr.line_to(14.2, 20.1);
        }
        Icon::Applications | Icon::App => {
            for (x, y) in [(3.4, 3.4), (13.3, 3.4), (3.4, 13.3), (13.3, 13.3)] {
                rounded_rect(cr, x, y, 7.2, 7.2, 1.8);
            }
        }
        Icon::Windows | Icon::Window => {
            cr.move_to(6.4, 16.2);
            cr.line_to(4.3, 16.2);
            cr.curve_to(3.0, 16.2, 2.4, 15.4, 2.4, 14.2);
            cr.line_to(2.4, 5.5);
            cr.curve_to(2.4, 4.2, 3.3, 3.5, 4.5, 3.5);
            cr.line_to(15.0, 3.5);
            cr.curve_to(16.2, 3.5, 17.0, 4.3, 17.0, 5.4);
            cr.line_to(17.0, 6.4);
            rounded_rect(cr, 7.1, 8.0, 14.5, 12.3, 1.9);
            cr.move_to(7.7, 11.6);
            cr.line_to(21.0, 11.6);
        }
        Icon::File => {
            cr.move_to(6.0, 2.8);
            cr.line_to(13.5, 2.8);
            cr.line_to(19.4, 8.8);
            cr.line_to(19.4, 19.5);
            cr.curve_to(19.4, 20.5, 18.5, 21.2, 17.6, 21.2);
            cr.line_to(6.1, 21.2);
            cr.curve_to(5.0, 21.2, 4.5, 20.4, 4.5, 19.4);
            cr.line_to(4.5, 4.4);
            cr.curve_to(4.5, 3.4, 5.1, 2.8, 6.0, 2.8);
            cr.close_path();
            cr.move_to(13.4, 3.3);
            cr.line_to(13.4, 9.1);
            cr.line_to(18.8, 9.1);
            cr.move_to(8.0, 14.0);
            cr.line_to(15.5, 14.0);
        }
        Icon::CustomMenu | Icon::Submenu => {
            cr.new_sub_path();
            cr.arc(12.0, 12.0, 8.6, 0.0, TAU);
            cr.new_sub_path();
            cr.arc(12.0, 12.0, 3.8, 0.0, TAU);
            if icon == Icon::CustomMenu {
                for angle in [0.0, FRAC_PI_2, PI, PI + FRAC_PI_2] {
                    cr.move_to(12.0 + 3.8 * angle.cos(), 12.0 + 3.8 * angle.sin());
                    cr.line_to(12.0 + 8.6 * angle.cos(), 12.0 + 8.6 * angle.sin());
                }
            }
        }
        Icon::Action => {
            rounded_rect(cr, 3.0, 4.2, 18.0, 15.6, 2.6);
            cr.move_to(7.5, 9.2);
            cr.line_to(11.0, 12.0);
            cr.line_to(7.5, 14.8);
            cr.move_to(12.5, 14.8);
            cr.line_to(16.8, 14.8);
        }
        Icon::Wifi | Icon::WifiNetwork | Icon::WifiConnected | Icon::WifiLocked | Icon::WifiOff => {
            let cy = 17.6;
            for r in [12.0, 8.2, 4.4] {
                cr.new_sub_path();
                cr.arc(12.0, cy, r, -PI * 0.76, -PI * 0.24);
            }
            cr.new_sub_path();
            cr.arc(12.0, cy, 1.35, 0.0, TAU);
            if icon == Icon::WifiLocked {
                // Small padlock in the bottom-right corner.
                rounded_rect(cr, 15.4, 15.6, 6.2, 5.2, 1.1);
                cr.new_sub_path();
                cr.arc(18.5, 15.6, 2.0, PI, TAU);
            } else if icon == Icon::WifiOff {
                cr.move_to(4.0, 4.0);
                cr.line_to(20.0, 20.0);
            }
        }
        Icon::Bluetooth | Icon::BluetoothDevice | Icon::BluetoothConnected | Icon::BluetoothOff => {
            cr.move_to(7.2, 7.2);
            cr.line_to(16.8, 16.4);
            cr.line_to(12.0, 20.6);
            cr.line_to(12.0, 3.4);
            cr.line_to(16.8, 7.6);
            cr.line_to(7.2, 16.8);
            if icon == Icon::BluetoothConnected {
                cr.new_sub_path();
                cr.arc(12.0, 12.0, 8.8, -0.45, 0.45);
                cr.new_sub_path();
                cr.arc(12.0, 12.0, 8.8, PI - 0.45, PI + 0.45);
            } else if icon == Icon::BluetoothOff {
                cr.move_to(4.0, 4.0);
                cr.line_to(20.0, 20.0);
            }
        }
        Icon::Scan => {
            cr.new_sub_path();
            cr.arc(12.0, 12.0, 7.6, -2.55, 0.35);
            cr.move_to(16.6, 14.3);
            cr.line_to(19.5, 14.9);
            cr.line_to(19.6, 11.8);
            cr.new_sub_path();
            cr.arc(12.0, 12.0, 7.6, 0.6, 3.45);
            cr.move_to(7.4, 9.7);
            cr.line_to(4.5, 9.1);
            cr.line_to(4.4, 12.2);
        }
        Icon::Settings => {
            cr.move_to(4.0, 7.0);
            cr.line_to(8.2, 7.0);
            cr.move_to(13.0, 7.0);
            cr.line_to(20.0, 7.0);
            cr.new_sub_path();
            cr.arc(10.6, 7.0, 2.3, 0.0, TAU);

            cr.move_to(4.0, 12.0);
            cr.line_to(13.2, 12.0);
            cr.move_to(18.0, 12.0);
            cr.line_to(20.0, 12.0);
            cr.new_sub_path();
            cr.arc(15.6, 12.0, 2.3, 0.0, TAU);

            cr.move_to(4.0, 17.0);
            cr.line_to(6.8, 17.0);
            cr.move_to(11.6, 17.0);
            cr.line_to(20.0, 17.0);
            cr.new_sub_path();
            cr.arc(9.2, 17.0, 2.3, 0.0, TAU);
        }
        Icon::ToggleOn | Icon::ToggleOff => {
            rounded_rect(cr, 2.8, 6.8, 18.4, 10.4, 5.2);
            let knob_x = if icon == Icon::ToggleOn { 15.8 } else { 8.2 };
            cr.new_sub_path();
            cr.arc(knob_x, 12.0, 3.0, 0.0, TAU);
        }
        Icon::Add => {
            cr.new_sub_path();
            cr.arc(12.0, 12.0, 8.6, 0.0, TAU);
            cr.move_to(12.0, 7.4);
            cr.line_to(12.0, 16.6);
            cr.move_to(7.4, 12.0);
            cr.line_to(16.6, 12.0);
        }
        Icon::Disconnect => {
            cr.new_sub_path();
            cr.arc(12.0, 12.0, 8.4, 0.0, TAU);
            cr.move_to(8.8, 8.8);
            cr.line_to(15.2, 15.2);
            cr.move_to(15.2, 8.8);
            cr.line_to(8.8, 15.2);
        }
        Icon::Forget => {
            cr.move_to(4.5, 6.8);
            cr.line_to(19.5, 6.8);
            cr.move_to(9.2, 6.8);
            cr.line_to(9.6, 4.4);
            cr.line_to(14.4, 4.4);
            cr.line_to(14.8, 6.8);
            cr.move_to(6.6, 6.8);
            cr.line_to(7.4, 19.0);
            cr.curve_to(7.5, 20.0, 8.2, 20.5, 9.2, 20.5);
            cr.line_to(14.8, 20.5);
            cr.curve_to(15.8, 20.5, 16.5, 20.0, 16.6, 19.0);
            cr.line_to(17.4, 6.8);
            cr.move_to(10.2, 10.6);
            cr.line_to(10.2, 16.8);
            cr.move_to(13.8, 10.6);
            cr.line_to(13.8, 16.8);
        }
        Icon::Palette => {
            // Artist palette outline with paint dots.
            cr.new_sub_path();
            cr.arc(12.0, 12.0, 8.6, 0.35, TAU - 0.15);
            cr.curve_to(19.0, 13.8, 15.6, 13.8, 15.2, 16.2);
            cr.curve_to(14.8, 18.6, 17.8, 19.2, 16.4, 20.0);
            for (cx, cy) in [(8.0, 9.2), (12.0, 7.2), (16.0, 9.2), (7.6, 13.8)] {
                cr.new_sub_path();
                cr.arc(cx, cy, 1.1, 0.0, TAU);
            }
        }
        Icon::Keyboard => {
            rounded_rect(cr, 2.8, 6.2, 18.4, 11.6, 2.4);
            for x in [6.2, 9.6, 13.0, 16.4] {
                cr.move_to(x, 9.8);
                cr.line_to(x + 1.2, 9.8);
            }
            for x in [7.4, 10.8, 14.2] {
                cr.move_to(x, 12.6);
                cr.line_to(x + 1.2, 12.6);
            }
            cr.move_to(7.5, 15.2);
            cr.line_to(16.5, 15.2);
        }
        Icon::Appearance => {
            // Half-filled contrast / display circle with sparkle rays.
            cr.new_sub_path();
            cr.arc(12.0, 12.0, 5.2, 0.0, TAU);
            cr.move_to(12.0, 6.8);
            cr.line_to(12.0, 17.2);
            for i in 0..8 {
                let a = f64::from(i) * FRAC_PI_4;
                cr.move_to(12.0 + a.cos() * 7.6, 12.0 + a.sin() * 7.6);
                cr.line_to(12.0 + a.cos() * 9.4, 12.0 + a.sin() * 9.4);
            }
        }
        Icon::MoveUp => {
            cr.move_to(12.0, 18.5);
            cr.line_to(12.0, 5.8);
            cr.move_to(7.0, 10.8);
            cr.line_to(12.0, 5.8);
            cr.line_to(17.0, 10.8);
        }
        Icon::MoveDown => {
            cr.move_to(12.0, 5.5);
            cr.line_to(12.0, 18.2);
            cr.move_to(7.0, 13.2);
            cr.line_to(12.0, 18.2);
            cr.line_to(17.0, 13.2);
        }
        Icon::Reset => {
            cr.new_sub_path();
            cr.arc(12.0, 12.0, 7.6, -2.35, 2.55);
            cr.move_to(6.2, 4.4);
            cr.line_to(6.6, 8.8);
            cr.line_to(10.8, 7.6);
        }
        Icon::ColorSwatch(..) => {
            cr.new_sub_path();
            cr.arc(12.0, 12.0, 7.8, 0.0, TAU);
        }
    }
    cr.stroke()
}

fn text_layout(cr: &Context, text: &str, size: f64, bold: bool) -> gtk::pango::Layout {
    let layout = pangocairo::functions::create_layout(cr);
    let mut font = FontDescription::new();
    font.set_family("Sans");
    font.set_weight(if bold {
        Weight::Semibold
    } else {
        Weight::Medium
    });
    font.set_absolute_size(size * f64::from(SCALE));
    layout.set_font_description(Some(&font));
    layout.set_single_paragraph_mode(true);
    layout.set_text(text);
    layout
}

fn draw_centered_text(
    cr: &Context,
    layout: &gtk::pango::Layout,
    (x, y): (f64, f64),
    width: f64,
    text_color: Rgb,
    shadow: Rgb,
    alpha: f64,
) -> Result<(), cairo::Error> {
    layout.set_ellipsize(EllipsizeMode::End);
    layout.set_alignment(Alignment::Center);
    layout.set_width((width * f64::from(SCALE)) as i32);
    let (_, h) = layout.pixel_size();
    let top = y - f64::from(h) / 2.0;
    cr.move_to(x - width / 2.0, top + 1.0);
    color(cr, shadow, 0.80 * alpha);
    pangocairo::functions::show_layout(cr, layout);
    cr.move_to(x - width / 2.0, top);
    color(cr, text_color, alpha);
    pangocairo::functions::show_layout(cr, layout);
    Ok(())
}

fn center_text(
    cr: &Context,
    layout: &gtk::pango::Layout,
    y: f64,
    width: f64,
    text_color: Rgb,
    shadow: Rgb,
) -> Result<(), cairo::Error> {
    draw_centered_text(cr, layout, (0.0, y), width, text_color, shadow, 1.0)
}

fn add_suggestion(layout: &gtk::pango::Layout, text: &str, suffix: &str, palette: &Palette) {
    if suffix.is_empty() {
        return;
    }
    layout.set_text(&format!("{text}{suffix}"));
    let attrs = AttrList::new();
    let (r, g, b) = palette.text_suggestion;
    let mut ghost = AttrColor::new_foreground(
        (r * 65_535.0).round() as u16,
        (g * 65_535.0).round() as u16,
        (b * 65_535.0).round() as u16,
    );
    ghost.set_start_index(text.len() as u32);
    attrs.insert(ghost);
    layout.set_attributes(Some(&attrs));
}

fn capsule(
    cr: &Context,
    text: &str,
    y: f64,
    max_w: f64,
    suggestion: Option<&str>,
    material: Material,
    high_contrast: bool,
    palette: &Palette,
) -> Result<(f64, f64), cairo::Error> {
    let is_query = suggestion.is_some();
    let suffix = suggestion.unwrap_or_default();
    let layout = text_layout(cr, text, if is_query { 14.0 } else { 12.0 }, true);
    add_suggestion(&layout, text, suffix, palette);
    let (text_width, _) = layout.pixel_size();
    let h = if is_query { 32.0 } else { 26.0 };
    let w = (f64::from(text_width) + 26.0).clamp(h, max_w);
    let left = -w / 2.0;
    let top = y - h / 2.0;
    let radius = h / 2.0;

    // Soft graduated outer shadow strictly outside the capsule glass, matching the ring.
    cr.save()?;
    cr.set_fill_rule(cairo::FillRule::EvenOdd);
    for (pad, alpha) in [(1.8, 0.065), (3.6, 0.038), (5.4, 0.018)] {
        cr.new_path();
        rounded_rect(
            cr,
            left - pad,
            top + 1.0 - pad,
            w + 2.0 * pad,
            h + 2.0 * pad,
            radius + pad,
        );
        rounded_rect(cr, left, top, w, h, radius);
        color(cr, palette.shadow, alpha);
        cr.fill()?;
    }
    cr.restore()?;

    // Carry the ring gradient through the captions, tinted by the capsule colors.
    let top_color = lerp_rgb(palette.ring_top, palette.capsule_top, 0.5);
    let bottom_color = lerp_rgb(palette.ring_bottom, palette.capsule_bottom, 0.5);
    let rim_top = lerp_rgb(palette.rim_top, palette.capsule_border, 0.5);
    let glass = LinearGradient::new(-w * 0.3, top, w * 0.25, top + h);
    for (stop, rgb, alpha) in [
        (0.0, top_color, material.opacity(0.61, 0.71)),
        (0.42, palette.ring_mid, material.opacity(0.55, 0.66)),
        (1.0, bottom_color, material.opacity(0.63, 0.74)),
    ] {
        glass.add_color_stop_rgba(stop, rgb.0, rgb.1, rgb.2, alpha);
    }
    cr.new_path();
    rounded_rect(cr, left, top, w, h, radius);
    cr.set_source(&glass)?;
    cr.fill()?;

    // Subtle inner top specular catchlight for translucent glass depth.
    if !high_contrast && material != Material::Solid {
        cr.save()?;
        cr.new_path();
        rounded_rect(cr, left, top, w, h, radius);
        cr.clip();
        let sheen = LinearGradient::new(0.0, top, 0.0, y);
        sheen.add_color_stop_rgba(0.0, rim_top.0, rim_top.1, rim_top.2, 0.10);
        sheen.add_color_stop_rgba(1.0, rim_top.0, rim_top.1, rim_top.2, 0.0);
        cr.set_source(&sheen)?;
        cr.paint()?;
        cr.restore()?;
    }

    // Diagonal glass rim matching `draw_ring_rim_at`.
    let rim_bottom = lerp_rgb(palette.capsule_border, palette.rim_bottom, 0.5);
    let rim = LinearGradient::new(-w * 0.25, top, w * 0.25, top + h);
    rim.add_color_stop_rgba(
        0.0,
        rim_top.0,
        rim_top.1,
        rim_top.2,
        if high_contrast { 1.0 } else { 0.70 },
    );
    rim.add_color_stop_rgba(
        1.0,
        rim_bottom.0,
        rim_bottom.1,
        rim_bottom.2,
        if high_contrast { 1.0 } else { 0.27 },
    );
    cr.new_path();
    rounded_rect(cr, left, top, w, h, radius);
    cr.set_source(&rim)?;
    cr.set_line_width(if high_contrast { 1.6 } else { 1.0 });
    cr.stroke()?;

    if is_query {
        // Keep the end of a typed path visible, or clip the preview after it.
        layout.set_ellipsize(if suffix.is_empty() {
            EllipsizeMode::Start
        } else {
            EllipsizeMode::End
        });
        layout.set_width(((w - 20.0) * f64::from(SCALE)) as i32);
        layout.set_alignment(Alignment::Center);
        let (_, text_h) = layout.pixel_size();
        let text_top = y - f64::from(text_h) / 2.0;
        let attrs = layout.attributes();
        layout.set_attributes(None);
        cr.move_to(-(w - 20.0) / 2.0, text_top + 1.0);
        color(cr, palette.text_shadow, 0.80);
        pangocairo::functions::show_layout(cr, &layout);
        layout.set_attributes(attrs.as_ref());
        cr.move_to(-(w - 20.0) / 2.0, text_top);
        cr.set_source_rgb(
            palette.text_capsule.0,
            palette.text_capsule.1,
            palette.text_capsule.2,
        );
        pangocairo::functions::show_layout(cr, &layout);
    } else {
        center_text(
            cr,
            &layout,
            y,
            w - 20.0,
            palette.text_capsule,
            palette.text_shadow,
        )?;
    }
    Ok((w, h))
}

#[derive(Clone, Eq, PartialEq, Hash)]
struct TextMeasureKey {
    text: String,
    size_bits: u64,
    bold: bool,
}

/// Measures single-line unwrapped text on a 1×1 identity-scale Cairo context so
/// animations that vary CTM scale never thrash FreeType's scaled-font cache.
pub(crate) fn measure_text(text: &str, size: f64, bold: bool) -> (i32, i32) {
    if text.is_empty() {
        return (0, 0);
    }
    thread_local! {
        static SCRATCH: RefCell<Option<Context>> = const { RefCell::new(None) };
        static CACHE: RefCell<HashMap<TextMeasureKey, (i32, i32)>> = RefCell::new(HashMap::new());
    }
    let key = TextMeasureKey {
        text: text.to_owned(),
        size_bits: size.to_bits(),
        bold,
    };
    CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if let Some(&dims) = map.get(&key) {
            return dims;
        }
        let dims = SCRATCH.with(|scratch| {
            let mut slot = scratch.borrow_mut();
            let cr = slot.get_or_insert_with(|| {
                let surf = ImageSurface::create(Format::ARgb32, 1, 1).expect("1x1 scratch surface");
                Context::new(&surf).expect("1x1 scratch context")
            });
            text_layout(cr, text, size, bold).pixel_size()
        });
        if map.len() >= 256 {
            map.clear();
        }
        map.insert(key, dims);
        dims
    })
}

#[derive(Clone, Eq, PartialEq, Hash)]
struct TextSurfaceKey {
    text: String,
    size_bits: u64,
    bold: bool,
    width_px: i32,
    wrap_lines: i8,
    ellipsize: u8,
    alignment: u8,
    text_color: [u64; 3],
    shadow: Option<([u64; 3], u64)>,
    sx: u64,
    sy: u64,
}

#[derive(Clone)]
pub(crate) struct CachedText {
    pub(crate) surface: ImageSurface,
    pub(crate) origin_x: f64,
    pub(crate) origin_y: f64,
    pub(crate) logical_w: i32,
    pub(crate) logical_h: i32,
}

fn ellipsize_code(mode: EllipsizeMode) -> u8 {
    match mode {
        EllipsizeMode::None => 0,
        EllipsizeMode::Start => 1,
        EllipsizeMode::Middle => 2,
        EllipsizeMode::End => 3,
        _ => 3,
    }
}

fn alignment_code(align: Alignment) -> u8 {
    match align {
        Alignment::Left => 0,
        Alignment::Center => 1,
        Alignment::Right => 2,
        _ => 0,
    }
}

/// Renders and caches a styled Pango text layout (including optional drop shadow)
/// at identity logical scale (device scale `(sx, sy)`) so scaling/hover/morph
/// animations simply blit a tight `ImageSurface` without re-shaping or thrashing
/// FreeType's glyph cache.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cached_text_surface(
    cr: &Context,
    text: &str,
    size: f64,
    bold: bool,
    width_px: i32,
    wrap_lines: i8,
    ellipsize: EllipsizeMode,
    alignment: Alignment,
    text_color: Rgb,
    shadow: Option<(Rgb, f64)>,
) -> Result<Option<CachedText>, cairo::Error> {
    if text.is_empty() {
        return Ok(None);
    }
    let (sx, sy) = surface_device_scale(cr);
    let key = TextSurfaceKey {
        text: text.to_owned(),
        size_bits: size.to_bits(),
        bold,
        width_px,
        wrap_lines,
        ellipsize: ellipsize_code(ellipsize),
        alignment: alignment_code(alignment),
        text_color: rgb_bits(text_color),
        shadow: shadow.map(|(rgb, a)| (rgb_bits(rgb), a.to_bits())),
        sx: sx.to_bits(),
        sy: sy.to_bits(),
    };
    thread_local! {
        static MEASURE_CTX: RefCell<Option<Context>> = const { RefCell::new(None) };
        static CACHE: RefCell<HashMap<TextSurfaceKey, Option<CachedText>>> =
            RefCell::new(HashMap::new());
    }
    CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if let Some(hit) = map.get(&key) {
            return Ok(hit.clone());
        }
        let built = MEASURE_CTX.with(|scratch| -> Result<Option<CachedText>, cairo::Error> {
            let mut slot = scratch.borrow_mut();
            let mcr = match slot.as_ref() {
                Some(c) => c,
                None => {
                    let surf = ImageSurface::create(Format::ARgb32, 1, 1)?;
                    let c = Context::new(&surf)?;
                    *slot = Some(c);
                    slot.as_ref().unwrap()
                }
            };
            let layout = text_layout(mcr, text, size, bold);
            if width_px > 0 {
                layout.set_width(width_px * SCALE);
            }
            if wrap_lines > 0 {
                layout.set_wrap(WrapMode::WordChar);
                layout.set_height(-i32::from(wrap_lines));
            }
            layout.set_ellipsize(ellipsize);
            layout.set_alignment(alignment);
            let (ink, logical) = layout.pixel_extents();
            let shadow_dy = if shadow.is_some() { 1 } else { 0 };
            let x0 = ink.x().min(logical.x()) - 2;
            let y0 = ink.y().min(logical.y()) - 2;
            let x1 = (ink.x() + ink.width()).max(logical.x() + logical.width()) + 2;
            let y1 = (ink.y() + ink.height() + shadow_dy)
                .max(logical.y() + logical.height() + shadow_dy)
                + 2;
            let lw = x1 - x0;
            let lh = y1 - y0;
            if lw <= 0 || lh <= 0 {
                return Ok(None);
            }
            let surf = ImageSurface::create(
                Format::ARgb32,
                (f64::from(lw) * sx).ceil() as i32,
                (f64::from(lh) * sy).ceil() as i32,
            )?;
            surf.set_device_scale(sx, sy);
            {
                let ctx = Context::new(&surf)?;
                if let Some((shadow_rgb, shadow_alpha)) = shadow {
                    ctx.move_to(f64::from(-x0), f64::from(-y0) + 1.0);
                    color(&ctx, shadow_rgb, shadow_alpha);
                    pangocairo::functions::show_layout(&ctx, &layout);
                }
                ctx.move_to(f64::from(-x0), f64::from(-y0));
                color(&ctx, text_color, 1.0);
                pangocairo::functions::show_layout(&ctx, &layout);
            }
            Ok(Some(CachedText {
                surface: surf,
                origin_x: f64::from(x0),
                origin_y: f64::from(y0),
                logical_w: logical.width(),
                logical_h: logical.height(),
            }))
        })?;
        if map.len() >= 384 {
            map.clear();
        }
        map.insert(key, built.clone());
        Ok(built)
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_cached_centered_text(
    cr: &Context,
    text: &str,
    size: f64,
    bold: bool,
    (x, y): (f64, f64),
    width: f64,
    text_color: Rgb,
    shadow: Rgb,
    alpha: f64,
) -> Result<(), cairo::Error> {
    if text.is_empty() || alpha <= 0.004 {
        return Ok(());
    }
    let width_px = width.floor().max(1.0) as i32;
    let (nat_w, _) = measure_text(text, size, bold);
    let (key_w, align) = if nat_w <= width_px {
        (0, Alignment::Left)
    } else {
        (width_px, Alignment::Center)
    };
    if let Some(cached) = cached_text_surface(
        cr,
        text,
        size,
        bold,
        key_w,
        0,
        if key_w > 0 {
            EllipsizeMode::End
        } else {
            EllipsizeMode::None
        },
        align,
        text_color,
        Some((shadow, 0.80)),
    )? {
        let tx = if key_w == 0 {
            x - f64::from(cached.logical_w) * 0.5
        } else {
            x - f64::from(key_w) * 0.5
        };
        let ty = y - f64::from(cached.logical_h) * 0.5;
        paint_surface(cr, &cached.surface, tx + cached.origin_x, ty + cached.origin_y, alpha)?;
    }
    Ok(())
}

const CAPSULE_CACHE_PAD: f64 = 8.0;

#[derive(Clone, Eq, PartialEq, Hash)]
struct CapsuleKey {
    text: String,
    suggestion: Option<String>,
    max_w_bits: u64,
    material: Material,
    high_contrast: bool,
    ring_top: [u64; 3],
    ring_mid: [u64; 3],
    ring_bottom: [u64; 3],
    capsule_top: [u64; 3],
    capsule_bottom: [u64; 3],
    capsule_border: [u64; 3],
    rim_top: [u64; 3],
    rim_bottom: [u64; 3],
    shadow: [u64; 3],
    text_capsule: [u64; 3],
    text_suggestion: [u64; 3],
    text_shadow: [u64; 3],
    sx: u64,
    sy: u64,
}

#[allow(clippy::too_many_arguments)]
fn capsule_cached(
    cr: &Context,
    text: &str,
    y: f64,
    max_w: f64,
    suggestion: Option<&str>,
    material: Material,
    high_contrast: bool,
    palette: &Palette,
    alpha: f64,
) -> Result<(f64, f64), cairo::Error> {
    let (sx, sy) = surface_device_scale(cr);
    let key = CapsuleKey {
        text: text.to_owned(),
        suggestion: suggestion.map(str::to_owned),
        max_w_bits: max_w.to_bits(),
        material,
        high_contrast,
        ring_top: rgb_bits(palette.ring_top),
        ring_mid: rgb_bits(palette.ring_mid),
        ring_bottom: rgb_bits(palette.ring_bottom),
        capsule_top: rgb_bits(palette.capsule_top),
        capsule_bottom: rgb_bits(palette.capsule_bottom),
        capsule_border: rgb_bits(palette.capsule_border),
        rim_top: rgb_bits(palette.rim_top),
        rim_bottom: rgb_bits(palette.rim_bottom),
        shadow: rgb_bits(palette.shadow),
        text_capsule: rgb_bits(palette.text_capsule),
        text_suggestion: rgb_bits(palette.text_suggestion),
        text_shadow: rgb_bits(palette.text_shadow),
        sx: sx.to_bits(),
        sy: sy.to_bits(),
    };
    thread_local! {
        static CACHE: RefCell<HashMap<CapsuleKey, (ImageSurface, f64, f64)>> =
            RefCell::new(HashMap::new());
    }
    let (surf, w, h) = CACHE.with(|cache| -> Result<(ImageSurface, f64, f64), cairo::Error> {
        let mut map = cache.borrow_mut();
        if let Some(hit) = map.get(&key) {
            return Ok(hit.clone());
        }
        let is_query = suggestion.is_some();
        let suffix = suggestion.unwrap_or_default();
        let combined = if suffix.is_empty() {
            text.to_owned()
        } else {
            format!("{text}{suffix}")
        };
        let (text_width, _) = measure_text(&combined, if is_query { 14.0 } else { 12.0 }, true);
        let h = if is_query { 32.0 } else { 26.0 };
        let w = (f64::from(text_width) + 26.0).clamp(h, max_w);
        let surf_w = w + CAPSULE_CACHE_PAD * 2.0;
        let surf_h = h + CAPSULE_CACHE_PAD * 2.0;
        let surf = ImageSurface::create(
            Format::ARgb32,
            (surf_w * sx).ceil() as i32,
            (surf_h * sy).ceil() as i32,
        )?;
        surf.set_device_scale(sx, sy);
        let (actual_w, actual_h) = {
            let ctx = Context::new(&surf)?;
            ctx.translate(surf_w * 0.5, surf_h * 0.5);
            capsule(&ctx, text, 0.0, max_w, suggestion, material, high_contrast, palette)?
        };
        if map.len() >= 32 {
            map.clear();
        }
        map.insert(key, (surf.clone(), actual_w, actual_h));
        Ok((surf, actual_w, actual_h))
    })?;
    paint_surface(
        cr,
        &surf,
        -(w + CAPSULE_CACHE_PAD * 2.0) * 0.5,
        y - (h + CAPSULE_CACHE_PAD * 2.0) * 0.5,
        alpha,
    )?;
    Ok((w, h))
}

fn ring_material_at(
    cr: &Context,
    inner: f64,
    outer: f64,
    material: Material,
    palette: &Palette,
) -> Result<(), cairo::Error> {
    // A graduated shadow just outside the rim leaves the center fully clear.
    let shadow = RadialGradient::new(0.0, 1.0, outer, 0.0, 1.0, outer + 8.5);
    shadow.add_color_stop_rgba(0.0, palette.shadow.0, palette.shadow.1, palette.shadow.2, 0.13);
    shadow.add_color_stop_rgba(1.0, palette.shadow.0, palette.shadow.1, palette.shadow.2, 0.0);
    cr.new_path();
    cr.arc(0.0, 1.0, outer + 8.5, 0.0, TAU);
    cr.arc_negative(0.0, 1.0, outer, TAU, 0.0);
    cr.close_path();
    cr.set_source(&shadow)?;
    cr.fill()?;

    // Vertical gradient uses Pixman's fast scanline path during dynamic 2D pulse frames.
    let glass = LinearGradient::new(0.0, -outer, 0.0, outer);
    for (t, rgb, alpha) in [
        (0.0, palette.ring_top, material.opacity(0.61, 0.71)),
        (0.42, palette.ring_mid, material.opacity(0.55, 0.66)),
        (1.0, palette.ring_bottom, material.opacity(0.63, 0.74)),
    ] {
        glass.add_color_stop_rgba(t, rgb.0, rgb.1, rgb.2, alpha);
    }
    sector_at(cr, 0.0, TAU, inner, outer);
    cr.set_source(&glass)?;
    cr.fill()
}

fn ring_material(cr: &Context, material: Material, palette: &Palette) -> Result<(), cairo::Error> {
    ring_material_at(cr, INNER, OUTER, material, palette)
}

fn draw_ring_rim_at(
    cr: &Context,
    inner: f64,
    outer: f64,
    high_contrast: bool,
    palette: &Palette,
) -> Result<(), cairo::Error> {
    let rim = LinearGradient::new(0.0, -outer, 0.0, outer);
    rim.add_color_stop_rgba(
        0.0,
        palette.rim_top.0,
        palette.rim_top.1,
        palette.rim_top.2,
        if high_contrast { 1.0 } else { 0.70 },
    );
    rim.add_color_stop_rgba(
        1.0,
        palette.rim_bottom.0,
        palette.rim_bottom.1,
        palette.rim_bottom.2,
        if high_contrast { 1.0 } else { 0.27 },
    );
    cr.set_source(&rim)?;
    cr.set_line_width(if high_contrast { 1.8 } else { 1.0 });
    for radius in [inner, outer] {
        cr.new_sub_path();
        cr.arc(0.0, 0.0, radius, 0.0, TAU);
        cr.close_path();
    }
    cr.stroke()
}

fn draw_ring_rim(cr: &Context, high_contrast: bool, palette: &Palette) -> Result<(), cairo::Error> {
    draw_ring_rim_at(cr, INNER, OUTER, high_contrast, palette)
}

const RING_CACHE_HALF: f64 = 184.0;

#[derive(Clone, Copy, Eq, PartialEq)]
struct RingMaterialKey {
    material: Material,
    top: [u64; 3],
    mid: [u64; 3],
    bottom: [u64; 3],
    shadow: [u64; 3],
    sx: u64,
    sy: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct RingRimKey {
    high_contrast: bool,
    rim_top: [u64; 3],
    rim_bottom: [u64; 3],
    sx: u64,
    sy: u64,
}

fn draw_ring_material_cached(
    cr: &Context,
    material: Material,
    palette: &Palette,
) -> Result<(), cairo::Error> {
    thread_local! {
        static CACHE: RefCell<Option<(RingMaterialKey, ImageSurface)>> = const { RefCell::new(None) };
    }
    let (sx, sy) = surface_device_scale(cr);
    let key = RingMaterialKey {
        material,
        top: rgb_bits(palette.ring_top),
        mid: rgb_bits(palette.ring_mid),
        bottom: rgb_bits(palette.ring_bottom),
        shadow: rgb_bits(palette.shadow),
        sx: sx.to_bits(),
        sy: sy.to_bits(),
    };
    let surface = CACHE.with(|cache| -> Result<ImageSurface, cairo::Error> {
        let mut slot = cache.borrow_mut();
        if let Some((cached_key, surf)) = slot.as_ref()
            && *cached_key == key
        {
            return Ok(surf.clone());
        }
        let dim_x = (RING_CACHE_HALF * 2.0 * sx).ceil() as i32;
        let dim_y = (RING_CACHE_HALF * 2.0 * sy).ceil() as i32;
        let surf = ImageSurface::create(Format::ARgb32, dim_x, dim_y)?;
        surf.set_device_scale(sx, sy);
        {
            let ctx = Context::new(&surf)?;
            ctx.translate(RING_CACHE_HALF, RING_CACHE_HALF);
            ring_material(&ctx, material, palette)?;
        }
        *slot = Some((key, surf.clone()));
        Ok(surf)
    })?;
    paint_surface(cr, &surface, -RING_CACHE_HALF, -RING_CACHE_HALF, 1.0)
}

pub(crate) fn paint_surface(
    cr: &Context,
    surface: &ImageSurface,
    x: f64,
    y: f64,
    alpha: f64,
) -> Result<(), cairo::Error> {
    if alpha <= 0.004 {
        return Ok(());
    }
    if alpha >= 0.996 {
        cr.set_source_surface(surface, x, y)?;
        cr.source().set_filter(cairo::Filter::Bilinear);
        return cr.paint();
    }
    let mat = cr.matrix();
    let (src_sx, src_sy) = surface.device_scale();
    let (dst_sx, dst_sy) = surface_device_scale(cr);
    let pure_unit_scale = (mat.xx() - 1.0).abs() < 1e-6
        && (mat.yy() - 1.0).abs() < 1e-6
        && mat.xy().abs() < 1e-6
        && mat.yx().abs() < 1e-6
        && (src_sx - dst_sx).abs() < 1e-6
        && (src_sy - dst_sy).abs() < 1e-6;
    cr.set_source_surface(surface, x, y)?;
    if pure_unit_scale {
        let tx = (mat.x0() + x) * dst_sx;
        let ty = (mat.y0() + y) * dst_sy;
        if (tx - tx.round()).abs() < 1e-3 && (ty - ty.round()).abs() < 1e-3 {
            cr.source().set_filter(cairo::Filter::Nearest);
        } else {
            cr.source().set_filter(cairo::Filter::Bilinear);
        }
    } else {
        cr.source().set_filter(cairo::Filter::Bilinear);
    }
    cr.paint_with_alpha(alpha)
}

fn draw_ring_rim_cached(
    cr: &Context,
    high_contrast: bool,
    palette: &Palette,
) -> Result<(), cairo::Error> {
    thread_local! {
        static CACHE: RefCell<Option<(RingRimKey, ImageSurface)>> = const { RefCell::new(None) };
    }
    let (sx, sy) = surface_device_scale(cr);
    let key = RingRimKey {
        high_contrast,
        rim_top: rgb_bits(palette.rim_top),
        rim_bottom: rgb_bits(palette.rim_bottom),
        sx: sx.to_bits(),
        sy: sy.to_bits(),
    };
    let surface = CACHE.with(|cache| -> Result<ImageSurface, cairo::Error> {
        let mut slot = cache.borrow_mut();
        if let Some((cached_key, surf)) = slot.as_ref()
            && *cached_key == key
        {
            return Ok(surf.clone());
        }
        let dim_x = (RING_CACHE_HALF * 2.0 * sx).ceil() as i32;
        let dim_y = (RING_CACHE_HALF * 2.0 * sy).ceil() as i32;
        let surf = ImageSurface::create(Format::ARgb32, dim_x, dim_y)?;
        surf.set_device_scale(sx, sy);
        {
            let ctx = Context::new(&surf)?;
            ctx.translate(RING_CACHE_HALF, RING_CACHE_HALF);
            draw_ring_rim(&ctx, high_contrast, palette)?;
        }
        *slot = Some((key, surf.clone()));
        Ok(surf)
    })?;
    paint_surface(cr, &surface, -RING_CACHE_HALF, -RING_CACHE_HALF, 1.0)
}

const LABEL_INNER_MARGIN: f64 = 3.0;
const LABEL_OUTER_MARGIN: f64 = 4.5;
const LABEL_RAY_MARGIN: f64 = 3.5;
const MIN_MAIN_FONT: f64 = 9.5;
const MAX_HOVER_ZOOM: f64 = 1.04;

/// Horizontal `[lo, hi]` span on scanline `y` that lies inside the annular sector
/// `[start, end]` with inner/outer/ray padding, picking the interval closest to `ref_x`.
fn sector_horizontal_span_at_y(
    y: f64,
    start: f64,
    end: f64,
    ref_x: f64,
) -> Option<(f64, f64)> {
    let r_in = INNER + LABEL_INNER_MARGIN;
    let r_out = OUTER - LABEL_OUTER_MARGIN;
    if y.abs() >= r_out {
        return None;
    }
    let x_out = (r_out * r_out - y * y).sqrt();
    let mut intervals = [(0.0_f64, 0.0_f64); 2];
    let mut len = if y.abs() < r_in {
        let x_in = (r_in * r_in - y * y).sqrt();
        intervals[0] = (-x_out, -x_in);
        intervals[1] = (x_in, x_out);
        2
    } else {
        intervals[0] = (-x_out, x_out);
        1
    };

    if end - start <= PI + 1e-4 {
        let planes = [
            (-start.sin(), start.cos()),
            (end.sin(), -end.cos()),
        ];
        let mut out_len = 0;
        for i in 0..len {
            let (mut lo, mut hi) = intervals[i];
            for (a, b) in planes {
                let rhs = LABEL_RAY_MARGIN - b * y;
                if a > 1e-9 {
                    lo = lo.max(rhs / a);
                } else if a < -1e-9 {
                    hi = hi.min(rhs / a);
                } else if rhs > 0.0 {
                    lo = hi + 1.0;
                }
            }
            if lo < hi {
                intervals[out_len] = (lo, hi);
                out_len += 1;
            }
        }
        len = out_len;
    }

    intervals[..len].iter().copied().min_by(|&(l1, h1), &(l2, h2)| {
        let dist = |lo: f64, hi: f64| {
            if ref_x < lo {
                lo - ref_x
            } else if ref_x > hi {
                ref_x - hi
            } else {
                0.0
            }
        };
        dist(l1, h1).total_cmp(&dist(l2, h2))
    })
}

/// Safe horizontal `[lo, hi]` range inside sector `[start, end]` across the vertical
/// band `[y_top, y_bot]`.
fn sector_label_span(
    y_top: f64,
    y_bot: f64,
    start: f64,
    end: f64,
    ref_x: f64,
) -> Option<(f64, f64)> {
    let ys = [y_top, y_bot, 0.0];
    let n = if y_top < 0.0 && 0.0 < y_bot { 3 } else { 2 };
    let mut lo = f64::NEG_INFINITY;
    let mut hi = f64::INFINITY;
    for &y in &ys[..n] {
        let (l, h) = sector_horizontal_span_at_y(y, start, end, ref_x)?;
        lo = lo.max(l);
        hi = hi.min(h);
    }
    (lo < hi).then_some((lo, hi))
}

#[derive(Clone, Copy)]
struct ItemStyle {
    horizontal_radius: f64,
    plate: f64,
    icon_size: f64,
    icon_y: f64,
    label_y: f64,
    base_font: f64,
    max_label_width: f64,
    shrink_to_fit: bool,
}

fn item_style(main: bool, count: usize) -> ItemStyle {
    if main && count <= 3 {
        // Longer home labels need clearance where the rim curves inward below them.
        ItemStyle {
            horizontal_radius: ITEM_RADIUS - 8.0,
            plate: 41.0,
            icon_size: 26.0,
            icon_y: -12.0,
            label_y: 22.0,
            base_font: 15.0,
            max_label_width: 148.0,
            shrink_to_fit: true,
        }
    } else if main && count <= 5 {
        ItemStyle {
            horizontal_radius: ITEM_RADIUS,
            plate: 36.0,
            icon_size: 23.0,
            icon_y: -10.5,
            label_y: 19.5,
            base_font: 13.5,
            max_label_width: 148.0,
            shrink_to_fit: true,
        }
    } else if main {
        ItemStyle {
            horizontal_radius: ITEM_RADIUS,
            plate: 33.0,
            icon_size: 21.0,
            icon_y: -9.5,
            label_y: 18.0,
            base_font: 12.5,
            max_label_width: 148.0,
            shrink_to_fit: true,
        }
    } else {
        ItemStyle {
            horizontal_radius: ITEM_RADIUS,
            plate: 32.0,
            icon_size: 20.0,
            icon_y: -9.0,
            label_y: 17.5,
            base_font: 12.5,
            max_label_width: if count <= 3 { 104.0 } else { 83.0 },
            shrink_to_fit: false,
        }
    }
}

#[cfg(test)]
fn fit_item_label(
    cr: &Context,
    label: &str,
    sector_bounds: (f64, f64),
    origin: (f64, f64),
    style: ItemStyle,
    zoom: f64,
) -> (gtk::pango::Layout, f64, f64) {
    let (start, end) = sector_bounds;
    let (origin_x, origin_y) = origin;
    let mut layout = text_layout(cr, label, style.base_font, true);
    let (mut text_w, text_h) = layout.pixel_size();
    let zoom_ref = zoom.max(MAX_HOVER_ZOOM);
    let y_center = origin_y + style.label_y * zoom_ref;
    let half_h = (f64::from(text_h) / 2.0 + 1.0) * zoom_ref;

    let local_span = sector_label_span(y_center - half_h, y_center + half_h, start, end, origin_x)
        .map(|(lo, hi)| ((lo - origin_x) / zoom_ref, (hi - origin_x) / zoom_ref));

    let Some((local_lo, local_hi)) = local_span else {
        return (layout, 0.0, style.max_label_width.min(104.0));
    };

    let avail_w = (local_hi - local_lo).max(0.0);
    let width = avail_w.min(style.max_label_width);
    if style.shrink_to_fit && f64::from(text_w) > width && width > 0.0 {
        let mut size = (style.base_font * (width / f64::from(text_w)))
            .clamp(MIN_MAIN_FONT, style.base_font);
        layout = text_layout(cr, label, size, true);
        text_w = layout.pixel_size().0;
        while f64::from(text_w) > width && size > MIN_MAIN_FONT {
            size = (size - 0.5).max(MIN_MAIN_FONT);
            layout = text_layout(cr, label, size, true);
            text_w = layout.pixel_size().0;
        }
    }

    let draw_w = f64::from(text_w).min(width);
    let label_x = if local_lo <= -draw_w / 2.0 && draw_w / 2.0 <= local_hi {
        0.0
    } else if draw_w < avail_w {
        0.0_f64.clamp(local_lo + draw_w / 2.0, local_hi - draw_w / 2.0)
    } else {
        (local_lo + local_hi) * 0.5
    };
    (layout, label_x, width)
}

fn fit_item_label_cached(
    label: &str,
    sector_bounds: (f64, f64),
    origin: (f64, f64),
    style: ItemStyle,
    zoom: f64,
) -> (f64, f64, f64) {
    let (start, end) = sector_bounds;
    let (origin_x, origin_y) = origin;
    let mut size = style.base_font;
    let (mut text_w, text_h) = measure_text(label, size, true);
    let zoom_ref = zoom.max(MAX_HOVER_ZOOM);
    let y_center = origin_y + style.label_y * zoom_ref;
    let half_h = (f64::from(text_h) / 2.0 + 1.0) * zoom_ref;

    let local_span = sector_label_span(y_center - half_h, y_center + half_h, start, end, origin_x)
        .map(|(lo, hi)| ((lo - origin_x) / zoom_ref, (hi - origin_x) / zoom_ref));

    let Some((local_lo, local_hi)) = local_span else {
        return (size, 0.0, style.max_label_width.min(104.0));
    };

    let avail_w = (local_hi - local_lo).max(0.0);
    let width = avail_w.min(style.max_label_width);
    if style.shrink_to_fit && f64::from(text_w) > width && width > 0.0 {
        size = (style.base_font * (width / f64::from(text_w)))
            .clamp(MIN_MAIN_FONT, style.base_font);
        text_w = measure_text(label, size, true).0;
        while f64::from(text_w) > width && size > MIN_MAIN_FONT {
            size = (size - 0.5).max(MIN_MAIN_FONT);
            text_w = measure_text(label, size, true).0;
        }
    }

    let draw_w = f64::from(text_w).min(width);
    let label_x = if local_lo <= -draw_w / 2.0 && draw_w / 2.0 <= local_hi {
        0.0
    } else if draw_w < avail_w {
        0.0_f64.clamp(local_lo + draw_w / 2.0, local_hi - draw_w / 2.0)
    } else {
        (local_lo + local_hi) * 0.5
    };
    (size, label_x, width)
}

fn draw_icon_foreground(
    cr: &Context,
    item: &Item,
    y: f64,
    plate: f64,
    icon_size: f64,
    palette: &Palette,
    alpha: f64,
) -> Result<(), cairo::Error> {
    let tint = palette.accent(item.icon);
    cr.save()?;
    if let Some(image) = &item.pixbuf {
        let width = f64::from(image.width());
        let height = f64::from(image.height());
        let scale = (plate - 5.0) / width.max(height);
        cr.translate(0.0, y);
        cr.scale(scale, scale);
        if let Ok(surface) = pixbuf_surface(image) {
            paint_surface(cr, &surface, -width / 2.0, -height / 2.0, alpha)?;
        } else {
            cr.set_source_pixbuf(image, -width / 2.0, -height / 2.0);
            cr.source().set_filter(cairo::Filter::Bilinear);
            if alpha < 1.0 {
                cr.paint_with_alpha(alpha)?;
            } else {
                cr.paint()?;
            }
        }
    } else if let Icon::ColorSwatch(r, g, b) = item.icon {
        cr.translate(-icon_size / 2.0, y - icon_size / 2.0);
        cr.scale(icon_size / 24.0, icon_size / 24.0);
        cr.new_path();
        cr.arc(12.0, 12.0, 7.8, 0.0, TAU);
        cr.set_source_rgba(
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
            alpha,
        );
        cr.fill_preserve()?;
        color(cr, palette.text_primary, 0.55 * alpha);
        cr.set_line_width(1.6);
        cr.stroke()?;
    } else {
        cr.translate(-icon_size / 2.0, y - icon_size / 2.0);
        cr.scale(icon_size / 24.0, icon_size / 24.0);
        color(cr, tint, alpha);
        glyph(cr, item.icon)?;
    }
    cr.restore()
}

fn draw_icon_plate_uncached(
    cr: &Context,
    item: &Item,
    y: f64,
    plate: f64,
    icon_size: f64,
    palette: &Palette,
    alpha: f64,
) -> Result<(), cairo::Error> {
    let tint = palette.accent(item.icon);
    // Draw shell background, then foreground, then active dot on top.
    rounded_rect(
        cr,
        -plate / 2.0,
        y - plate / 2.0,
        plate,
        plate,
        plate * 0.29,
    );
    let plate_glass = LinearGradient::new(0.0, y - plate / 2.0, 0.0, y + plate / 2.0);
    plate_glass.add_color_stop_rgba(0.0, tint.0, tint.1, tint.2, 0.21 * alpha);
    plate_glass.add_color_stop_rgba(1.0, tint.0, tint.1, tint.2, 0.07 * alpha);
    cr.set_source(&plate_glass)?;
    cr.fill_preserve()?;
    color(cr, tint, 0.42 * alpha);
    cr.set_line_width(0.8);
    cr.stroke()?;
    draw_icon_foreground(cr, item, y, plate, icon_size, palette, alpha)?;
    if item.active {
        let dot_x = plate / 2.0 - 2.0;
        let dot_y = y - plate / 2.0 + 2.0;
        cr.new_sub_path();
        cr.arc(dot_x, dot_y, 3.8, 0.0, TAU);
        color(cr, tint, 0.98 * alpha);
        cr.fill_preserve()?;
        cr.set_line_width(1.2);
        color(cr, palette.ring_mid, 0.95 * alpha);
        cr.stroke()?;
    }
    Ok(())
}

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct IconPlateKey {
    icon: Icon,
    pixbuf_ptr: usize,
    active: bool,
    plate_bits: u64,
    icon_size_bits: u64,
    tint: [u64; 3],
    ring_mid: [u64; 3],
    text_primary: [u64; 3],
    sx: u64,
    sy: u64,
}

struct CachedIconPlate {
    pixbuf: Option<glib::WeakRef<Pixbuf>>,
    surface: ImageSurface,
    half: f64,
}

fn cached_icon_plate_surface(
    cr: &Context,
    item: &Item,
    plate: f64,
    icon_size: f64,
    palette: &Palette,
) -> Result<(ImageSurface, f64), cairo::Error> {
    thread_local! {
        static CACHE: RefCell<HashMap<IconPlateKey, CachedIconPlate>> =
            RefCell::new(HashMap::new());
    }
    let (sx, sy) = surface_device_scale(cr);
    let tint = palette.accent(item.icon);
    let key = IconPlateKey {
        icon: item.icon,
        pixbuf_ptr: item.pixbuf.as_ref().map_or(0, |pb| pb.as_ptr() as usize),
        active: item.active,
        plate_bits: plate.to_bits(),
        icon_size_bits: icon_size.to_bits(),
        tint: rgb_bits(tint),
        ring_mid: rgb_bits(palette.ring_mid),
        text_primary: rgb_bits(palette.text_primary),
        sx: sx.to_bits(),
        sy: sy.to_bits(),
    };
    CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if let Some(entry) = map.get(&key) {
            let valid = match (&item.pixbuf, &entry.pixbuf) {
                (Some(pb), Some(w)) => w.upgrade().is_some_and(|alive| alive == *pb),
                (None, None) => true,
                _ => false,
            };
            if valid {
                return Ok((entry.surface.clone(), entry.half));
            }
        }
        // Tight bounding half-size reduces rotated bilinear pixel count (~20% fewer pixels).
        let pad = if item.active { 3.5 } else { 1.5 };
        let half = (plate * 0.5 + pad).ceil();
        let dim_x = (half * 2.0 * sx).ceil() as i32;
        let dim_y = (half * 2.0 * sy).ceil() as i32;
        let surf = ImageSurface::create(Format::ARgb32, dim_x, dim_y)?;
        surf.set_device_scale(sx, sy);
        {
            let ctx = Context::new(&surf)?;
            ctx.translate(half, half);
            draw_icon_plate_uncached(&ctx, item, 0.0, plate, icon_size, palette, 1.0)?;
        }
        if map.len() >= 128 {
            map.retain(|_, entry| entry.pixbuf.as_ref().is_none_or(|w| w.upgrade().is_some()));
            if map.len() >= 128 {
                map.clear();
            }
        }
        map.insert(
            key,
            CachedIconPlate {
                pixbuf: item.pixbuf.as_ref().map(ObjectExt::downgrade),
                surface: surf.clone(),
                half,
            },
        );
        Ok((surf, half))
    })
}

fn draw_icon_plate(
    cr: &Context,
    item: &Item,
    y: f64,
    plate: f64,
    icon_size: f64,
    palette: &Palette,
    alpha: f64,
) -> Result<(), cairo::Error> {
    if alpha <= 0.004 {
        return Ok(());
    }
    // Snap ring/settled plate sizes to their exact canonical key so late morph
    // frames never allocate throwaway cache entries, and reuse the canonical
    // 48px box plate (single blit) while `plate` interpolates across 32..48.
    const BOX_PLATE: f64 = 48.0;
    const BOX_ICON: f64 = 26.0;
    let canonical = [
        (32.0, 20.0),
        (33.0, 21.0),
        (36.0, 23.0),
        (41.0, 26.0),
        (BOX_PLATE, BOX_ICON),
    ]
    .into_iter()
    .find(|&(p, i)| (plate - p).abs() < 0.05 && (icon_size - i).abs() < 0.05);
    if let Some((cp, ci)) = canonical {
        let (surf, half) = cached_icon_plate_surface(cr, item, cp, ci, palette)?;
        return paint_surface(cr, &surf, -half, y - half, alpha);
    }

    let (surf, half) = cached_icon_plate_surface(cr, item, BOX_PLATE, BOX_ICON, palette)?;
    let s = plate / BOX_PLATE;
    cr.save()?;
    cr.translate(0.0, y);
    cr.scale(s, s);
    paint_surface(cr, &surf, -half, -half, alpha)?;
    cr.restore()
}

#[allow(clippy::too_many_arguments)]
fn draw_item(
    cr: &Context,
    item: &Item,
    start: f64,
    end: f64,
    count: usize,
    pressed: bool,
    focus: f64,
    tilt: f64,
    palette: &Palette,
    alpha: f64,
) -> Result<(), cairo::Error> {
    cr.save()?;
    let angle = (start + end) * 0.5;
    let main = matches!(
        item.icon,
        Icon::Files
            | Icon::Applications
            | Icon::Windows
            | Icon::Wifi
            | Icon::Bluetooth
            | Icon::CustomMenu
    );
    let style = item_style(main, count);
    let origin_x = style.horizontal_radius * angle.cos();
    let origin_y = ITEM_RADIUS * angle.sin();
    cr.translate(origin_x, origin_y);
    let zoom = if pressed { 0.96 } else { 1.0 + 0.04 * focus };
    if (zoom - 1.0).abs() >= 1e-4 {
        cr.scale(zoom, zoom);
    }
    cr.save()?;
    // Below 0.003 rad (~0.17°), displacement at the plate corner is <0.07 px;
    // skipping rotation keeps Pixman on its ~5× faster unrotated SSE2 path.
    if tilt.abs() >= 0.003 {
        cr.rotate(tilt);
    }
    draw_icon_plate(
        cr,
        item,
        style.icon_y,
        style.plate,
        style.icon_size,
        palette,
        alpha,
    )?;
    cr.restore()?;

    let (font_size, label_x, width) = fit_item_label_cached(
        &item.label,
        (start, end),
        (origin_x, origin_y),
        style,
        zoom,
    );
    draw_cached_centered_text(
        cr,
        &item.label,
        font_size,
        true,
        (label_x, style.label_y),
        width,
        palette.text_primary,
        palette.text_shadow,
        alpha,
    )?;
    cr.restore()
}

const PERSPECTIVE_D: f64 = 1050.0;
const RING_HALF_THICKNESS: f64 = 11.0;
const RING_SEGMENTS: usize = 64;

#[derive(Clone, Copy)]
struct Vec3(f64, f64, f64);

impl Vec3 {
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0, self.1 + other.1, self.2 + other.2)
    }

    fn scale(self, s: f64) -> Self {
        Self(self.0 * s, self.1 * s, self.2 * s)
    }

    fn dot(self, other: Self) -> f64 {
        self.0 * other.0 + self.1 * other.1 + self.2 * other.2
    }

    fn cross(self, other: Self) -> Self {
        Self(
            self.1 * other.2 - self.2 * other.1,
            self.2 * other.0 - self.0 * other.2,
            self.0 * other.1 - self.1 * other.0,
        )
    }

    fn normalize(self) -> Self {
        let len = self.dot(self).sqrt();
        if len > 0.0 {
            Self(self.0 / len, self.1 / len, self.2 / len)
        } else {
            Self(0.0, 0.0, 1.0)
        }
    }
}

struct Transform3D {
    r: [[f64; 3]; 3],
}

impl Transform3D {
    fn from_pose(pose: LoadingPose) -> Self {
        Self { r: pose.rotation }
    }

    fn rotate(&self, v: Vec3) -> Vec3 {
        Vec3(
            self.r[0][0] * v.0 + self.r[0][1] * v.1 + self.r[0][2] * v.2,
            self.r[1][0] * v.0 + self.r[1][1] * v.1 + self.r[1][2] * v.2,
            self.r[2][0] * v.0 + self.r[2][1] * v.1 + self.r[2][2] * v.2,
        )
    }

    fn project_world(w: Vec3) -> (f64, f64, f64) {
        let persp = PERSPECTIVE_D / (PERSPECTIVE_D - w.2);
        (w.0 * persp, w.1 * persp, w.2)
    }

    fn project(&self, p: Vec3) -> (f64, f64, f64) {
        Self::project_world(self.rotate(p))
    }

    fn cam_local(&self) -> Vec3 {
        Vec3(
            self.r[2][0] * PERSPECTIVE_D,
            self.r[2][1] * PERSPECTIVE_D,
            self.r[2][2] * PERSPECTIVE_D,
        )
    }
}

fn lerp_rgb(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

fn trace_polygon(cr: &Context, pts: &[(f64, f64)]) {
    if let Some(&(x0, y0)) = pts.first() {
        cr.move_to(x0, y0);
        for &(x, y) in &pts[1..] {
            cr.line_to(x, y);
        }
        cr.close_path();
    }
}

fn convex_hull(mut pts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    pts.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut lower: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

struct ConvexScanner {
    left: Vec<(f64, f64)>,
    right: Vec<(f64, f64)>,
    li: usize,
    ri: usize,
    min_y: f64,
    max_y: f64,
}

impl ConvexScanner {
    fn new(poly: &[(f64, f64)]) -> Option<Self> {
        let n = poly.len();
        if n < 3 {
            return None;
        }
        let mut top = 0;
        let mut bot = 0;
        for i in 1..n {
            if poly[i].1 < poly[top].1 {
                top = i;
            }
            if poly[i].1 > poly[bot].1 {
                bot = i;
            }
        }
        let min_y = poly[top].1;
        let max_y = poly[bot].1;
        if min_y.partial_cmp(&max_y) != Some(std::cmp::Ordering::Less) {
            return None;
        }
        let mut left = Vec::with_capacity(n / 2 + 2);
        let mut i = top;
        loop {
            left.push(poly[i]);
            if i == bot {
                break;
            }
            i = (i + 1) % n;
        }
        let mut right = Vec::with_capacity(n / 2 + 2);
        let mut i = top;
        loop {
            right.push(poly[i]);
            if i == bot {
                break;
            }
            i = (i + n - 1) % n;
        }
        Some(Self {
            left,
            right,
            li: 0,
            ri: 0,
            min_y,
            max_y,
        })
    }

    fn span(&mut self, y: f64) -> Option<(f64, f64)> {
        if y < self.min_y || y >= self.max_y {
            return None;
        }
        while self.li + 1 < self.left.len() && self.left[self.li + 1].1 <= y {
            self.li += 1;
        }
        while self.ri + 1 < self.right.len() && self.right[self.ri + 1].1 <= y {
            self.ri += 1;
        }
        if self.li + 1 >= self.left.len() || self.ri + 1 >= self.right.len() {
            return None;
        }
        let (lx0, ly0) = self.left[self.li];
        let (lx1, ly1) = self.left[self.li + 1];
        let (rx0, ry0) = self.right[self.ri];
        let (rx1, ry1) = self.right[self.ri + 1];
        if ly1.partial_cmp(&ly0) != Some(std::cmp::Ordering::Greater)
            || ry1.partial_cmp(&ry0) != Some(std::cmp::Ordering::Greater)
        {
            return None;
        }
        let x_a = lx0 + ((y - ly0) / (ly1 - ly0)) * (lx1 - lx0);
        let x_b = rx0 + ((y - ry0) / (ry1 - ry0)) * (rx1 - rx0);
        let (lo, hi) = if x_a <= x_b { (x_a, x_b) } else { (x_b, x_a) };
        (lo < hi).then_some((lo, hi))
    }
}

fn blur_region_3d(
    geom: &Geometry,
    scale: f64,
    outer_hull: &[(f64, f64)],
    inner_front: &[(f64, f64, f64)],
    inner_back: &[(f64, f64, f64)],
) -> Region {
    let outer_s: Vec<(f64, f64)> = outer_hull
        .iter()
        .map(|&(x, y)| (geom.center_x + x * scale, geom.center_y + y * scale))
        .collect();
    let Some(mut outer_scan) = ConvexScanner::new(&outer_s) else {
        return Region::create();
    };
    let inner_f_s: Vec<(f64, f64)> = inner_front
        .iter()
        .map(|&(x, y, _)| (geom.center_x + x * scale, geom.center_y + y * scale))
        .collect();
    let inner_b_s: Vec<(f64, f64)> = inner_back
        .iter()
        .map(|&(x, y, _)| (geom.center_x + x * scale, geom.center_y + y * scale))
        .collect();
    let mut inner_f_scan = ConvexScanner::new(&inner_f_s);
    let mut inner_b_scan = ConvexScanner::new(&inner_b_s);

    let first_pixel = |edge: f64| (edge - 0.5).ceil() as i32;
    let y0 = first_pixel(outer_scan.min_y);
    let y1 = first_pixel(outer_scan.max_y);
    let mut rows = Vec::with_capacity((y1 - y0).max(0) as usize * 2);
    for y in y0..y1 {
        let y_mid = f64::from(y) + 0.5;
        let Some((out_l, out_r)) = outer_scan.span(y_mid) else {
            continue;
        };
        let hole = match (
            inner_f_scan.as_mut().and_then(|s| s.span(y_mid)),
            inner_b_scan.as_mut().and_then(|s| s.span(y_mid)),
        ) {
            (Some((l1, r1)), Some((l2, r2))) => {
                let hl = l1.max(l2);
                let hr = r1.min(r2);
                (hr > hl).then_some((hl, hr))
            }
            _ => None,
        };
        let ol = first_pixel(out_l);
        let or = first_pixel(out_r);
        if let Some((hl, hr)) = hole {
            let hl_px = first_pixel(hl).clamp(ol, or);
            let hr_px = first_pixel(hr).clamp(ol, or);
            if hl_px > ol {
                rows.push(RectangleInt::new(ol, y, hl_px - ol, 1));
            }
            if or > hr_px {
                rows.push(RectangleInt::new(hr_px, y, or - hr_px, 1));
            }
        } else if or > ol {
            rows.push(RectangleInt::new(ol, y, or - ol, 1));
        }
    }
    Region::create_rectangles(&rows)
}

fn draw_sector_chrome(
    cr: &Context,
    sector_view: &crate::motion::Sector<'_>,
    view: &View<'_>,
    palette: &Palette,
    alpha: f64,
) -> Result<(), cairo::Error> {
    let slot = sector_view.slot;
    let (start, end) = (sector_view.start, sector_view.end);
    let span = end - start;
    let pressed = slot.index.is_some() && slot.index == view.pressed;
    let focus = slot.focus().clamp(0.0, 1.0);
    if (slot.focused() || pressed)
        && let Some(face) = slot.faces.last()
    {
        let tint = palette.accent(face.item.icon);
        sector(cr, start, end);
        color(
            cr,
            tint,
            if pressed { 0.23 } else { 0.05 + 0.08 * focus } * alpha,
        );
        cr.fill()?;
        if span >= TAU - 0.0001 {
            cr.arc(0.0, 0.0, OUTER - 2.6, 0.0, TAU);
            cr.close_path();
        } else {
            let inset = 0.026_f64.min(span * 0.25).min((TAU - span) * 0.5);
            cr.arc(0.0, 0.0, OUTER - 2.6, start + inset, end - inset);
        }
        cr.set_line_cap(LineCap::Round);
        cr.set_line_width(if view.high_contrast { 3.5 } else { 2.6 });
        color(
            cr,
            tint,
            if pressed { 0.98 } else { 0.45 + 0.53 * focus } * alpha,
        );
        cr.stroke()?;
    }
    if span < TAU - 0.0001 {
        cr.move_to((INNER + 1.0) * start.cos(), (INNER + 1.0) * start.sin());
        cr.line_to((OUTER - 1.0) * start.cos(), (OUTER - 1.0) * start.sin());
        let opacity = slot
            .faces
            .iter()
            .map(|face| face.opacity)
            .sum::<f64>()
            .min(1.0);
        color(
            cr,
            palette.divider,
            opacity * if view.high_contrast { 0.72 } else { 0.19 } * alpha,
        );
        cr.set_line_width(1.0);
        cr.stroke()?;
    }
    Ok(())
}

fn draw_sectors(
    cr: &Context,
    view: &View<'_>,
    palette: &Palette,
    alpha: f64,
) -> Result<(), cairo::Error> {
    if alpha <= 0.004 {
        return Ok(());
    }
    let count = view.count;
    for sector_view in view.motion.sectors() {
        let slot = sector_view.slot;
        if slot.born_in_box() {
            continue;
        }
        let (start, end) = (sector_view.start, sector_view.end);
        let span = end - start;
        let pressed = slot.index.is_some() && slot.index == view.pressed;
        draw_sector_chrome(cr, &sector_view, view, palette, alpha)?;
        // Wide active sectors (up to 8 items: TAU/8 ≈ 0.785) already fit their
        // icon plates and fitted labels inside the wedge; skip Cairo's expensive
        // curved arc clip mask unless a wedge is shrinking or narrow.
        let needs_sector_clip = span < 0.72 || slot.index.is_none();
        if needs_sector_clip {
            cr.save()?;
            sector(cr, start, end);
            cr.clip();
        }
        for face in &slot.faces {
            let face_alpha = face.opacity * alpha;
            if face_alpha <= 0.008 {
                continue;
            }
            draw_item(
                cr,
                &face.item,
                start,
                end,
                count,
                pressed,
                slot.focus(),
                slot.tilt(),
                palette,
                face_alpha,
            )?;
        }
        if needs_sector_clip {
            cr.restore()?;
        }
        cr.new_path();
    }
    Ok(())
}

static RING_COS_SIN: LazyLock<[(f64, f64); RING_SEGMENTS]> = LazyLock::new(|| {
    std::array::from_fn(|i| {
        let a = TAU * i as f64 / RING_SEGMENTS as f64;
        (a.cos(), a.sin())
    })
});

static RING_MID_COS_SIN: LazyLock<[(f64, f64); RING_SEGMENTS]> = LazyLock::new(|| {
    std::array::from_fn(|i| {
        let a = TAU * (i as f64 + 0.5) / RING_SEGMENTS as f64;
        (a.cos(), a.sin())
    })
});

fn draw_loading_ring_3d(
    cr: &Context,
    geom: &Geometry,
    scale: f64,
    view: &View<'_>,
    material: Material,
    pose: LoadingPose,
) -> Result<Region, cairo::Error> {
    let palette = view.palette;
    let accent = view.loading_accent.unwrap_or(palette.rim_top);
    let tf = Transform3D::from_pose(pose);
    let cam = tf.cam_local();
    let half_h = RING_HALF_THICKNESS * pose.depth;

    let mut outer_front = Vec::with_capacity(RING_SEGMENTS);
    let mut outer_back = Vec::with_capacity(RING_SEGMENTS);
    let mut inner_front = Vec::with_capacity(RING_SEGMENTS);
    let mut inner_back = Vec::with_capacity(RING_SEGMENTS);
    let radius_scale = 1.0 - 0.025 * pose.depth;
    let outer_r = OUTER * radius_scale;
    let inner_r = INNER * radius_scale;

    for &(ca, sa) in RING_COS_SIN.iter() {
        outer_front.push(tf.project(Vec3(outer_r * ca, outer_r * sa, half_h)));
        outer_back.push(tf.project(Vec3(outer_r * ca, outer_r * sa, -half_h)));
        inner_front.push(tf.project(Vec3(inner_r * ca, inner_r * sa, half_h)));
        inner_back.push(tf.project(Vec3(inner_r * ca, inner_r * sa, -half_h)));
    }

    let mut hull_input = Vec::with_capacity(RING_SEGMENTS * 2);
    for &(x, y, _) in &outer_front {
        hull_input.push((x, y));
    }
    for &(x, y, _) in &outer_back {
        hull_input.push((x, y));
    }
    let outer_hull = convex_hull(hull_input);
    let (mut hx0, mut hy0, mut hx1, mut hy1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for &(x, y) in &outer_hull {
        hx0 = hx0.min(x);
        hy0 = hy0.min(y);
        hx1 = hx1.max(x);
        hy1 = hy1.max(y);
    }

    // Graduated shadow outside the 3D silhouette leaves the glass and center hole clear.
    cr.set_line_width(2.8);
    cr.set_line_join(LineJoin::Bevel);
    for (expand, weight) in [(1.4, 0.85), (4.0, 0.50), (6.6, 0.18)] {
        cr.new_path();
        for (idx, &(x, y)) in outer_hull.iter().enumerate() {
            let r = x.hypot(y);
            let s = if r > 1e-3 { (r + expand) / r } else { 1.0 };
            let px = x * s;
            let py = y * s + 1.0;
            if idx == 0 {
                cr.move_to(px, py);
            } else {
                cr.line_to(px, py);
            }
        }
        cr.close_path();
        color(cr, palette.shadow, 0.13 * weight);
        cr.stroke()?;
    }

    let light = Vec3(-0.42, -0.72, 0.80).normalize();
    let half_vec = Vec3(light.0, light.1, light.2 + 1.0).normalize();
    let front_is_pos = cam.2 >= 0.0;

    let shade_outer = |ca: f64, sa: f64| -> Rgb {
        let n = tf.rotate(Vec3(ca, sa, 0.0));
        let diff = n.dot(light).max(0.0);
        let spec = n.dot(half_vec).max(0.0).powf(12.0);
        let base = lerp_rgb(palette.ring_bottom, palette.ring_top, 0.20 + 0.80 * diff);
        let lit = lerp_rgb(base, palette.rim_top, 0.28 * diff + 0.48 * spec);
        lerp_rgb(lit, accent, (0.22 * diff + 0.45 * spec) * pose.depth)
    };
    let shade_inner = |ca: f64, sa: f64| -> Rgb {
        let n = tf.rotate(Vec3(-ca, -sa, 0.0));
        let diff = n.dot(light).max(0.0);
        let spec = n.dot(half_vec).max(0.0).powf(10.0);
        let base = lerp_rgb(palette.ring_mid, palette.ring_top, 0.12 + 0.72 * diff);
        let lit = lerp_rgb(base, palette.rim_bottom, 0.20 * diff + 0.34 * spec);
        lerp_rgb(lit, accent, (0.16 * diff + 0.32 * spec) * pose.depth)
    };

    if half_h > 0.05 && hx0.is_finite() {
        struct WallQuad {
            z_avg: f64,
            pts: [(f64, f64); 4],
            rgb: Rgb,
        }
        let mut quads = Vec::with_capacity(RING_SEGMENTS * 2);

        for (i, &(ca, sa)) in RING_MID_COS_SIN.iter().enumerate() {
            let j = (i + 1) % RING_SEGMENTS;

            // Outer cylindrical wall.
            if cam.0 * ca + cam.1 * sa - outer_r > -4.0 {
                let (p0, p1, p2, p3) = (outer_front[i], outer_front[j], outer_back[j], outer_back[i]);
                quads.push(WallQuad {
                    z_avg: 0.25 * (p0.2 + p1.2 + p2.2 + p3.2),
                    pts: [(p0.0, p0.1), (p1.0, p1.1), (p2.0, p2.1), (p3.0, p3.1)],
                    rgb: shade_outer(ca, sa),
                });
            }

            // Inner cylindrical wall (the 3D hole thickness).
            if -(cam.0 * ca + cam.1 * sa) + inner_r > -4.0 {
                let (p0, p1, p2, p3) = (inner_front[i], inner_front[j], inner_back[j], inner_back[i]);
                quads.push(WallQuad {
                    z_avg: 0.25 * (p0.2 + p1.2 + p2.2 + p3.2),
                    pts: [(p0.0, p0.1), (p1.0, p1.1), (p2.0, p2.1), (p3.0, p3.1)],
                    rgb: shade_inner(ca, sa),
                });
            }
        }

        quads.sort_by(|a, b| a.z_avg.total_cmp(&b.z_avg));

        let draw_quads = |target: &Context| -> Result<(), cairo::Error> {
            for q in &quads {
                // Overlap adjacent circumferential edges slightly to prevent AA seams
                // without stroking every quad or protruding beyond silhouettes.
                let dt = (q.pts[1].0 - q.pts[0].0, q.pts[1].1 - q.pts[0].1);
                let lt = dt.0.hypot(dt.1);
                let ft = if lt > 1e-3 { (0.45 / lt).min(0.15) } else { 0.0 };
                let db = (q.pts[2].0 - q.pts[3].0, q.pts[2].1 - q.pts[3].1);
                let lb = db.0.hypot(db.1);
                let fb = if lb > 1e-3 { (0.45 / lb).min(0.15) } else { 0.0 };

                target.new_path();
                target.move_to(q.pts[0].0 - dt.0 * ft, q.pts[0].1 - dt.1 * ft);
                target.line_to(q.pts[1].0 + dt.0 * ft, q.pts[1].1 + dt.1 * ft);
                target.line_to(q.pts[2].0 + db.0 * fb, q.pts[2].1 + db.0 * fb);
                target.line_to(q.pts[3].0 - db.0 * fb, q.pts[3].1 - db.0 * fb);
                target.close_path();
                target.set_source_rgb(q.rgb.0, q.rgb.1, q.rgb.2);
                target.fill()?;
            }
            Ok(())
        };

        let wall_alpha = material.opacity(0.80, 0.88);
        if wall_alpha < 1.0 {
            // Reuse a persistent scratch ImageSurface in exact 1:1 device pixels
            // instead of allocating and freeing an offscreen surface via `push_group()`
            // on every frame of the 3D spin.
            let (sx, sy) = surface_device_scale(cr);
            let mat = cr.matrix();
            let corners = [
                (hx0 - 4.0, hy0 - 4.0),
                (hx1 + 4.0, hy0 - 4.0),
                (hx0 - 4.0, hy1 + 4.0),
                (hx1 + 4.0, hy1 + 4.0),
            ];
            let (mut px0, mut py0, mut px1, mut py1) = (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            );
            for (ux, uy) in corners {
                let (lx, ly) = mat.transform_point(ux, uy);
                let dx = lx * sx;
                let dy = ly * sy;
                px0 = px0.min(dx);
                py0 = py0.min(dy);
                px1 = px1.max(dx);
                py1 = py1.max(dy);
            }
            let ix0 = px0.floor() as i32;
            let iy0 = py0.floor() as i32;
            let pw = (px1.ceil() as i32 - ix0).max(0);
            let ph = (py1.ceil() as i32 - iy0).max(0);
            if pw > 0 && ph > 0 {
                thread_local! {
                    static WALL_SCRATCH: RefCell<Option<ImageSurface>> = const { RefCell::new(None) };
                }
                let scratch = WALL_SCRATCH.with(|cell| -> Result<ImageSurface, cairo::Error> {
                    let mut slot = cell.borrow_mut();
                    let reuse = slot
                        .as_ref()
                        .is_some_and(|s| s.width() >= pw && s.height() >= ph);
                    if !reuse {
                        let alloc_w = pw.max(slot.as_ref().map_or(0, ImageSurface::width));
                        let alloc_h = ph.max(slot.as_ref().map_or(0, ImageSurface::height));
                        *slot = Some(ImageSurface::create(Format::ARgb32, alloc_w, alloc_h)?);
                    }
                    Ok(slot.as_ref().unwrap().clone())
                })?;
                {
                    let ctx = Context::new(&scratch)?;
                    ctx.rectangle(0.0, 0.0, f64::from(pw), f64::from(ph));
                    ctx.clip();
                    ctx.set_operator(cairo::Operator::Source);
                    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.0);
                    ctx.paint()?;
                    ctx.set_operator(cairo::Operator::Over);
                    ctx.translate(-f64::from(ix0), -f64::from(iy0));
                    ctx.scale(sx, sy);
                    ctx.transform(mat);
                    draw_quads(&ctx)?;
                }
                cr.save()?;
                cr.identity_matrix();
                cr.rectangle(
                    f64::from(ix0) / sx,
                    f64::from(iy0) / sy,
                    f64::from(pw) / sx,
                    f64::from(ph) / sy,
                );
                cr.clip();
                cr.scale(1.0 / sx, 1.0 / sy);
                cr.set_source_surface(&scratch, f64::from(ix0), f64::from(iy0))?;
                cr.source().set_filter(cairo::Filter::Nearest);
                cr.paint_with_alpha(wall_alpha)?;
                cr.restore()?;
            }
        } else {
            draw_quads(cr)?;
        }
    }

    // Near flat glass face (only visible when camera is strictly outside the thickness slab).
    if cam.2.abs() > half_h {
        let face_outer: Vec<(f64, f64)> = if front_is_pos {
            outer_front.iter().map(|&(x, y, _)| (x, y)).collect()
        } else {
            outer_back.iter().map(|&(x, y, _)| (x, y)).collect()
        };
        let face_inner: Vec<(f64, f64)> = if front_is_pos {
            inner_front.iter().map(|&(x, y, _)| (x, y)).collect()
        } else {
            inner_back.iter().map(|&(x, y, _)| (x, y)).collect()
        };

        let trace_face = |cr: &Context| {
            cr.new_path();
            trace_polygon(cr, &face_outer);
            cr.new_sub_path();
            trace_polygon(cr, &face_inner);
        };

        // Upright tangent basis in the visible face plane so lighting and settling
        // sectors stay screen-aligned regardless of arbitrary 3D tumble or roll.
        let face_n = tf.rotate(Vec3(0.0, 0.0, if front_is_pos { 1.0 } else { -1.0 }));
        let face_u = Vec3(
            1.0 - face_n.0 * face_n.0,
            -face_n.0 * face_n.1,
            -face_n.0 * face_n.2,
        )
        .normalize();
        let face_v = face_n.cross(face_u);
        let face_center = face_n.scale(half_h);
        let project_face = |fx: f64, fy: f64| {
            Transform3D::project_world(face_center.add(face_u.scale(fx)).add(face_v.scale(fy)))
        };

        let gy_top = if hy0.is_finite() { hy0 } else { -OUTER };
        let gy_bot = if hy1.is_finite() && hy1 > gy_top + 1.0 {
            hy1
        } else {
            gy_top + 2.0 * OUTER
        };
        // Screen-aligned vertical gradients use Pixman's scanline fast path (~5× faster than diagonal).
        let glass = LinearGradient::new(0.0, gy_top, 0.0, gy_bot);
        for (t, rgb, alpha) in [
            (0.0, palette.ring_top, material.opacity(0.61, 0.71)),
            (0.42, palette.ring_mid, material.opacity(0.55, 0.66)),
            (1.0, palette.ring_bottom, material.opacity(0.63, 0.74)),
        ] {
            glass.add_color_stop_rgba(t, rgb.0, rgb.1, rgb.2, alpha);
        }
        let sheen = face_n.dot(half_vec).max(0.0).powf(4.0) * pose.depth;
        cr.save()?;
        cr.set_fill_rule(FillRule::EvenOdd);
        trace_face(cr);
        cr.set_source(&glass)?;
        if sheen > 0.005 {
            cr.fill_preserve()?;
            // Soft 3D specular light reflection sweeping across the glass face.
            let sheen_grad = LinearGradient::new(0.0, gy_top, 0.0, gy_bot);
            let highlight = lerp_rgb(palette.rim_top, accent, 0.35);
            let peak = if view.high_contrast { 0.14 } else { 0.22 } * sheen;
            let band = (0.5 - 0.22 * face_n.1).clamp(0.18, 0.82);
            sheen_grad.add_color_stop_rgba(0.0, highlight.0, highlight.1, highlight.2, 0.0);
            sheen_grad.add_color_stop_rgba(
                (band - 0.06).max(0.01),
                highlight.0,
                highlight.1,
                highlight.2,
                peak,
            );
            sheen_grad.add_color_stop_rgba(
                (band + 0.06).min(0.99),
                highlight.0,
                highlight.1,
                highlight.2,
                peak * 0.85,
            );
            sheen_grad.add_color_stop_rgba(1.0, highlight.0, highlight.1, highlight.2, 0.0);
            cr.set_source(&sheen_grad)?;
            cr.fill()?;
        } else {
            cr.fill()?;
        }

        // Keep fading or emerging sectors aligned with the ring during brief entry/settle.
        let face_facing = (cam.2.abs() / PERSPECTIVE_D).clamp(0.0, 1.0);
        if face_facing > 0.35 && view.motion.sectors().next().is_some() {
            let o = project_face(0.0, 0.0);
            let px = project_face(100.0 * radius_scale, 0.0);
            let py = project_face(0.0, 100.0 * radius_scale);
            let ex = ((px.0 - o.0) / 100.0, (px.1 - o.1) / 100.0);
            let ey = ((py.0 - o.0) / 100.0, (py.1 - o.1) / 100.0);
            let det = ex.0 * ey.1 - ex.1 * ey.0;
            if det.abs() > 0.15 {
                let sector_alpha = ((face_facing - 0.35) / 0.65).clamp(0.0, 1.0);
                cr.save()?;
                trace_face(cr);
                cr.clip();
                cr.set_fill_rule(FillRule::Winding);
                cr.transform(Matrix::new(ex.0, ex.1, ey.0, ey.1, o.0, o.1));
                draw_sectors(cr, view, palette, sector_alpha)?;
                cr.restore()?;
            }
        }
        cr.restore()?;

        // Near face rims (matching 2D rim stroke and blending cleanly as depth -> 0).
        let rim = LinearGradient::new(0.0, gy_top, 0.0, gy_bot);
        rim.add_color_stop_rgba(
            0.0,
            palette.rim_top.0,
            palette.rim_top.1,
            palette.rim_top.2,
            if view.high_contrast { 1.0 } else { 0.70 },
        );
        rim.add_color_stop_rgba(
            1.0,
            palette.rim_bottom.0,
            palette.rim_bottom.1,
            palette.rim_bottom.2,
            if view.high_contrast { 1.0 } else { 0.27 },
        );
        cr.set_source(&rim)?;
        cr.set_line_width(if view.high_contrast { 1.8 } else { 1.0 });
        trace_face(cr);
        cr.stroke()?;

        if pose.depth > 0.01 {
            cr.new_path();
            trace_polygon(cr, &face_outer);
            cr.set_line_width(if view.high_contrast { 2.2 } else { 1.4 });
            color(cr, accent, (0.35 + 0.40 * sheen) * pose.depth);
            cr.stroke()?;
        }
    }

    if material == Material::Frosted {
        Ok(blur_region_3d(geom, scale, &outer_hull, &inner_front, &inner_back))
    } else {
        Ok(Region::create())
    }
}

fn ring_blur_region_cached(
    cx: f64,
    cy: f64,
    inner: f64,
    outer: f64,
    scale: f64,
) -> Result<Region, cairo::Error> {
    let key = [
        cx.to_bits(),
        cy.to_bits(),
        inner.to_bits(),
        outer.to_bits(),
        scale.to_bits(),
    ];
    thread_local! {
        static CACHE: RefCell<Option<([u64; 5], Region)>> = const { RefCell::new(None) };
    }
    CACHE.with(|cache| {
        let mut slot = cache.borrow_mut();
        if let Some((cached_key, region)) = slot.as_ref()
            && *cached_key == key
        {
            return Ok(region.copy());
        }
        let disk = |r| capsule_region(cx, cy, 2.0 * r * scale, 2.0 * r * scale);
        let ring = disk(outer);
        ring.subtract(&disk(inner))?;
        let out = ring.copy();
        *slot = Some((key, ring));
        Ok(out)
    })
}

fn draw_inner(cr: &Context, geom: &Geometry, view: &View<'_>) -> Result<Region, cairo::Error> {
    let scale = geom.scale * view.motion.scale();
    if scale <= 0.0 {
        return Ok(Region::create());
    }
    let palette = view.palette;
    cr.save()?;
    cr.translate(geom.center_x, geom.center_y);
    cr.scale(scale, scale);
    let material = Material::for_view(view);
    let blur = if let Some(pose) = view.motion.loading_pose() {
        draw_loading_ring_3d(cr, geom, scale, view, material, pose)?
    } else if let Some((inner, outer)) = ring_radii(view.motion) {
        ring_material_at(cr, inner, outer, material, palette)?;
        let blur = if material == Material::Frosted {
            ring_blur_region_cached(geom.center_x, geom.center_y, inner, outer, scale)?
        } else {
            Region::create()
        };
        if view.motion.sectors().next().is_some() {
            cr.save()?;
            sector_at(cr, 0.0, TAU, inner, outer);
            cr.clip();
            let s = outer / OUTER;
            cr.scale(s, s);
            draw_sectors(cr, view, palette, 1.0)?;
            cr.restore()?;
        }
        draw_ring_rim_at(cr, inner, outer, view.high_contrast, palette)?;
        blur
    } else {
        draw_ring_material_cached(cr, material, palette)?;
        let blur = if material == Material::Frosted {
            ring_blur_region_cached(geom.center_x, geom.center_y, INNER, OUTER, scale)?
        } else {
            Region::create()
        };
        draw_sectors(cr, view, palette, 1.0)?;
        draw_ring_rim_cached(cr, view.high_contrast, palette)?;
        blur
    };

    // The same Pango measurement, center and opening scale drive both the
    // painted capsule and the compositor mask. Shadows stay outside the mask.
    draw_captions(cr, geom.center_x, geom.center_y, scale, view, Some(&blur), 1.0)?;
    cr.restore()?;
    Ok(blur)
}

fn draw_captions(
    cr: &Context,
    cx: f64,
    cy: f64,
    scale: f64,
    view: &View<'_>,
    blur: Option<&Region>,
    alpha: f64,
) -> Result<(), cairo::Error> {
    if alpha <= 0.004 {
        return Ok(());
    }
    let material = Material::for_view(view);
    let palette = view.palette;
    let outer = caption_outer_radius(view.motion);
    let draw_capsule =
        |text: &str, y: f64, max_w: f64, suggestion: Option<&str>| -> Result<(), cairo::Error> {
            let (w, h) = capsule_cached(
                cr,
                text,
                y,
                max_w,
                suggestion,
                material,
                view.high_contrast,
                palette,
                alpha,
            )?;
            if material == Material::Frosted
                && let Some(blur) = blur
            {
                blur.union(&capsule_region(
                    cx,
                    cy + y * scale,
                    w * scale,
                    h * scale,
                ))?;
            }
            Ok(())
        };
    if !view.title.is_empty() {
        let title = match view.page {
            Some((current, total)) if total > 1 => {
                format!("{}   ·   {current} / {total}", view.title)
            }
            _ => view.title.to_owned(),
        };
        draw_capsule(&title, -outer - 22.0, 280.0, None)?;
    }
    if !view.query.is_empty() {
        draw_capsule(view.query, outer + 25.0, 332.0, Some(view.suggestion))?;
    }
    if !view.status.is_empty() {
        let y = outer + if view.query.is_empty() { 24.0 } else { 60.0 };
        draw_capsule(view.status, y, 344.0, None)?;
    }
    Ok(())
}

/// Draw and return this frame's compositor blur region in surface-local logical pixels.
pub fn draw(cr: &Context, geom: &Geometry, view: &View<'_>) -> Region {
    draw_inner(cr, geom, view).unwrap_or_else(|err| {
        eprintln!("ReCircles could not draw the ring: {err}");
        Region::create()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translucent_caption_tracks_ring_color_with_the_same_capsule_tint() {
        let render = |ring_color| {
            let palette = Palette {
                ring_top: ring_color,
                ring_mid: ring_color,
                ring_bottom: ring_color,
                ..Palette::default()
            };
            let mut image = cairo::ImageSurface::create(cairo::Format::ARgb32, 80, 40).unwrap();
            {
                let cr = Context::new(&image).unwrap();
                cr.translate(40.0, 20.0);
                capsule(&cr, "", 0.0, 80.0, None, Material::Translucent, false, &palette).unwrap();
            }
            let offset = 20 * image.stride() as usize + 40 * 4;
            let pixels = image.data().unwrap();
            u32::from_ne_bytes(pixels[offset..offset + 4].try_into().unwrap()).to_be_bytes()
        };
        let [warm_alpha, warm_red, _, warm_blue] = render((1.0, 0.0, 0.0));
        let [cool_alpha, cool_red, _, cool_blue] = render((0.0, 0.0, 1.0));
        assert!(warm_red > cool_red && cool_blue > warm_blue);
        assert!((1..255).contains(&warm_alpha));
        assert_eq!(warm_alpha, cool_alpha);
    }

    #[test]
    fn blur_mask_tracks_the_scaled_glass_and_excludes_the_clear_spaces() {
        let g = Geometry::new(381.0, 384.0, 380.0, 0.0);
        let mut motion = Motion::default();
        motion.open();
        for dt in [0.0, 0.045, 0.4] {
            motion.advance(dt);
            let scale = g.scale * motion.scale();
            let mut image = cairo::ImageSurface::create(cairo::Format::ARgb32, 381, 384).unwrap();
            let palette = Palette::default();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: 0,
                pressed: None,
                title: "Files",
                query: "~/",
                suggestion: "Projects/",
                status: "Type a folder path",
                page: None,
                high_contrast: false,
                solid: false,
                blur_available: true,
                loading_accent: None,
            };
            let region = draw_inner(&Context::new(&image).unwrap(), &g, &view).unwrap();
            let stride = image.stride() as usize;
            let pixels = image.data().unwrap();
            for (dx, dy, glass) in [
                (0.0, 0.0, false),
                (INNER - 3.0, 0.0, false),
                (INNER + 3.0, 0.0, true),
                (OUTER - 3.0, 0.0, true),
                (OUTER + 10.0, 0.0, false),
                (0.0, -OUTER - 22.0, true),
                (0.0, -OUTER - 5.0, false),
                (0.0, OUTER + 25.0, true),
                (0.0, OUTER + 44.0, false),
                (0.0, OUTER + 60.0, true),
                (110.0, OUTER + 60.0, false),
            ] {
                let x = (g.center_x + dx * scale) as i32;
                let y = (g.center_y + dy * scale) as i32;
                assert_eq!(
                    region.contains_point(x, y),
                    glass,
                    "{dx},{dy} at scale {scale}"
                );
                let offset = y as usize * stride + x as usize * 4;
                let argb = u32::from_ne_bytes(pixels[offset..offset + 4].try_into().unwrap());
                assert_eq!(argb >> 24 > 100, glass, "{dx},{dy} at scale {scale}");
                if dx == INNER + 3.0 {
                    assert!((130..=170).contains(&(argb >> 24)), "glass alpha {argb:#x}");
                }
            }
            drop(pixels);
            let cr = Context::new(&image).unwrap();
            assert!(
                draw_inner(
                    &cr,
                    &g,
                    &View {
                        high_contrast: true,
                        ..view
                    }
                )
                .unwrap()
                .is_empty()
            );
            assert!(
                draw_inner(
                    &cr,
                    &g,
                    &View {
                        blur_available: false,
                        ..view
                    }
                )
                .unwrap()
                .is_empty()
            );
        }
    }

    #[test]
    fn empty_modes_keep_the_title_above_the_ring() {
        let g = Geometry::new(500.0, 500.0, 250.0, 240.0);
        for title in ["Files", "Applications", "Alt-Tab"] {
            let mut image = cairo::ImageSurface::create(cairo::Format::ARgb32, 500, 500).unwrap();
            {
                let cr = Context::new(&image).unwrap();
                let palette = Palette::default();
                let view = View {
                    motion: &Motion::default(),
                    palette: &palette,
                    count: 0,
                    pressed: None,
                    title,
                    query: "",
                    suggestion: "",
                    status: "Loading…",
                    page: None,
                    high_contrast: false,
                    solid: true,
                    blur_available: false,
                    loading_accent: None,
                };
                draw_inner(&cr, &g, &view).unwrap();
            }
            let offset = 46 * image.stride() as usize + 250 * 4;
            let pixels = image.data().unwrap();
            let pixel = u32::from_ne_bytes(pixels[offset..offset + 4].try_into().unwrap());
            assert_eq!(pixel >> 24, 255, "{title}");
        }
    }

    #[test]
    fn active_result_preserves_the_transparent_hole_and_draws_its_app_icon() {
        let mut image = cairo::ImageSurface::create(cairo::Format::ARgb32, 500, 500).unwrap();
        let g = Geometry::new(500.0, 500.0, 250.0, 240.0);
        {
            let cr = Context::new(&image).unwrap();
            let icon = Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 32, 16).unwrap();
            icon.fill(0xe52d49ff);
            let mut motion = Motion::default();
            motion.update(
                vec![Item {
                    label: "Editor".into(),
                    icon: Icon::App,
                    pixbuf: Some(icon),
                    active: false,
                }],
                Some(0),
                false,
            );
            let palette = Palette::default();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: 1,
                pressed: Some(0),
                title: "Files",
                query: "~/",
                suggestion: "Documents/",
                status: "",
                page: None,
                high_contrast: true,
                solid: true,
                blur_available: false,
                loading_accent: None,
            };
            draw_inner(&cr, &g, &view).unwrap();
        }
        let stride = image.stride() as usize;
        let pixels = image.data().unwrap();
        let pixel = |x: usize, y: usize| {
            let offset = y * stride + x * 4;
            u32::from_ne_bytes(pixels[offset..offset + 4].try_into().unwrap())
        };
        assert_eq!(pixel(250, 240), 0);
        assert_eq!(pixel(320, 240), 0);
        assert_eq!(pixel(370, 240) >> 24, 255);
        assert_eq!(pixel(250, 108), 0xffe52d49);
        assert_ne!(pixel(250, 96), 0xffe52d49);
    }

    #[test]
    fn every_visible_sector_and_animated_center_agree() {
        for count in 1..=8 {
            let g = Geometry::new(800.0, 600.0, 450.0, 300.0);
            let mut layout = Motion::default();
            layout.update(
                (0..count)
                    .map(|i| Item {
                        label: i.to_string(),
                        icon: Icon::Folder,
                        pixbuf: None,
                        active: false,
                    })
                    .collect(),
                None,
                false,
            );
            for motion in [0.82, 0.93, 1.0] {
                assert_eq!(g.angle(450.0, 300.0, motion), None);
                for i in 0..count {
                    let a = -FRAC_PI_2 + TAU * i as f64 / count as f64;
                    let x = g.center_x + ITEM_RADIUS * a.cos() * g.scale * motion;
                    let y = g.center_y + ITEM_RADIUS * a.sin() * g.scale * motion;
                    assert_eq!(g.angle(x, y, motion).and_then(|a| layout.hit(a)), Some(i));
                }
            }
            assert_eq!(g.angle(450.0 + OUTER + 1.0, 300.0, 1.0), None);
        }
    }

    #[test]
    fn single_item_highlight_closes_the_bottom_seam() {
        let mut image = cairo::ImageSurface::create(cairo::Format::ARgb32, 500, 500).unwrap();
        let g = Geometry::new(500.0, 500.0, 250.0, 240.0);
        {
            let cr = Context::new(&image).unwrap();
            let mut motion = Motion::default();
            motion.update(
                vec![Item {
                    label: "Only".into(),
                    icon: Icon::App,
                    pixbuf: None,
                    active: false,
                }],
                Some(0),
                false,
            );
            let palette = Palette::default();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: 1,
                pressed: None,
                title: "Applications",
                query: "",
                suggestion: "",
                status: "",
                page: None,
                high_contrast: false,
                solid: true,
                blur_available: false,
                loading_accent: None,
            };
            draw_inner(&cr, &g, &view).unwrap();
        }
        let stride = image.stride() as usize;
        let pixels = image.data().unwrap();
        let red = |x: usize, y: usize| {
            let offset = y * stride + x * 4;
            (u32::from_ne_bytes(pixels[offset..offset + 4].try_into().unwrap()) >> 16) & 0xff
        };
        let r = (OUTER - 2.6).round() as usize;
        assert!(red(250, 240 + r) > 220, "bottom highlight missing");
        assert!(red(250, 240 - r) > 220, "top highlight missing");
    }

    #[test]
    fn renders_wifi_and_bluetooth_glyphs_and_active_badges() {
        let icons = [
            Icon::Wifi,
            Icon::Bluetooth,
            Icon::WifiNetwork,
            Icon::WifiConnected,
            Icon::WifiLocked,
            Icon::WifiOff,
            Icon::BluetoothDevice,
            Icon::BluetoothConnected,
            Icon::BluetoothOff,
            Icon::Scan,
            Icon::Settings,
            Icon::ToggleOn,
            Icon::ToggleOff,
            Icon::Add,
            Icon::Disconnect,
            Icon::Forget,
            Icon::Palette,
            Icon::Keyboard,
            Icon::Appearance,
            Icon::MoveUp,
            Icon::MoveDown,
            Icon::Reset,
            Icon::ColorSwatch(0xff, 0x9e, 0x47),
        ];
        let g = Geometry::new(500.0, 500.0, 250.0, 240.0);
        let palette = Palette::default();
        for chunk in icons.chunks(8) {
            let items: Vec<_> = chunk
                .iter()
                .enumerate()
                .map(|(i, &icon)| Item {
                    label: format!("Item {i}"),
                    icon,
                    pixbuf: None,
                    active: i % 2 == 0,
                })
                .collect();
            let mut motion = Motion::default();
            motion.update(items, Some(0), false);
            let image = cairo::ImageSurface::create(cairo::Format::ARgb32, 500, 500).unwrap();
            let cr = Context::new(&image).unwrap();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: chunk.len(),
                pressed: None,
                title: "Wi-Fi",
                query: "",
                suggestion: "",
                status: "Connected",
                page: None,
                high_contrast: false,
                solid: true,
                blur_available: false,
                loading_accent: None,
            };
            draw_inner(&cr, &g, &view).unwrap();
        }
    }

    #[test]
    fn overlays_fit_at_corners_and_on_small_outputs() {
        for (w, h) in [(290.0, 240.0), (1920.0, 1080.0)] {
            for (x, y) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
                let g = Geometry::new(w, h, x, y);
                assert!(g.center_x - SIDE * g.scale >= -1e-6);
                assert!(g.center_y - TOP * g.scale >= -1e-6);
                assert!(g.center_x + SIDE * g.scale <= w + 1e-6);
                assert!(g.center_y + BOTTOM * g.scale <= h + 1e-6);
                assert_eq!(g.angle(g.center_x, g.center_y, 1.0), None);
            }
        }
    }

    #[test]
    fn loading_ring_renders_3d_rotation_and_tracks_blur_mask() {
        let g = Geometry::new(500.0, 500.0, 250.0, 240.0);
        let palette = Palette::default();
        let mut motion = Motion::default();
        motion.set_loading(true, true);

        let mut saw_narrower_edge = false;
        let mut saw_clear_hole = false;
        for step in 0..90 {
            motion.advance(1.0 / 30.0);
            let mut image = cairo::ImageSurface::create(cairo::Format::ARgb32, 500, 500).unwrap();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: 0,
                pressed: None,
                title: "Bluetooth",
                query: "",
                suggestion: "",
                status: "",
                page: None,
                high_contrast: false,
                solid: false,
                blur_available: true,
                loading_accent: Some(palette.accent_bluetooth),
            };
            let region = draw_inner(&Context::new(&image).unwrap(), &g, &view).unwrap();
            assert!(!region.is_empty(), "step {step} should produce blur region");

            // Check the top title capsule is drawn and the bottom status area stays empty.
            let stride = image.stride() as usize;
            let pixels = image.data().unwrap();
            let alpha_at = |x: usize, y: usize| {
                let offset = y * stride + x * 4;
                u32::from_ne_bytes(pixels[offset..offset + 4].try_into().unwrap()) >> 24
            };
            assert!(alpha_at(250, 46) > 100, "title capsule missing at step {step}");
            assert_eq!(alpha_at(250, 436), 0, "status capsule should be absent at step {step}");

            if !region.contains_point(250, 240) && alpha_at(250, 240) == 0 {
                saw_clear_hole = true;
            }
            // When tumbling through edge-on in any direction, a cardinal flank clears out.
            if [(250 + 130, 240), (250 - 130, 240), (250, 240 + 130), (250, 240 - 130)]
                .into_iter()
                .any(|(x, y)| !region.contains_point(x, y))
            {
                saw_narrower_edge = true;
            }
            drop(pixels);

            let cr = Context::new(&image).unwrap();
            assert!(
                draw_inner(
                    &cr,
                    &g,
                    &View {
                        solid: true,
                        ..view
                    }
                )
                .unwrap()
                .is_empty()
            );
        }
        assert!(saw_clear_hole, "3D ring should show its transparent center hole");
        assert!(saw_narrower_edge, "3D ring should foreshorten horizontally while spinning");
    }

    #[test]
    fn main_menu_titles_fit_inside_their_sectors_without_clipping() {
        let image = cairo::ImageSurface::create(cairo::Format::ARgb32, 500, 500).unwrap();
        let cr = Context::new(&image).unwrap();
        let sets: &[&[&str]] = &[
            &["Files", "Applications", "Alt-Tab"],
            &["Files", "Applications", "Alt-Tab", "Wi-Fi", "Bluetooth"],
            &[
                "Files",
                "Applications",
                "Alt-Tab",
                "Wi-Fi",
                "Bluetooth",
                "Screenshots",
                "Power Menu",
                "Audio Mixer",
            ],
        ];
        for labels in sets {
            let count = labels.len();
            let seam = -FRAC_PI_2 - PI / count as f64;
            let span = TAU / count as f64;
            let style = item_style(true, count);
            for (i, &label) in labels.iter().enumerate() {
                let start = seam + i as f64 * span;
                let end = start + span;
                let angle = (start + end) * 0.5;
                let origin_x = style.horizontal_radius * angle.cos();
                let origin_y = ITEM_RADIUS * angle.sin();
                for zoom in [1.0, MAX_HOVER_ZOOM] {
                    let (layout, label_x, width) = fit_item_label(
                        &cr,
                        label,
                        (start, end),
                        (origin_x, origin_y),
                        style,
                        zoom,
                    );
                    let (text_w, text_h) = layout.pixel_size();
                    assert!(
                        f64::from(text_w) <= width + 0.5,
                        "count={count} item {label:?} ({text_w}px) exceeded width {width:.1}px"
                    );
                    let half_w = f64::from(text_w) * 0.5 * zoom;
                    let cx = origin_x + label_x * zoom;
                    let cy = origin_y + style.label_y * zoom;
                    let half_h = f64::from(text_h) * 0.5 * zoom;
                    for (px, py) in [
                        (cx - half_w, cy - half_h),
                        (cx + half_w, cy - half_h),
                        (cx - half_w, cy + half_h),
                        (cx + half_w, cy + half_h),
                    ] {
                        let r = px.hypot(py);
                        assert!(
                            (INNER..=OUTER).contains(&r),
                            "count={count} item {label:?} corner ({px:.1},{py:.1}) r={r:.1} outside [{INNER},{OUTER}]"
                        );
                        let da = (py.atan2(px) - start).rem_euclid(TAU);
                        assert!(
                            da <= span,
                            "count={count} item {label:?} corner ({px:.1},{py:.1}) outside sector"
                        );
                    }
                }
            }
        }
    }

    fn painted_bbox(surface: &mut ImageSurface) -> Option<(i32, i32, i32, i32)> {
        let w = surface.width();
        let h = surface.height();
        let stride = surface.stride() as usize;
        let (sx, sy) = surface.device_scale();
        let data = surface.data().ok()?;
        let (mut min_x, mut min_y) = (i32::MAX, i32::MAX);
        let (mut max_x, mut max_y) = (i32::MIN, i32::MIN);
        for py in 0..h {
            let row = py as usize * stride;
            for px in 0..w {
                if data[row + px as usize * 4 + 3] != 0 {
                    let lx = (f64::from(px) / sx).floor() as i32;
                    let ly = (f64::from(py) / sy).floor() as i32;
                    let rx = (f64::from(px + 1) / sx).ceil() as i32;
                    let ry = (f64::from(py + 1) / sy).ceil() as i32;
                    min_x = min_x.min(lx);
                    min_y = min_y.min(ly);
                    max_x = max_x.max(rx);
                    max_y = max_y.max(ry);
                }
            }
        }
        (min_x < max_x && min_y < max_y).then_some((min_x, min_y, max_x, max_y))
    }

    #[test]
    fn damage_rect_contains_every_painted_pixel_in_2d_and_3d_modes() {
        let palette = Palette::default();
        let items: Vec<Item> = (0..8)
            .map(|i| Item {
                label: format!("Application {i}"),
                icon: Icon::Applications,
                pixbuf: None,
                active: i == 0,
            })
            .collect();

        // Zero scale produces an empty damage rect.
        let zero_g = Geometry::new(0.0, 0.0, 0.0, 0.0);
        let empty = zero_g.damage_rect(&Motion::default());
        assert_eq!((empty.width(), empty.height()), (0, 0));

        // 2D ring with all captions, shadow, active dot, and both 1x and 2x device scales.
        for (dev_scale, lw, lh, anchor) in [
            (1.0, 900.0, 900.0, (450.0, 450.0)),
            (2.0, 900.0, 900.0, (450.0, 450.0)),
            (1.0, 420.0, 500.0, (20.0, 20.0)),
        ] {
            let mut motion = Motion::default();
            motion.update(items.clone(), Some(0), false);
            motion.open();
            motion.advance(0.03);
            let geom = Geometry::new(lw, lh, anchor.0, anchor.1);
            let damage = geom.damage_rect(&motion);
            let mut surface = ImageSurface::create(
                Format::ARgb32,
                (lw * dev_scale) as i32,
                (lh * dev_scale) as i32,
            )
            .unwrap();
            surface.set_device_scale(dev_scale, dev_scale);
            let cr = Context::new(&surface).unwrap();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: items.len(),
                pressed: Some(0),
                title: "Applications",
                query: "firefox",
                suggestion: " browser",
                status: "8 matching applications",
                page: Some((1, 3)),
                high_contrast: true,
                solid: false,
                blur_available: true,
                loading_accent: None,
            };
            draw(&cr, &geom, &view);
            drop(cr);
            let (min_x, min_y, max_x, max_y) = painted_bbox(&mut surface).unwrap();
            assert!(
                min_x >= damage.x()
                    && min_y >= damage.y()
                    && max_x <= damage.x() + damage.width()
                    && max_y <= damage.y() + damage.height(),
                "2D painted bbox ({min_x},{min_y})..({max_x},{max_y}) escaped damage ({},{})..({},{})",
                damage.x(),
                damage.y(),
                damage.x() + damage.width(),
                damage.y() + damage.height()
            );
        }

        // 3D loading ring across multiple tumble orientations.
        let mut lmotion = Motion::default();
        lmotion.set_loading(true, true);
        let geom = Geometry::new(900.0, 900.0, 450.0, 450.0);
        for step in [0.05, 0.25, 0.55, 1.10, 2.00] {
            lmotion.advance(step);
            let damage = geom.damage_rect(&lmotion);
            let mut surface = ImageSurface::create(Format::ARgb32, 900, 900).unwrap();
            let cr = Context::new(&surface).unwrap();
            let view = View {
                motion: &lmotion,
                palette: &palette,
                count: 0,
                pressed: None,
                title: "Scanning Wi-Fi",
                query: "net",
                suggestion: "work",
                status: "Searching nearby networks",
                page: None,
                high_contrast: false,
                solid: false,
                blur_available: true,
                loading_accent: Some(palette.accent_wifi),
            };
            draw(&cr, &geom, &view);
            drop(cr);
            let (min_x, min_y, max_x, max_y) = painted_bbox(&mut surface).unwrap();
            assert!(
                min_x >= damage.x()
                    && min_y >= damage.y()
                    && max_x <= damage.x() + damage.width()
                    && max_y <= damage.y() + damage.height(),
                "3D step={step} painted bbox ({min_x},{min_y})..({max_x},{max_y}) escaped damage ({},{})..({},{})",
                damage.x(),
                damage.y(),
                damage.x() + damage.width(),
                damage.y() + damage.height()
            );
        }
    }

    #[test]
    fn convex_scanner_matches_brute_force_edge_intersections() {
        fn brute_span(poly: &[(f64, f64)], y: f64) -> Option<(f64, f64)> {
            let n = poly.len();
            if n < 3 {
                return None;
            }
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for i in 0..n {
                let (x0, y0) = poly[i];
                let (x1, y1) = poly[(i + 1) % n];
                let (ya, yb, xa, xb) = if y0 <= y1 {
                    (y0, y1, x0, x1)
                } else {
                    (y1, y0, x1, x0)
                };
                if ya <= y && y < yb {
                    let x = xa + ((y - ya) / (yb - ya)) * (xb - xa);
                    lo = lo.min(x);
                    hi = hi.max(x);
                }
            }
            (lo < hi).then_some((lo, hi))
        }

        // Degenerate polygons return None cleanly.
        assert!(ConvexScanner::new(&[]).is_none());
        assert!(ConvexScanner::new(&[(0.0, 0.0), (10.0, 10.0)]).is_none());
        assert!(ConvexScanner::new(&[(0.0, 5.0), (10.0, 5.0), (20.0, 5.0)]).is_none());

        let regular_64: Vec<_> = (0..64)
            .map(|i| {
                let a = TAU * i as f64 / 64.0;
                (100.0 + 80.0 * a.cos(), 100.0 + 45.0 * a.sin())
            })
            .collect();
        let flat_trapezoid = vec![(20.0, 10.0), (80.0, 10.0), (100.0, 60.0), (0.0, 60.0)];
        let diamond = vec![(50.0, 5.0), (95.0, 50.0), (50.0, 95.0), (5.0, 50.0)];
        let mut lmotion = Motion::default();
        lmotion.set_loading(true, true);
        for _ in 0..30 {
            lmotion.advance(1.0 / 60.0);
        }
        let tf = Transform3D::from_pose(lmotion.loading_pose().unwrap());
        let projected_ellipse: Vec<_> = (0..RING_SEGMENTS)
            .map(|i| {
                let a = TAU * i as f64 / RING_SEGMENTS as f64;
                let (x, y, _) = tf.project(Vec3(INNER * a.cos(), INNER * a.sin(), 10.0));
                (200.0 + x, 200.0 + y)
            })
            .collect();

        for poly in [regular_64, flat_trapezoid, diamond, projected_ellipse] {
            let mut scanner = ConvexScanner::new(&poly).unwrap();
            let y0 = (scanner.min_y.floor() as i32) - 2;
            let y1 = (scanner.max_y.ceil() as i32) + 2;
            for iy in y0..=y1 {
                let y = f64::from(iy) + 0.5;
                let got = scanner.span(y);
                let expected = brute_span(&poly, y);
                match (got, expected) {
                    (Some((a0, a1)), Some((b0, b1))) => {
                        assert!((a0 - b0).abs() < 1e-9 && (a1 - b1).abs() < 1e-9);
                    }
                    (None, None) => {}
                    _ => panic!("scanline y={y}: scanner={got:?} != brute={expected:?}"),
                }
            }
        }
    }

    #[test]
    fn cached_ring_surfaces_invalidate_on_style_and_scale_changes() {
        let render_pixel = |material: Material, high_contrast: bool, palette: &Palette, scale: f64| {
            let dim = (400.0 * scale) as i32;
            let mut surface = ImageSurface::create(Format::ARgb32, dim, dim).unwrap();
            surface.set_device_scale(scale, scale);
            {
                let cr = Context::new(&surface).unwrap();
                cr.translate(200.0, 200.0);
                draw_ring_material_cached(&cr, material, palette).unwrap();
                draw_ring_rim_cached(&cr, high_contrast, palette).unwrap();
            }
            let stride = surface.stride() as usize;
            let data = surface.data().unwrap();
            let px = |lx: f64, ly: f64| {
                let x = (lx * scale) as usize;
                let y = (ly * scale) as usize;
                let at = y * stride + x * 4;
                [data[at], data[at + 1], data[at + 2], data[at + 3]]
            };
            (px(200.0, 200.0 - 120.0), px(200.0, 200.0 - OUTER))
        };

        let base = Palette::default();
        let (body_frosted, rim_normal) = render_pixel(Material::Frosted, false, &base, 1.0);
        let (body_solid, rim_hc) = render_pixel(Material::Solid, true, &base, 1.0);
        assert!(body_solid[3] > body_frosted[3], "Solid material should have higher alpha");
        assert!(rim_hc[3] >= rim_normal[3] && rim_hc != rim_normal);

        let mut green = base;
        green.ring_top = (0.05, 0.90, 0.15);
        green.ring_mid = (0.05, 0.85, 0.15);
        green.ring_bottom = (0.05, 0.80, 0.15);
        let (body_green, _) = render_pixel(Material::Solid, false, &green, 1.0);
        assert!(body_green[1] > body_solid[1] + 50, "Palette change must invalidate cache");

        // 2x HiDPI renders validly without panicking or stale 1x artifacts.
        let (body_2x, _) = render_pixel(Material::Solid, false, &green, 2.0);
        assert!(body_2x[1] > body_solid[1] + 50);
    }

    #[test]
    fn empty_ring_pulse_shrinks_radius_modulates_width_and_updates_blur_mask() {
        let g = Geometry::new(500.0, 500.0, 250.0, 240.0);
        let palette = Palette::default();
        let mut motion = Motion::default();
        motion.set_empty(true, false, false);
        assert_eq!(ring_radii(&motion), Some((EMPTY_INNER, EMPTY_OUTER)));

        motion.set_empty(true, true, true);

        let mut min_width = f64::INFINITY;
        let mut max_width = 0.0_f64;
        for step in 0..120 {
            motion.advance(1.0 / 60.0);
            let (inner, outer) = ring_radii(&motion).expect("empty ring radii");
            if step >= 30 {
                assert!(outer < OUTER - 50.0, "empty ring should be smaller than normal OUTER");
                let width = outer - inner;
                min_width = min_width.min(width);
                max_width = max_width.max(width);
            }
            let image = ImageSurface::create(Format::ARgb32, 500, 500).unwrap();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: 0,
                pressed: None,
                title: "Clipboard",
                query: "",
                suggestion: "",
                status: "",
                page: None,
                high_contrast: false,
                solid: false,
                blur_available: true,
                loading_accent: None,
            };
            let region = draw_inner(&Context::new(&image).unwrap(), &g, &view).unwrap();
            let mid_r = (inner + outer) * 0.5;
            assert!(
                region.contains_point((g.center_x + mid_r) as i32, g.center_y as i32),
                "blur mask should cover pulsing ring body at step {step}"
            );
            assert!(
                !region.contains_point(g.center_x as i32, g.center_y as i32),
                "center hole should stay clear at step {step}"
            );
            if outer < OUTER - 20.0 {
                assert!(
                    !region.contains_point((g.center_x + OUTER - 4.0) as i32, g.center_y as i32),
                    "shrunken ring should not blur the full normal outer radius at step {step}"
                );
            }
        }
        assert!(
            max_width - min_width > 25.0,
            "ring width should pulse significantly (min={min_width:.1}, max={max_width:.1})"
        );
    }

    #[test]
    fn icon_plate_and_capsule_caches_invalidate_across_pixbuf_palette_and_scale() {
        let base = Palette::default();
        let make_pb = |rgba: u32| {
            let pb = Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 32, 32).unwrap();
            pb.fill(rgba);
            pb
        };
        let render_center_rgba = |item: &Item, plate: f64, icon_size: f64, palette: &Palette, scale: f64| {
            let dim = (120.0 * scale).ceil() as i32;
            let mut surf = ImageSurface::create(Format::ARgb32, dim, dim).unwrap();
            surf.set_device_scale(scale, scale);
            {
                let cr = Context::new(&surf).unwrap();
                cr.translate(60.0, 60.0);
                draw_icon_plate(&cr, item, 0.0, plate, icon_size, palette, 1.0).unwrap();
            }
            let stride = surf.stride() as usize;
            let data = surf.data().unwrap();
            let px = (60.0 * scale) as usize;
            let py = (60.0 * scale) as usize;
            let at = py * stride + px * 4;
            // ARGB32 on little-endian: [B, G, R, A] -> return [R, G, B, A]
            [data[at + 2], data[at + 1], data[at], data[at + 3]]
        };

        let red_item = Item {
            label: "App".into(),
            icon: Icon::App,
            pixbuf: Some(make_pb(0xff2020ff)),
            active: false,
        };
        let green_item = Item {
            label: "App".into(),
            icon: Icon::App,
            pixbuf: Some(make_pb(0x20ff20ff)),
            active: true,
        };
        for scale in [1.0, 1.5, 2.0] {
            // Canonical 48px and morph-interpolated 40px must both render the right pixbuf.
            for (plate, icon_size) in [(48.0, 26.0), (40.0, 23.0)] {
                let red = render_center_rgba(&red_item, plate, icon_size, &base, scale);
                let green = render_center_rgba(&green_item, plate, icon_size, &base, scale);
                assert!(
                    red[0] > 180 && red[1] < 80,
                    "scale={scale} plate={plate} red pixbuf mismatch: {red:?}"
                );
                assert!(
                    green[1] > 180 && green[0] < 80,
                    "scale={scale} plate={plate} green pixbuf mismatch: {green:?}"
                );
            }
        }

        // Verify capsule_cached invalidates on palette ring tint change.
        let sample_capsule = |palette: &Palette| {
            let mut surf = ImageSurface::create(Format::ARgb32, 300, 120).unwrap();
            let w = {
                let cr = Context::new(&surf).unwrap();
                cr.translate(150.0, 0.0);
                capsule_cached(
                    &cr,
                    "Applications",
                    60.0,
                    240.0,
                    None,
                    Material::Frosted,
                    false,
                    palette,
                    1.0,
                )
                .unwrap()
                .0
            };
            let stride = surf.stride() as usize;
            let data = surf.data().unwrap();
            let px = (150.0 - w * 0.5 + 6.0).round() as usize;
            let at = 60 * stride + px * 4;
            [data[at + 2], data[at + 1], data[at], data[at + 3]]
        };
        let mut blue_pal = base;
        blue_pal.ring_top = (0.05, 0.15, 0.95);
        blue_pal.ring_mid = (0.05, 0.15, 0.90);
        blue_pal.ring_bottom = (0.05, 0.15, 0.85);
        let c_base = sample_capsule(&base);
        let c_blue = sample_capsule(&blue_pal);
        assert!(
            c_blue[2] > c_base[2] + 40,
            "capsule_cached must invalidate on palette change: base={c_base:?} blue={c_blue:?}"
        );
    }

    #[test]
    fn ring_2d_and_3d_wall_scratch_stay_inside_damage_rect_across_fractional_scales() {
        let palette = Palette::default();
        let geom = Geometry::new(700.0, 700.0, 350.0, 350.0);
        let sample_items: Vec<Item> = [
            ("Files", Icon::Files),
            ("Applications", Icon::Applications),
            ("Alt-Tab", Icon::Windows),
            ("Wi-Fi", Icon::Wifi),
            ("Bluetooth", Icon::Bluetooth),
        ]
        .into_iter()
        .map(|(label, icon)| Item {
            label: label.into(),
            icon,
            pixbuf: None,
            active: false,
        })
        .collect();

        for scale in [1.0_f64, 1.25, 1.5, 2.0] {
            let pw = (700.0 * scale).ceil() as i32;
            let ph = (700.0 * scale).ceil() as i32;

            // 1. 2D opening ring with captions & hover zoom
            let mut m2d = Motion::default();
            m2d.update(sample_items.clone(), Some(1), false);
            m2d.open();
            m2d.advance(0.05);
            let dmg2d = geom.damage_rect(&m2d);
            let mut surf2d = ImageSurface::create(Format::ARgb32, pw, ph).unwrap();
            surf2d.set_device_scale(scale, scale);
            {
                let cr = Context::new(&surf2d).unwrap();
                let view = View {
                    motion: &m2d,
                    palette: &palette,
                    count: sample_items.len(),
                    pressed: None,
                    title: "ReCircles",
                    query: "app",
                    suggestion: "lications",
                    status: "5 items",
                    page: Some((1, 2)),
                    high_contrast: false,
                    solid: false,
                    blur_available: true,
                    loading_accent: None,
                };
                let _ = draw(&cr, &geom, &view);
            }
            let stride2d = surf2d.stride() as usize;
            let data2d = surf2d.data().unwrap();
            let mut painted2d = 0usize;
            for dy in 0..ph {
                let row = dy as usize * stride2d;
                for dx in 0..pw {
                    if data2d[row + dx as usize * 4 + 3] != 0 {
                        painted2d += 1;
                        let lx = (f64::from(dx) / scale).floor() as i32;
                        let ly = (f64::from(dy) / scale).floor() as i32;
                        assert!(
                            lx >= dmg2d.x()
                                && ly >= dmg2d.y()
                                && lx < dmg2d.x() + dmg2d.width()
                                && ly < dmg2d.y() + dmg2d.height(),
                            "2D ring scale={scale} pixel ({dx},{dy}) logical ({lx},{ly}) escaped {dmg2d:?}"
                        );
                    }
                }
            }
            assert!(painted2d > 500);

            // 2. 3D loading spin exercising translucent WALL_SCRATCH
            let mut m3d = Motion::default();
            m3d.set_loading(true, true);
            for _ in 0..35 {
                m3d.advance(1.0 / 60.0);
            }
            let dmg3d = geom.damage_rect(&m3d);
            let mut surf3d = ImageSurface::create(Format::ARgb32, pw, ph).unwrap();
            surf3d.set_device_scale(scale, scale);
            {
                let cr = Context::new(&surf3d).unwrap();
                let view = View {
                    motion: &m3d,
                    palette: &palette,
                    count: 0,
                    pressed: None,
                    title: "Bluetooth",
                    query: "",
                    suggestion: "",
                    status: "Scanning...",
                    page: None,
                    high_contrast: false,
                    solid: false,
                    blur_available: true,
                    loading_accent: Some(palette.accent_bluetooth),
                };
                let region = draw(&cr, &geom, &view);
                assert!(!region.is_empty());
            }
            let stride3d = surf3d.stride() as usize;
            let data3d = surf3d.data().unwrap();
            let mut painted3d = 0usize;
            for dy in 0..ph {
                let row = dy as usize * stride3d;
                for dx in 0..pw {
                    if data3d[row + dx as usize * 4 + 3] != 0 {
                        painted3d += 1;
                        let lx = (f64::from(dx) / scale).floor() as i32;
                        let ly = (f64::from(dy) / scale).floor() as i32;
                        assert!(
                            lx >= dmg3d.x()
                                && ly >= dmg3d.y()
                                && lx < dmg3d.x() + dmg3d.width()
                                && ly < dmg3d.y() + dmg3d.height(),
                            "3D ring scale={scale} pixel ({dx},{dy}) logical ({lx},{ly}) escaped {dmg3d:?}"
                        );
                    }
                }
            }
            assert!(painted3d > 500, "3D ring at scale={scale} should render visible pixels");
            // Center of 3D ring hole remains transparent when viewing near face.
            let cx = (geom.center_x * scale) as usize;
            let cy = (geom.center_y * scale) as usize;
            assert_eq!(
                data3d[cy * stride3d + cx * 4 + 3],
                0,
                "3D ring center hole should stay transparent at scale={scale}"
            );
        }
    }
}
