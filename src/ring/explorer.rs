//! One critically damped spring moves the glass outline, its cutout, and each
//! item from the ring's coordinates into the grid. Input uses the same live
//! presentation geometry.
use super::*;
use crate::menu::{GRID_COLUMNS, GRID_ROWS, LIST_ROWS, PAGE_SIZE};
use crate::motion::Sector;

const CELL_W: f64 = 142.0;
const CELL_H: f64 = 110.0;
const LEFT: f64 = 14.0;
const RIGHT_PAD: f64 = 20.0;
const WIDTH: f64 = LEFT + GRID_COLUMNS as f64 * CELL_W + RIGHT_PAD;
const GRID_H: f64 = CELL_H * GRID_ROWS as f64;
const HEADER_PLAIN: f64 = 48.0;
const HEADER_QUERY: f64 = 80.0;
const FOOTER: f64 = 34.0;
const BOX_RADIUS: f64 = 20.0;

const LIST_W: f64 = 368.0;
const LIST_ROW_H: f64 = 42.0;
const LIST_FOOTER: f64 = 10.0;

const INPUT_W: f64 = 388.0;
const INPUT_H: f64 = 160.0;
pub const COLOR_INPUT_H: f64 = 362.0;
const INPUT_HEADER: f64 = 46.0;
const INPUT_FOOTER: f64 = 54.0;

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Copy)]
pub struct Bounds {
    cx: f64,
    cy: f64,
    scale: f64,
    width: f64,
    height: f64,
    radius: f64,
    hole: f64,
    header: f64,
    footer: f64,
    list_rows: f64,
    list_rows_settled: bool,
    t: f64,
    box_t: f64,
    list_t: f64,
    input_t: f64,
}

impl Bounds {
    pub fn new(size: (f64, f64), anchor: (f64, f64), has_query: bool, motion: &Motion) -> Self {
        Self::new_with_input_height(size, anchor, has_query, motion, INPUT_H)
    }

    pub fn new_with_input_height(
        size: (f64, f64),
        anchor: (f64, f64),
        has_query: bool,
        motion: &Motion,
        input_h: f64,
    ) -> Self {
        let grid_header = if has_query {
            HEADER_QUERY
        } else {
            HEADER_PLAIN
        };
        let grid_h = grid_header + GRID_H + FOOTER;
        let list_rows = motion.list_rows();
        let list_h = grid_header + list_rows * LIST_ROW_H + LIST_FOOTER;

        let w_box = motion.box_mix().clamp(0.0, 1.0);
        let w_list = motion.list_mix().clamp(0.0, 1.0);
        let w_input = motion.input_mix().clamp(0.0, 1.0);
        let w_sum = w_box + w_list + w_input;
        let t = w_sum.clamp(0.0, 1.0);

        let (target_w, target_h, header, footer) = if w_sum > 1e-6 {
            (
                (w_box * WIDTH + w_list * LIST_W + w_input * INPUT_W) / w_sum,
                (w_box * grid_h + w_list * list_h + w_input * input_h) / w_sum,
                (w_box * grid_header + w_list * grid_header + w_input * INPUT_HEADER) / w_sum,
                (w_box * FOOTER + w_list * LIST_FOOTER + w_input * INPUT_FOOTER) / w_sum,
            )
        } else {
            (WIDTH, grid_h, grid_header, FOOTER)
        };

        let fit = (size.0 / (target_w + 20.0))
            .min(size.1 / (target_h + 20.0))
            .min(1.0);
        let center = |anchor: f64, extent: f64, screen: f64| {
            let margin = (extent / 2.0 + 8.0) * fit;
            anchor.max(margin).min(screen - margin)
        };
        let ring = Geometry::new(size.0, size.1, anchor.0, anchor.1);
        let scale = lerp(ring.scale, fit, t) * motion.scale();
        // Interpolate screen-space extents as well as the center on small outputs.
        let extent = |from, to| {
            lerp(from * ring.scale, to * fit, t) / lerp(ring.scale, fit, t).max(f64::EPSILON)
        };
        let hole_t = (1.0 - (t / 0.52).min(1.0)).powi(2);
        let hole = INNER * hole_t;
        Self {
            cx: lerp(ring.center_x, center(anchor.0, target_w, size.0), t),
            cy: lerp(ring.center_y, center(anchor.1, target_h, size.1), t),
            width: extent(2.0 * OUTER, target_w),
            height: extent(2.0 * OUTER, target_h),
            radius: extent(OUTER, BOX_RADIUS),
            hole: if hole < 1.0 { 0.0 } else { hole },
            header,
            footer,
            list_rows,
            list_rows_settled: motion.list_rows_settled(),
            scale,
            t,
            box_t: w_box,
            list_t: w_list,
            input_t: w_input,
        }
    }

    fn local(&self, (x, y): (f64, f64)) -> (f64, f64) {
        ((x - self.cx) / self.scale, (y - self.cy) / self.scale)
    }

    fn inside(&self, x: f64, y: f64) -> bool {
        let dx = (x.abs() - (self.width * 0.5 - self.radius)).max(0.0);
        let dy = (y.abs() - (self.height * 0.5 - self.radius)).max(0.0);
        dx.hypot(dy) <= self.radius && x.hypot(y) >= self.hole
    }

    pub fn contains(&self, pos: (f64, f64)) -> bool {
        let (x, y) = self.local(pos);
        self.inside(x, y)
    }

    fn tile(&self, sector: &Sector<'_>) -> Tile {
        let index = sector.slot.grid_index();
        let gx = LEFT + (index % GRID_COLUMNS) as f64 * CELL_W + CELL_W / 2.0 - WIDTH / 2.0;
        let gy = self.header + ((index / GRID_COLUMNS) as f64 + 0.5) * CELL_H
            - (self.header + GRID_H + FOOTER) * 0.5;
        let (x, y, emerge_alpha, emerge_scale) = if sector.slot.born_in_box() {
            let expand = lerp(0.76, 1.0, self.box_t);
            (
                gx * expand,
                gy * expand,
                smoothstep(0.18, 0.62, self.box_t),
                lerp(0.86, 1.0, self.box_t),
            )
        } else {
            let angle = sector
                .slot
                .ring_angle()
                .unwrap_or((sector.start + sector.end) * 0.5);
            let (emerge_alpha, emerge_scale) = if sector.slot.ring_sibling() {
                let a = smoothstep(0.0, 0.22, self.box_t);
                (a, lerp(0.88, 1.0, a))
            } else {
                (1.0, 1.0)
            };
            (
                lerp(ITEM_RADIUS * angle.cos(), gx, self.box_t),
                lerp(ITEM_RADIUS * angle.sin(), gy, self.box_t),
                emerge_alpha,
                emerge_scale,
            )
        };
        Tile {
            x,
            y,
            width: lerp(84.0, CELL_W - 8.0, self.box_t),
            height: lerp(72.0, CELL_H - 8.0, self.box_t),
            emerge_alpha,
            emerge_scale,
        }
    }

    pub fn hit(&self, pos: (f64, f64), motion: &Motion) -> Option<usize> {
        let (x, y) = self.local(pos);
        if !self.inside(x, y) {
            return None;
        }
        if self.box_t < 0.12 {
            return motion.hit(y.atan2(x));
        }
        // Later tiles are painted on top if their paths cross mid-transition.
        [true, false]
            .into_iter()
            .flat_map(|siblings| {
                motion
                    .sectors()
                    .filter(move |s| s.slot.ring_sibling() == siblings)
            })
            .filter_map(|sector| {
                let index = sector.slot.index?;
                let tile = self.tile(&sector);
                (tile.emerge_alpha >= 0.5
                    && tile.contains(x, y)
                    && sector.slot.faces.last()?.opacity >= 0.5)
                    .then_some(index)
            })
            .last()
    }

    fn grid_top(&self) -> f64 {
        -self.height / 2.0 + self.header
    }

    fn thumb(&self, first: usize, total: usize) -> Option<(f64, f64)> {
        let rows = total.div_ceil(GRID_COLUMNS);
        if rows <= GRID_ROWS || self.box_t < 0.96 {
            return None;
        }
        let track_h = GRID_H - 8.0;
        let height = (track_h * GRID_ROWS as f64 / rows as f64).max(26.0);
        let top = self.grid_top()
            + 4.0
            + (track_h - height) * (first / GRID_COLUMNS) as f64 / (rows - GRID_ROWS) as f64;
        Some((top, height))
    }

    pub fn grab_thumb(&self, pos: (f64, f64), first: usize, total: usize) -> Option<f64> {
        let (top, height) = self.thumb(first, total)?;
        let (x, y) = self.local(pos);
        if !(self.width / 2.0 - 16.0..self.width / 2.0).contains(&x)
            || !(self.grid_top()..self.grid_top() + GRID_H).contains(&y)
        {
            return None;
        }
        let offset = y - top;
        Some(if (0.0..height).contains(&offset) {
            offset
        } else {
            height / 2.0
        })
    }

    pub fn dragged_first(&self, pos: (f64, f64), grab: f64, total: usize) -> usize {
        let Some((_, height)) = self.thumb(0, total) else {
            return 0;
        };
        let track_h = GRID_H - 8.0;
        let fraction = ((self.local(pos).1 - (self.grid_top() + 4.0) - grab)
            / (track_h - height))
            .clamp(0.0, 1.0);
        (fraction * (total.div_ceil(GRID_COLUMNS) - GRID_ROWS) as f64).round() as usize
            * GRID_COLUMNS
    }

    fn list_body_height(&self) -> f64 {
        self.list_rows * LIST_ROW_H
    }

    fn list_row(&self, sector: &Sector<'_>, has_scrollbar: bool) -> Tile {
        let index = sector.slot.last_index();
        let right_pad = if has_scrollbar { 20.0 } else { 14.0 };
        let row_w = LIST_W - 14.0 - right_pad;
        let rx = (14.0 - right_pad) * 0.5;
        let body_h = self.list_body_height();
        let ry = self.header + (index as f64 + 0.5) * LIST_ROW_H
            - (self.header + body_h + LIST_FOOTER) * 0.5;
        let (x, y, emerge_alpha, emerge_scale) = if sector.slot.born_in_box() {
            let expand = lerp(0.84, 1.0, self.list_t);
            (
                rx * expand,
                ry * expand,
                smoothstep(0.18, 0.62, self.list_t),
                lerp(0.90, 1.0, self.list_t),
            )
        } else {
            let angle = (sector.start + sector.end) * 0.5;
            (
                lerp(ITEM_RADIUS * angle.cos(), rx, self.list_t),
                lerp(ITEM_RADIUS * angle.sin(), ry, self.list_t),
                1.0,
                1.0,
            )
        };
        Tile {
            x,
            y,
            width: lerp(84.0, row_w, self.list_t),
            height: lerp(64.0, LIST_ROW_H - 4.0, self.list_t),
            emerge_alpha,
            emerge_scale,
        }
    }

    pub fn hit_list(&self, pos: (f64, f64), total: usize, motion: &Motion) -> Option<usize> {
        let (x, y) = self.local(pos);
        if !self.inside(x, y) {
            return None;
        }
        if self.list_t < 0.12 {
            return motion.hit(y.atan2(x));
        }
        let has_scrollbar = total > LIST_ROWS;
        motion
            .sectors()
            .filter_map(|sector| {
                let index = sector.slot.index?;
                let row = self.list_row(&sector, has_scrollbar);
                (row.emerge_alpha >= 0.5
                    && row.contains(x, y)
                    && sector.slot.faces.last()?.opacity >= 0.5)
                    .then_some(index)
            })
            .last()
    }

    fn list_thumb(&self, first: usize, total: usize) -> Option<(f64, f64)> {
        if total <= LIST_ROWS || self.list_t < 0.96 {
            return None;
        }
        let body_h = self.list_body_height();
        let track_h = (body_h - 8.0).max(24.0);
        let height = (track_h * LIST_ROWS as f64 / total as f64).max(22.0);
        let top = self.grid_top()
            + 4.0
            + (track_h - height) * first.min(total - LIST_ROWS) as f64
                / (total - LIST_ROWS) as f64;
        Some((top, height))
    }

    pub fn grab_list_thumb(&self, pos: (f64, f64), first: usize, total: usize) -> Option<f64> {
        let (top, height) = self.list_thumb(first, total)?;
        let (x, y) = self.local(pos);
        let body_h = self.list_body_height();
        if !(self.width / 2.0 - 16.0..self.width / 2.0).contains(&x)
            || !(self.grid_top()..self.grid_top() + body_h).contains(&y)
        {
            return None;
        }
        let offset = y - top;
        Some(if (0.0..height).contains(&offset) {
            offset
        } else {
            height / 2.0
        })
    }

    pub fn dragged_list_first(&self, pos: (f64, f64), grab: f64, total: usize) -> usize {
        let Some((_, height)) = self.list_thumb(0, total) else {
            return 0;
        };
        let track_h = (self.list_body_height() - 8.0).max(24.0);
        let fraction = ((self.local(pos).1 - (self.grid_top() + 4.0) - grab)
            / (track_h - height))
            .clamp(0.0, 1.0);
        (fraction * (total - LIST_ROWS) as f64).round() as usize
    }

    fn input_field_rect(&self) -> (f64, f64, f64, f64) {
        let top = -self.height / 2.0;
        let x = -self.width / 2.0 + 18.0;
        let w = (self.width - 36.0).max(40.0);
        let body_h = (self.height - self.header - self.footer).max(40.0);
        let h = 40.0;
        let y = top + self.header + (body_h - h) * 0.5;
        (x, y, w, h)
    }

    fn color_sv_rect(&self) -> (f64, f64, f64, f64) {
        let top = -self.height / 2.0;
        let x = -self.width / 2.0 + 20.0;
        let w = (self.width - 40.0).max(80.0);
        let y = top + self.header + 10.0;
        (x, y, w, 172.0)
    }

    fn color_hue_rect(&self) -> (f64, f64, f64, f64) {
        let top = -self.height / 2.0;
        let x = -self.width / 2.0 + 20.0;
        let w = (self.width - 40.0).max(80.0);
        let y = top + self.header + 192.0;
        (x, y, w, 14.0)
    }

    fn color_hex_field_rect(&self) -> (f64, f64, f64, f64) {
        let top = -self.height / 2.0;
        let x = -self.width / 2.0 + 20.0;
        let w = (self.width - 40.0).max(40.0);
        let y = top + self.header + 216.0;
        (x, y, w, 34.0)
    }

    pub fn hit_color_sv(&self, pos: (f64, f64)) -> Option<(f64, f64)> {
        let (x, y) = self.local(pos);
        if !self.inside(x, y) || self.input_t < 0.45 {
            return None;
        }
        let (sx, sy, sw, sh) = self.color_sv_rect();
        if x >= sx - 8.0 && x <= sx + sw + 8.0 && y >= sy - 8.0 && y <= sy + sh + 8.0 {
            let s = ((x - sx) / sw).clamp(0.0, 1.0);
            let v = (1.0 - (y - sy) / sh).clamp(0.0, 1.0);
            return Some((s, v));
        }
        None
    }

    pub fn color_sv_at(&self, pos: (f64, f64)) -> (f64, f64) {
        let (x, y) = self.local(pos);
        let (sx, sy, sw, sh) = self.color_sv_rect();
        (
            ((x - sx) / sw).clamp(0.0, 1.0),
            (1.0 - (y - sy) / sh).clamp(0.0, 1.0),
        )
    }

    pub fn hit_color_hue(&self, pos: (f64, f64)) -> Option<f64> {
        let (x, y) = self.local(pos);
        if !self.inside(x, y) || self.input_t < 0.45 {
            return None;
        }
        let (hx, hy, hw, hh) = self.color_hue_rect();
        if x >= hx - 8.0 && x <= hx + hw + 8.0 && y >= hy - 8.0 && y <= hy + hh + 8.0 {
            let h = (((x - hx) / hw).clamp(0.0, 1.0) * 360.0).min(359.999);
            return Some(h);
        }
        None
    }

    pub fn color_hue_at(&self, pos: (f64, f64)) -> f64 {
        let (x, _) = self.local(pos);
        let (hx, _, hw, _) = self.color_hue_rect();
        (((x - hx) / hw).clamp(0.0, 1.0) * 360.0).min(359.999)
    }

    fn input_button_rects(&self) -> [(f64, f64, f64, f64); 2] {
        let bottom = self.height / 2.0;
        let y = bottom - 43.0;
        let h = 32.0;
        let inner_w = (self.width - 36.0).max(60.0);
        let gap = 12.0;
        let btn_w = (inner_w - gap) * 0.5;
        let x0 = -self.width / 2.0 + 18.0;
        let x1 = x0 + btn_w + gap;
        [(x0, y, btn_w, h), (x1, y, btn_w, h)]
    }

    pub fn hit_input_button(&self, pos: (f64, f64)) -> Option<usize> {
        let (x, y) = self.local(pos);
        if !self.inside(x, y) || self.input_t < 0.45 {
            return None;
        }
        for (i, (bx, by, bw, bh)) in self.input_button_rects().into_iter().enumerate() {
            if x >= bx && x <= bx + bw && y >= by && y <= by + bh {
                return Some(i);
            }
        }
        None
    }

    fn path(&self, cr: &Context) {
        cr.new_path();
        rounded_rect(
            cr,
            -self.width / 2.0,
            -self.height / 2.0,
            self.width,
            self.height,
            self.radius,
        );
        if self.hole > 0.01 {
            cr.new_sub_path();
            cr.arc_negative(0.0, 0.0, self.hole, TAU, 0.0);
            cr.close_path();
        }
    }

    fn blur_region(&self) -> Result<Region, cairo::Error> {
        let key = [
            self.cx.to_bits(),
            self.cy.to_bits(),
            self.scale.to_bits(),
            self.width.to_bits(),
            self.height.to_bits(),
            self.radius.to_bits(),
            self.hole.to_bits(),
        ];
        thread_local! {
            static CACHE: RefCell<Option<([u64; 7], Region)>> = const { RefCell::new(None) };
        }
        CACHE.with(|cache| {
            let mut slot = cache.borrow_mut();
            if let Some((cached_key, region)) = slot.as_ref()
                && *cached_key == key
            {
                return Ok(region.copy());
            }
            let pixel = |edge: f64| (edge - 0.5).ceil() as i32;
            let half = self.height * 0.5;
            let rows: Vec<_> =
                (pixel(self.cy - half * self.scale)..pixel(self.cy + half * self.scale))
                    .filter_map(|y| {
                        let local = (y as f64 + 0.5 - self.cy) / self.scale;
                        let dy = (local.abs() - (half - self.radius)).max(0.0);
                        let reach = self.width / 2.0 - self.radius
                            + (self.radius.powi(2) - dy.powi(2)).max(0.0).sqrt();
                        let left = pixel(self.cx - reach * self.scale);
                        let right = pixel(self.cx + reach * self.scale);
                        (right > left).then(|| RectangleInt::new(left, y, right - left, 1))
                    })
                    .collect();
            let region = Region::create_rectangles(&rows);
            if self.hole > 0.01 {
                let diameter = 2.0 * self.hole * self.scale;
                region.subtract(&capsule_region(self.cx, self.cy, diameter, diameter))?;
            }
            let out = region.copy();
            *slot = Some((key, region));
            Ok(out)
        })
    }

    /// Conservative logical-pixel bounding rectangle covering the morphing panel,
    /// its drop shadow, and fading ring captions while `t < 0.30`.
    pub fn damage_rect(&self) -> RectangleInt {
        if self.scale <= 0.0 {
            return RectangleInt::new(0, 0, 0, 0);
        }
        let mut hw = (self.width * 0.5 + 12.0) * self.scale;
        let mut hh_top = (self.height * 0.5 + 12.0) * self.scale;
        let mut hh_bot = (self.height * 0.5 + 13.0) * self.scale;
        if self.t < 0.30 {
            hw = hw.max((SIDE + 4.0) * self.scale);
            hh_top = hh_top.max((TOP + 4.0) * self.scale);
            hh_bot = hh_bot.max((BOTTOM + 4.0) * self.scale);
        }
        let x0 = (self.cx - hw).floor() as i32 - 2;
        let y0 = (self.cy - hh_top).floor() as i32 - 2;
        let x1 = (self.cx + hw).ceil() as i32 + 2;
        let y1 = (self.cy + hh_bot).ceil() as i32 + 2;
        RectangleInt::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }

    fn tile_strictly_inside(&self, tile: &Tile, draw_card: bool) -> bool {
        // Account for card stroke, hover zoom (1.04×), icon tilt, 2-line wrapped
        // labels, and list row dividers so content never leaks past rounded
        // corners or into the closing center hole.
        let pad_x = if draw_card { 3.0 } else { 2.0 };
        let pad_y = if draw_card { 4.0 } else { 6.0 };
        let base_hw = if self.box_t >= 0.45 && self.box_t < 0.999 {
            (tile.width * 0.5).max((CELL_W - 20.0) * 0.5)
        } else {
            tile.width * 0.5
        };
        let hw = (base_hw + pad_x) * tile.emerge_scale * 1.04;
        let hh = (tile.height * 0.5 + pad_y) * tile.emerge_scale * 1.04;
        let x0 = tile.x - hw;
        let x1 = tile.x + hw;
        let y0 = tile.y - hh;
        let y1 = tile.y + hh;
        let in_outer = |x: f64, y: f64| {
            let dx = (x.abs() - (self.width * 0.5 - self.radius)).max(0.0);
            let dy = (y.abs() - (self.height * 0.5 - self.radius)).max(0.0);
            dx.hypot(dy) <= self.radius
        };
        if !(in_outer(x0, y0) && in_outer(x1, y0) && in_outer(x0, y1) && in_outer(x1, y1)) {
            return false;
        }
        if self.hole > 0.01 {
            let cx = 0.0_f64.clamp(x0, x1);
            let cy = 0.0_f64.clamp(y0, y1);
            if cx.hypot(cy) < self.hole + 2.0 {
                return false;
            }
        }
        true
    }
}

struct Tile {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    emerge_alpha: f64,
    emerge_scale: f64,
}

impl Tile {
    fn contains(&self, x: f64, y: f64) -> bool {
        (x - self.x).abs() <= self.width / 2.0 && (y - self.y).abs() <= self.height / 2.0
    }
}

fn label(
    cr: &Context,
    layout: &gtk::pango::Layout,
    x: f64,
    cy: f64,
    width: f64,
    tint: Rgb,
    alpha: f64,
) {
    layout.set_width((width * f64::from(SCALE)) as i32);
    if layout.ellipsize() == EllipsizeMode::None {
        layout.set_ellipsize(EllipsizeMode::End);
    }
    color(cr, tint, alpha);
    cr.move_to(x, cy - f64::from(layout.pixel_size().1) / 2.0);
    pangocairo::functions::show_layout(cr, layout);
    cr.new_path();
}

#[allow(clippy::too_many_arguments)]
fn draw_cached_label(
    cr: &Context,
    text: &str,
    size: f64,
    bold: bool,
    x: f64,
    cy: f64,
    width: f64,
    ellipsize: EllipsizeMode,
    tint: Rgb,
    alpha: f64,
) -> Result<(), cairo::Error> {
    if text.is_empty() || alpha <= 0.004 {
        return Ok(());
    }
    let width_px = width.floor().max(1.0) as i32;
    let (nat_w, _) = measure_text(text, size, bold);
    let (key_w, key_ellip) = if nat_w <= width_px {
        (0, EllipsizeMode::None)
    } else {
        (width_px, ellipsize)
    };
    if let Some(cached) = cached_text_surface(
        cr,
        text,
        size,
        bold,
        key_w,
        0,
        key_ellip,
        Alignment::Left,
        tint,
        None,
    )? {
        let ty = cy - f64::from(cached.logical_h) * 0.5;
        paint_surface(cr, &cached.surface, x + cached.origin_x, ty + cached.origin_y, alpha)?;
    }
    Ok(())
}

fn draw_soft_divider(cr: &Context, width: f64, y: f64, rgb: Rgb, alpha: f64) -> Result<(), cairo::Error> {
    let half = width / 2.0 - 16.0;
    let grad = LinearGradient::new(-half, y, half, y);
    grad.add_color_stop_rgba(0.0, rgb.0, rgb.1, rgb.2, 0.0);
    grad.add_color_stop_rgba(0.12, rgb.0, rgb.1, rgb.2, alpha);
    grad.add_color_stop_rgba(0.88, rgb.0, rgb.1, rgb.2, alpha);
    grad.add_color_stop_rgba(1.0, rgb.0, rgb.1, rgb.2, 0.0);
    cr.set_source(&grad)?;
    cr.set_line_width(1.0);
    cr.move_to(-half, y);
    cr.line_to(half, y);
    cr.stroke()
}

pub struct GridState {
    pub first: usize,
    pub total: usize,
    pub selected: Option<usize>,
    pub shortcuts: bool,
    pub dragging_thumb: bool,
}

fn draw_box_shadow(cr: &Context, b: Bounds, p: &Palette, fast_morph: bool) -> Result<(), cairo::Error> {
    // Graduated shadow outside the morphing glass outline (matches ring_material at t=0).
    let prev_tol = cr.tolerance();
    if fast_morph {
        cr.set_tolerance(0.35);
        cr.set_line_width(3.6);
        for (expand, weight) in [(1.8, 0.90), (5.0, 0.34)] {
            cr.new_path();
            rounded_rect(
                cr,
                -b.width / 2.0 - expand,
                -b.height / 2.0 - expand + 1.0,
                b.width + 2.0 * expand,
                b.height + 2.0 * expand,
                b.radius + expand,
            );
            color(cr, p.shadow, 0.13 * weight);
            cr.stroke()?;
        }
        cr.set_tolerance(prev_tol);
        return Ok(());
    }
    cr.set_line_width(2.8);
    for (expand, weight) in [(1.4, 0.85), (4.0, 0.50), (6.6, 0.18)] {
        cr.new_path();
        rounded_rect(
            cr,
            -b.width / 2.0 - expand,
            -b.height / 2.0 - expand + 1.0,
            b.width + 2.0 * expand,
            b.height + 2.0 * expand,
            b.radius + expand,
        );
        color(cr, p.shadow, 0.13 * weight);
        cr.stroke()?;
    }
    Ok(())
}

fn draw_box_glass(
    cr: &Context,
    b: Bounds,
    material: Material,
    p: &Palette,
) -> Result<(), cairo::Error> {
    b.path(cr);
    let glass = LinearGradient::new(0.0, -b.height / 2.0, 0.0, b.height / 2.0);
    for (at, (r, g, blue), ring_alpha, box_frosted, box_translucent) in [
        (0.0, p.ring_top, 0.61, 0.78, 0.91),
        (0.45, p.ring_mid, 0.55, 0.74, 0.89),
        (1.0, p.ring_bottom, 0.63, 0.80, 0.93),
    ] {
        glass.add_color_stop_rgba(
            at,
            r,
            g,
            blue,
            material.opacity(
                lerp(ring_alpha, box_frosted, b.t),
                lerp(ring_alpha + 0.1, box_translucent, b.t),
            ),
        );
    }
    cr.set_source(&glass)?;
    cr.fill()
}

fn draw_top_sheen(
    cr: &Context,
    b: Bounds,
    p: &Palette,
    alpha_scale: f64,
) -> Result<(), cairo::Error> {
    let top = -b.height / 2.0;
    let sheen = LinearGradient::new(0.0, top, 0.0, top + 28.0);
    sheen.add_color_stop_rgba(
        0.0,
        p.rim_top.0,
        p.rim_top.1,
        p.rim_top.2,
        0.08 * alpha_scale,
    );
    sheen.add_color_stop_rgba(1.0, p.rim_top.0, p.rim_top.1, p.rim_top.2, 0.0);
    cr.new_path();
    if b.radius <= 28.0 {
        let left = -b.width / 2.0;
        let right = b.width / 2.0;
        let r = b.radius;
        cr.arc(right - r, top + r, r, -FRAC_PI_2, 0.0);
        cr.line_to(right, top + 28.0);
        cr.line_to(left, top + 28.0);
        cr.arc(left + r, top + r, r, PI, PI + FRAC_PI_2);
        cr.close_path();
        cr.set_source(&sheen)?;
        cr.fill()
    } else {
        cr.save()?;
        rounded_rect(cr, -b.width / 2.0, top, b.width, b.height, b.radius);
        cr.clip();
        cr.rectangle(-b.width / 2.0, top, b.width, 28.0);
        cr.set_source(&sheen)?;
        cr.fill()?;
        cr.restore()
    }
}

fn draw_outer_rim(
    cr: &Context,
    b: Bounds,
    high_contrast: bool,
    p: &Palette,
) -> Result<(), cairo::Error> {
    let rim = LinearGradient::new(0.0, -b.height / 2.0, 0.0, b.height / 2.0);
    for (at, (r, g, blue), alpha) in [(0.0, p.rim_top, 0.70), (1.0, p.rim_bottom, 0.30)] {
        rim.add_color_stop_rgba(at, r, g, blue, if high_contrast { 1.0 } else { alpha });
    }
    cr.new_path();
    rounded_rect(
        cr,
        -b.width / 2.0,
        -b.height / 2.0,
        b.width,
        b.height,
        b.radius,
    );
    cr.set_source(&rim)?;
    cr.set_line_width(if high_contrast { 1.8 } else { 1.0 });
    cr.stroke()
}

const BOX_SHELL_PAD: f64 = 14.0;

#[derive(Clone, Copy, Eq, PartialEq)]
struct SettledBoxKey {
    width: u64,
    height: u64,
    radius: u64,
    header: u64,
    footer: u64,
    material: Material,
    high_contrast: bool,
    ring_top: [u64; 3],
    ring_mid: [u64; 3],
    ring_bottom: [u64; 3],
    rim_top: [u64; 3],
    rim_bottom: [u64; 3],
    divider: [u64; 3],
    shadow: [u64; 3],
    sx: u64,
    sy: u64,
}

fn draw_settled_box_shell_cached(
    cr: &Context,
    b: Bounds,
    material: Material,
    high_contrast: bool,
    p: &Palette,
) -> Result<(), cairo::Error> {
    thread_local! {
        static CACHE: RefCell<Option<(SettledBoxKey, ImageSurface)>> = const { RefCell::new(None) };
    }
    let (sx, sy) = surface_device_scale(cr);
    let key = SettledBoxKey {
        width: b.width.to_bits(),
        height: b.height.to_bits(),
        radius: b.radius.to_bits(),
        header: b.header.to_bits(),
        footer: b.footer.to_bits(),
        material,
        high_contrast,
        ring_top: rgb_bits(p.ring_top),
        ring_mid: rgb_bits(p.ring_mid),
        ring_bottom: rgb_bits(p.ring_bottom),
        rim_top: rgb_bits(p.rim_top),
        rim_bottom: rgb_bits(p.rim_bottom),
        divider: rgb_bits(p.divider),
        shadow: rgb_bits(p.shadow),
        sx: sx.to_bits(),
        sy: sy.to_bits(),
    };
    let half_w = b.width * 0.5 + BOX_SHELL_PAD;
    let half_h = b.height * 0.5 + BOX_SHELL_PAD;
    let surface = CACHE.with(|cache| -> Result<ImageSurface, cairo::Error> {
        let mut slot = cache.borrow_mut();
        if let Some((cached_key, surf)) = slot.as_ref()
            && *cached_key == key
        {
            return Ok(surf.clone());
        }
        let dim_x = (half_w * 2.0 * sx).ceil() as i32;
        let dim_y = (half_h * 2.0 * sy).ceil() as i32;
        let surf = ImageSurface::create(Format::ARgb32, dim_x, dim_y)?;
        surf.set_device_scale(sx, sy);
        {
            let ctx = Context::new(&surf)?;
            ctx.translate(half_w, half_h);
            draw_box_shadow(&ctx, b, p, false)?;
            draw_box_glass(&ctx, b, material, p)?;
            if !high_contrast {
                draw_top_sheen(&ctx, b, p, 1.0)?;
            }
            draw_soft_divider(
                &ctx,
                b.width,
                b.grid_top() - 4.0,
                p.divider,
                if high_contrast { 0.45 } else { 0.14 },
            )?;
            if b.footer >= 28.0 {
                draw_soft_divider(
                    &ctx,
                    b.width,
                    b.height / 2.0 - b.footer + 2.0,
                    p.divider,
                    if high_contrast { 0.40 } else { 0.11 },
                )?;
            }
            draw_outer_rim(&ctx, b, high_contrast, p)?;
        }
        *slot = Some((key, surf.clone()));
        Ok(surf)
    })?;
    paint_surface(cr, &surface, -half_w, -half_h, 1.0)
}

fn draw_tile(
    cr: &Context,
    bounds: Bounds,
    sector: Sector<'_>,
    view: &View<'_>,
    grid: &GridState,
) -> Result<(), cairo::Error> {
    let slot = sector.slot;
    let i = slot.index;
    let tile = bounds.tile(&sector);
    if tile.emerge_alpha <= 0.005 {
        return Ok(());
    }
    let tile_alpha = tile.emerge_alpha;
    let pressed = i.is_some() && view.pressed == i;
    let selected = i.is_some() && grid.selected == i;
    let p = view.palette;
    let card_mix = smoothstep(0.18, 0.58, bounds.box_t);
    let draw_card = (pressed || selected) && card_mix > 0.0 && slot.faces.last().is_some();
    let badge_alpha = smoothstep(0.74, 0.96, bounds.box_t);
    let draw_badge = grid.shortcuts
        && (i.is_some() || slot.merging_to_ring())
        && slot.grid_index() < PAGE_SIZE
        && badge_alpha > 0.0;
    let clip_to_box = !bounds.tile_strictly_inside(&tile, draw_card);

    cr.save()?;
    if clip_to_box {
        bounds.path(cr);
        cr.clip();
    }
    cr.translate(tile.x, tile.y);
    if tile.emerge_scale < 0.999 {
        cr.scale(tile.emerge_scale, tile.emerge_scale);
    }
    if draw_card && let Some(face) = slot.faces.last() {
        let tint = p.accent(face.item.icon);
        let focus = slot.focus().clamp(0.0, 1.0);
        let fill_alpha = if pressed {
            0.27
        } else {
            0.11 + 0.08 * focus
        } * card_mix
            * tile_alpha;
        let stroke_alpha = if view.high_contrast { 1.0 } else { 0.50 } * card_mix * tile_alpha;
        cr.set_line_width(if view.high_contrast { 1.5 } else { 1.0 });
        rounded_rect(
            cr,
            0.5 - tile.width / 2.0,
            0.5 - tile.height / 2.0,
            tile.width - 1.0,
            tile.height - 1.0,
            12.0,
        );
        color(cr, tint, fill_alpha);
        cr.fill_preserve()?;
        color(cr, tint, stroke_alpha);
        cr.stroke()?;
    }
    let icon_y = lerp(-9.0, -15.0, bounds.box_t);
    let plate = lerp(32.0, 48.0, bounds.box_t);
    let icon_size = lerp(20.0, 26.0, bounds.box_t);
    let focus = slot.focus().clamp(0.0, 1.0);
    let bold = bounds.box_t < 0.5;
    let wrap = bounds.box_t >= 0.45;
    let label_y = lerp(11.0, 14.5, bounds.box_t);
    let avail_w = tile.width - 12.0;
    for face in &slot.faces {
        let alpha = face.opacity * tile_alpha;
        if alpha <= 0.008 {
            continue;
        }
        let item = &face.item;
        cr.save()?;
        let zoom = if pressed { 0.96 } else { 1.0 + 0.04 * focus };
        if (zoom - 1.0).abs() >= 1e-4 {
            cr.scale(zoom, zoom);
        }
        cr.translate(0.0, icon_y);
        let tilt = slot.tilt();
        if tilt.abs() >= 0.003 {
            cr.rotate(tilt);
        }
        draw_icon_plate(cr, item, 0.0, plate, icon_size, p, alpha)?;
        cr.restore()?;

        // Cache-friendly tile labels: short labels share one unconstrained surface
        // across the entire morph; wrapped labels use settled tile width (122px)
        // so words never jitter between lines while tiles glide into place.
        let (nat_w, _) = measure_text(&item.label, 12.5, bold);
        let (key_w, wrap_lines, align, ellipsize) = if f64::from(nat_w) <= avail_w {
            (0, 0, Alignment::Left, EllipsizeMode::None)
        } else if wrap {
            ((CELL_W - 20.0) as i32, 2, Alignment::Center, EllipsizeMode::End)
        } else {
            let quantized = ((avail_w / 16.0).floor() as i32 * 16).max(48);
            (quantized, 0, Alignment::Center, EllipsizeMode::End)
        };
        if let Some(cached) = cached_text_surface(
            cr,
            &item.label,
            12.5,
            bold,
            key_w,
            wrap_lines,
            ellipsize,
            align,
            p.text_primary,
            Some((p.text_shadow, 0.72)),
        )? {
            let tx = if key_w == 0 {
                -f64::from(cached.logical_w) * 0.5
            } else {
                -f64::from(key_w) * 0.5
            };
            paint_surface(
                cr,
                &cached.surface,
                tx + cached.origin_x,
                label_y + cached.origin_y,
                alpha,
            )?;
        }
    }
    if draw_badge {
        const SHORTCUTS: [&str; 8] = ["1", "2", "3", "4", "5", "6", "7", "8"];
        if let Some(&badge) = SHORTCUTS.get(slot.grid_index()) {
            draw_cached_label(
                cr,
                badge,
                10.0,
                false,
                tile.width / 2.0 - 16.0,
                -tile.height / 2.0 + 13.0,
                12.0,
                EllipsizeMode::End,
                if selected || pressed {
                    p.text_primary
                } else {
                    p.text_suggestion
                },
                (if selected || pressed { 0.92 } else { 0.62 }) * badge_alpha * tile_alpha,
            )?;
        }
    }
    cr.restore()
}

pub fn draw(
    cr: &Context,
    b: Bounds,
    view: &View<'_>,
    grid: &GridState,
) -> Result<Region, cairo::Error> {
    if b.scale <= 0.0 {
        return Ok(Region::create());
    }
    let p = view.palette;
    let material = Material::for_view(view);
    let settled = b.t >= 0.9999 && b.hole == 0.0 && b.box_t >= 0.9999;
    cr.save()?;
    cr.translate(b.cx, b.cy);
    cr.scale(b.scale, b.scale);

    if settled {
        draw_settled_box_shell_cached(cr, b, material, view.high_contrast, p)?;
    } else {
        draw_box_shadow(cr, b, p, true)?;
        let prev_tol = cr.tolerance();
        cr.set_tolerance(0.35);
        draw_box_glass(cr, b, material, p)?;
        cr.set_tolerance(prev_tol);
    }

    // Fade out only the ring's radial dividers & outer highlight arc; items themselves
    // are rendered once below so they glide seamlessly into the grid without ghosting.
    let ring_chrome = (1.0 - b.t / 0.28).clamp(0.0, 1.0);
    if ring_chrome > 0.0 {
        cr.save()?;
        b.path(cr);
        cr.clip();
        for sector_view in view.motion.sectors() {
            if sector_view.slot.born_in_box() || sector_view.slot.merging_to_ring() {
                continue;
            }
            if let Some((start, end)) = sector_view.slot.ring_sector() {
                let ring_sec = Sector {
                    slot: sector_view.slot,
                    start,
                    end,
                };
                draw_sector_chrome(cr, &ring_sec, view, p, ring_chrome)?;
            } else if sector_view.slot.ring_angle().is_none() {
                draw_sector_chrome(cr, &sector_view, view, p, ring_chrome)?;
            }
        }
        cr.restore()?;
    }

    for siblings in [true, false] {
        for sector in view.motion.sectors() {
            if sector.slot.ring_sibling() != siblings {
                continue;
            }
            if sector.slot.index.is_some()
                || sector.slot.merging_to_ring()
                || b.box_t < 0.45
                || (view.count == 0 && sector.slot.born_in_box())
            {
                draw_tile(cr, b, sector, view, grid)?;
            }
        }
    }

    let box_chrome = smoothstep(0.65, 0.96, b.box_t);
    if box_chrome > 0.0 {
        let x = -b.width / 2.0 + 20.0;
        let top = -b.height / 2.0;

        // Subtle inner top specular catchlight for translucent glass depth.
        if !settled && !view.high_contrast {
            draw_top_sheen(cr, b, p, box_chrome)?;
        }

        let range_text = if grid.total > 0 {
            format!(
                "{}–{} / {}",
                grid.first + 1,
                grid.first + view.count,
                grid.total
            )
        } else {
            String::new()
        };
        let range_width = f64::from(measure_text(&range_text, 11.5, false).0) + 4.0;
        let right = b.width / 2.0 - 20.0 - range_width;
        draw_cached_label(
            cr,
            &range_text,
            11.5,
            false,
            right,
            top + 23.0,
            range_width,
            EllipsizeMode::End,
            p.text_suggestion,
            0.90 * box_chrome,
        )?;
        let title = if view.title.is_empty() {
            "Main menu"
        } else {
            view.title
        };
        draw_cached_label(
            cr,
            title,
            16.0,
            true,
            x,
            top + 23.0,
            (right - x - 12.0).max(20.0),
            EllipsizeMode::End,
            p.text_primary,
            box_chrome,
        )?;

        if !view.query.is_empty() {
            let bar_x = -b.width / 2.0 + 18.0;
            let bar_y = top + 40.0;
            let bar_w = b.width - 36.0;
            let bar_h = 28.0;
            if box_chrome < 1.0 {
                cr.save()?;
                cr.rectangle(bar_x - 2.0, bar_y - 2.0, bar_w + 4.0, bar_h + 4.0);
                cr.clip();
                cr.push_group();
            }
            rounded_rect(cr, bar_x, bar_y, bar_w, bar_h, 8.0);
            let bar_bg = LinearGradient::new(0.0, bar_y, 0.0, bar_y + bar_h);
            bar_bg.add_color_stop_rgba(
                0.0,
                p.capsule_top.0,
                p.capsule_top.1,
                p.capsule_top.2,
                material.opacity(0.55, 0.72),
            );
            bar_bg.add_color_stop_rgba(
                1.0,
                p.capsule_bottom.0,
                p.capsule_bottom.1,
                p.capsule_bottom.2,
                material.opacity(0.65, 0.80),
            );
            cr.set_source(&bar_bg)?;
            cr.fill_preserve()?;
            cr.set_line_width(if view.high_contrast { 1.2 } else { 0.8 });
            color(
                cr,
                p.capsule_border,
                if view.high_contrast { 0.85 } else { 0.28 },
            );
            cr.stroke()?;

            let path = text_layout(cr, view.query, 13.0, false);
            add_suggestion(&path, view.query, view.suggestion, p);
            path.set_ellipsize(if view.suggestion.is_empty() {
                EllipsizeMode::Start
            } else {
                EllipsizeMode::Middle
            });
            label(
                cr,
                &path,
                bar_x + 11.0,
                bar_y + bar_h / 2.0,
                bar_w - 22.0,
                p.text_capsule,
                1.0,
            );
            if box_chrome < 1.0 {
                cr.pop_group_to_source()?;
                cr.paint_with_alpha(box_chrome)?;
                cr.restore()?;
            }
        }

        if !settled {
            draw_soft_divider(
                cr,
                b.width,
                b.grid_top() - 4.0,
                p.divider,
                (if view.high_contrast { 0.45 } else { 0.14 }) * box_chrome,
            )?;
        }

        if let Some((thumb_top, height)) = b.thumb(grid.first, grid.total) {
            let track_x = b.width / 2.0 - 10.0;
            rounded_rect(cr, track_x, b.grid_top() + 4.0, 4.0, GRID_H - 8.0, 2.0);
            color(
                cr,
                p.divider,
                (if view.high_contrast { 0.30 } else { 0.09 }) * box_chrome,
            );
            cr.fill()?;

            rounded_rect(cr, track_x, thumb_top, 4.0, height, 2.0);
            color(
                cr,
                p.rim_top,
                (if view.high_contrast {
                    0.95
                } else if grid.dragging_thumb {
                    0.85
                } else {
                    0.50
                }) * box_chrome,
            );
            cr.fill()?;
        }

        if !settled {
            draw_soft_divider(
                cr,
                b.width,
                b.height / 2.0 - FOOTER + 2.0,
                p.divider,
                (if view.high_contrast { 0.40 } else { 0.11 }) * box_chrome,
            )?;
        }

        if !view.status.is_empty() {
            draw_cached_label(
                cr,
                view.status,
                11.5,
                false,
                x,
                b.height / 2.0 - FOOTER / 2.0 + 1.0,
                b.width - 40.0,
                EllipsizeMode::Middle,
                p.text_suggestion,
                0.95 * box_chrome,
            )?;
        }
    }

    if !settled {
        let prev_tol = cr.tolerance();
        cr.set_tolerance(0.35);
        // Outer glass rim (matches ring::draw rim stroke).
        draw_outer_rim(cr, b, view.high_contrast, p)?;

        // Inner hole rim fades smoothly as the hole closes.
        if b.hole > 2.0 {
            let hole_alpha = smoothstep(2.0, INNER * 0.65, b.hole);
            let hole_rim = LinearGradient::new(0.0, -b.hole, 0.0, b.hole);
            for (at, (r, g, blue), alpha) in [(0.0, p.rim_top, 0.70), (1.0, p.rim_bottom, 0.27)] {
                hole_rim.add_color_stop_rgba(
                    at,
                    r,
                    g,
                    blue,
                    hole_alpha * if view.high_contrast { 1.0 } else { alpha },
                );
            }
            cr.new_path();
            cr.arc(0.0, 0.0, b.hole, 0.0, TAU);
            cr.close_path();
            cr.set_source(&hole_rim)?;
            cr.set_line_width(if view.high_contrast { 1.8 } else { 1.0 });
            cr.stroke()?;
        }
        cr.set_tolerance(prev_tol);
    }

    let mask = if material == Material::Frosted {
        b.blur_region()?
    } else {
        Region::create()
    };
    if ring_chrome > 0.0
        && (!view.title.is_empty() || !view.query.is_empty() || !view.status.is_empty())
    {
        let capsule_blur = (ring_chrome > 0.5).then_some(&mask);
        draw_captions(cr, b.cx, b.cy, b.scale, view, capsule_blur, ring_chrome)?;
    }
    cr.restore()?;
    Ok(mask)
}

pub struct ListState {
    pub first: usize,
    pub total: usize,
    pub selected: Option<usize>,
    pub dragging_thumb: bool,
}

fn draw_list_row(
    cr: &Context,
    bounds: Bounds,
    sector: Sector<'_>,
    view: &View<'_>,
    list: &ListState,
) -> Result<(), cairo::Error> {
    let slot = sector.slot;
    let i = slot.index;
    let has_scrollbar = list.total > LIST_ROWS;
    let row = bounds.list_row(&sector, has_scrollbar);
    if row.emerge_alpha <= 0.005 {
        return Ok(());
    }
    let row_alpha = row.emerge_alpha;
    let pressed = i.is_some() && view.pressed == i;
    let selected = i.is_some() && list.selected == i;
    let p = view.palette;
    let card_mix = smoothstep(0.18, 0.58, bounds.list_t);
    let draw_card = (pressed || selected) && card_mix > 0.0 && slot.faces.last().is_some();
    let clip_to_box = !bounds.tile_strictly_inside(&row, draw_card);

    cr.save()?;
    if clip_to_box {
        bounds.path(cr);
        cr.clip();
    }
    cr.translate(row.x, row.y);
    let focus = slot.focus().clamp(0.0, 1.0);
    let zoom = if pressed {
        0.985
    } else {
        1.0 + 0.012 * focus * card_mix
    };
    let total_scale = row.emerge_scale * zoom;
    if (total_scale - 1.0).abs() >= 1e-4 {
        cr.scale(total_scale, total_scale);
    }

    if draw_card && let Some(face) = slot.faces.last() {
        let tint = p.accent(face.item.icon);
        let fill_alpha = if pressed {
            0.27
        } else {
            0.13 + 0.08 * focus
        } * card_mix
            * row_alpha;
        let stroke_alpha = if view.high_contrast { 1.0 } else { 0.52 } * card_mix * row_alpha;
        cr.set_line_width(if view.high_contrast { 1.5 } else { 1.0 });
        rounded_rect(
            cr,
            0.5 - row.width / 2.0,
            0.5 - row.height / 2.0,
            row.width - 1.0,
            row.height - 1.0,
            10.0,
        );
        color(cr, tint, fill_alpha);
        cr.fill_preserve()?;
        color(cr, tint, stroke_alpha);
        cr.stroke()?;

        // Subtle left accent bar inside the selected option row.
        rounded_rect(cr, -row.width / 2.0 + 7.0, -8.0, 3.0, 16.0, 1.5);
        color(cr, tint, (0.72 + 0.25 * focus) * card_mix * row_alpha);
        cr.fill()?;
    } else if bounds.t > 0.65 && i.is_some_and(|idx| idx + 1 < view.count) {
        let div_alpha = (if view.high_contrast { 0.22 } else { 0.07 })
            * smoothstep(0.65, 0.95, bounds.t)
            * row_alpha;
        cr.set_line_width(1.0);
        color(cr, p.divider, div_alpha);
        cr.move_to(-row.width / 2.0 + 14.0, row.height / 2.0 + 1.5);
        cr.line_to(row.width / 2.0 - 14.0, row.height / 2.0 + 1.5);
        cr.stroke()?;
    }

    // No icons in the pick & choose menu; only fade out departing ring icons during early morph.
    let departing_icon_alpha = if slot.born_in_box() {
        0.0
    } else {
        (1.0 - bounds.t / 0.28).clamp(0.0, 1.0)
    };

    for face in &slot.faces {
        let alpha = face.opacity * row_alpha;
        if alpha <= 0.008 {
            continue;
        }
        let item = &face.item;
        if departing_icon_alpha > 0.01 {
            draw_icon_plate(
                cr,
                item,
                lerp(-9.0, 0.0, bounds.t),
                32.0,
                20.0,
                p,
                alpha * departing_icon_alpha,
            )?;
        }

        let bold = selected || pressed || item.active || bounds.t < 0.45;
        let left_pad = 16.0;
        let right_reserve = if item.active { 32.0 } else { left_pad };
        let text_w = (row.width - left_pad - right_reserve).max(24.0);
        let width_px = text_w.floor().max(1.0) as i32;
        let (nat_w, _) = measure_text(&item.label, 13.5, bold);
        let (key_w, key_ellip) = if nat_w <= width_px {
            (0, EllipsizeMode::None)
        } else {
            (width_px, EllipsizeMode::End)
        };
        if let Some(cached) = cached_text_surface(
            cr,
            &item.label,
            13.5,
            bold,
            key_w,
            0,
            key_ellip,
            Alignment::Left,
            p.text_primary,
            Some((p.text_shadow, 0.72)),
        )? {
            let centered_x = -f64::from(cached.logical_w).min(text_w) * 0.5;
            let aligned_x = -row.width / 2.0 + left_pad;
            let text_x = lerp(centered_x, aligned_x, smoothstep(0.15, 0.65, bounds.t));
            let text_y = lerp(11.0, 0.0, smoothstep(0.0, 0.45, bounds.t))
                - f64::from(cached.logical_h) * 0.5;
            paint_surface(
                cr,
                &cached.surface,
                text_x + cached.origin_x,
                text_y + cached.origin_y,
                alpha,
            )?;
        }

        if item.active {
            let dot_alpha = smoothstep(0.45, 0.85, bounds.list_t) * alpha;
            if dot_alpha > 0.01 {
                let dot_x = row.width / 2.0 - 16.0;
                let tint = p.accent(item.icon);
                cr.new_sub_path();
                cr.arc(dot_x, 0.0, 3.6, 0.0, TAU);
                color(cr, tint, (if view.high_contrast { 1.0 } else { 0.92 }) * dot_alpha);
                cr.fill()?;
            }
        }
    }
    cr.restore()
}

pub fn draw_list(
    cr: &Context,
    b: Bounds,
    view: &View<'_>,
    list: &ListState,
) -> Result<Region, cairo::Error> {
    if b.scale <= 0.0 {
        return Ok(Region::create());
    }
    let p = view.palette;
    let material = Material::for_view(view);
    let settled = b.t >= 0.9999 && b.hole == 0.0 && b.list_t >= 0.9999 && b.list_rows_settled;
    cr.save()?;
    cr.translate(b.cx, b.cy);
    cr.scale(b.scale, b.scale);

    if settled {
        draw_settled_box_shell_cached(cr, b, material, view.high_contrast, p)?;
    } else {
        draw_box_shadow(cr, b, p, true)?;
        let prev_tol = cr.tolerance();
        cr.set_tolerance(0.35);
        draw_box_glass(cr, b, material, p)?;
        cr.set_tolerance(prev_tol);
    }

    let ring_chrome = (1.0 - b.t / 0.28).clamp(0.0, 1.0);
    if ring_chrome > 0.0 {
        cr.save()?;
        b.path(cr);
        cr.clip();
        for sector_view in view.motion.sectors() {
            if !sector_view.slot.born_in_box() {
                draw_sector_chrome(cr, &sector_view, view, p, ring_chrome)?;
            }
        }
        cr.restore()?;
    }

    for sector in view.motion.sectors() {
        if sector.slot.index.is_some() || b.list_t < 0.45 {
            draw_list_row(cr, b, sector, view, list)?;
        }
    }

    let box_chrome = smoothstep(0.60, 0.94, b.list_t);
    if box_chrome > 0.0 {
        let x = -b.width / 2.0 + 20.0;
        let top = -b.height / 2.0;

        if !settled && !view.high_contrast {
            draw_top_sheen(cr, b, p, box_chrome)?;
        }

        let title = if view.title.is_empty() {
            "Choose Option"
        } else {
            view.title
        };
        draw_cached_label(
            cr,
            title,
            16.0,
            true,
            x,
            top + 23.0,
            (b.width - 40.0).max(20.0),
            EllipsizeMode::End,
            p.text_primary,
            box_chrome,
        )?;

        if !view.query.is_empty() {
            let bar_x = -b.width / 2.0 + 18.0;
            let bar_y = top + 40.0;
            let bar_w = b.width - 36.0;
            let bar_h = 28.0;
            if box_chrome < 1.0 {
                cr.save()?;
                cr.rectangle(bar_x - 2.0, bar_y - 2.0, bar_w + 4.0, bar_h + 4.0);
                cr.clip();
                cr.push_group();
            }
            rounded_rect(cr, bar_x, bar_y, bar_w, bar_h, 8.0);
            let bar_bg = LinearGradient::new(0.0, bar_y, 0.0, bar_y + bar_h);
            bar_bg.add_color_stop_rgba(
                0.0,
                p.capsule_top.0,
                p.capsule_top.1,
                p.capsule_top.2,
                material.opacity(0.55, 0.72),
            );
            bar_bg.add_color_stop_rgba(
                1.0,
                p.capsule_bottom.0,
                p.capsule_bottom.1,
                p.capsule_bottom.2,
                material.opacity(0.65, 0.80),
            );
            cr.set_source(&bar_bg)?;
            cr.fill_preserve()?;
            cr.set_line_width(if view.high_contrast { 1.2 } else { 0.8 });
            color(
                cr,
                p.capsule_border,
                if view.high_contrast { 0.85 } else { 0.28 },
            );
            cr.stroke()?;

            let path = text_layout(cr, view.query, 13.0, false);
            add_suggestion(&path, view.query, view.suggestion, p);
            path.set_ellipsize(if view.suggestion.is_empty() {
                EllipsizeMode::Start
            } else {
                EllipsizeMode::Middle
            });
            label(
                cr,
                &path,
                bar_x + 11.0,
                bar_y + bar_h / 2.0,
                bar_w - 22.0,
                p.text_capsule,
                1.0,
            );
            if box_chrome < 1.0 {
                cr.pop_group_to_source()?;
                cr.paint_with_alpha(box_chrome)?;
                cr.restore()?;
            }
        }

        if !settled {
            draw_soft_divider(
                cr,
                b.width,
                b.grid_top() - 4.0,
                p.divider,
                (if view.high_contrast { 0.45 } else { 0.14 }) * box_chrome,
            )?;
        }

        if let Some((thumb_top, height)) = b.list_thumb(list.first, list.total) {
            let track_x = b.width / 2.0 - 10.0;
            let track_h = (b.list_body_height() - 8.0).max(24.0);
            rounded_rect(cr, track_x, b.grid_top() + 4.0, 4.0, track_h, 2.0);
            color(
                cr,
                p.divider,
                (if view.high_contrast { 0.30 } else { 0.09 }) * box_chrome,
            );
            cr.fill()?;

            rounded_rect(cr, track_x, thumb_top, 4.0, height, 2.0);
            color(
                cr,
                p.rim_top,
                (if view.high_contrast {
                    0.95
                } else if list.dragging_thumb {
                    0.85
                } else {
                    0.50
                }) * box_chrome,
            );
            cr.fill()?;
        }
    }

    if !settled {
        let prev_tol = cr.tolerance();
        cr.set_tolerance(0.35);
        draw_outer_rim(cr, b, view.high_contrast, p)?;
        if b.hole > 2.0 {
            let hole_alpha = smoothstep(2.0, INNER * 0.65, b.hole);
            let hole_rim = LinearGradient::new(0.0, -b.hole, 0.0, b.hole);
            for (at, (r, g, blue), alpha) in [(0.0, p.rim_top, 0.70), (1.0, p.rim_bottom, 0.27)] {
                hole_rim.add_color_stop_rgba(
                    at,
                    r,
                    g,
                    blue,
                    hole_alpha * if view.high_contrast { 1.0 } else { alpha },
                );
            }
            cr.new_path();
            cr.arc(0.0, 0.0, b.hole, 0.0, TAU);
            cr.close_path();
            cr.set_source(&hole_rim)?;
            cr.set_line_width(if view.high_contrast { 1.8 } else { 1.0 });
            cr.stroke()?;
        }
        cr.set_tolerance(prev_tol);
    }

    let mask = if material == Material::Frosted {
        b.blur_region()?
    } else {
        Region::create()
    };
    if ring_chrome > 0.0
        && (!view.title.is_empty() || !view.query.is_empty() || !view.status.is_empty())
    {
        let capsule_blur = (ring_chrome > 0.5).then_some(&mask);
        draw_captions(cr, b.cx, b.cy, b.scale, view, capsule_blur, ring_chrome)?;
    }
    cr.restore()?;
    Ok(mask)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorPickerState {
    pub h: f64,
    pub s: f64,
    pub v: f64,
}

impl ColorPickerState {
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        let (h, s, v) = rgb_to_hsv(r, g, b);
        Self { h, s, v }
    }

    pub fn to_rgb(self) -> (u8, u8, u8) {
        hsv_to_rgb(self.h, self.s, self.v)
    }

    pub fn hue_rgb(self) -> Rgb {
        hsv_to_rgb_f64(self.h, 1.0, 1.0)
    }
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let rf = f64::from(r) / 255.0;
    let gf = f64::from(g) / 255.0;
    let bf = f64::from(b) / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;
    let h = if delta <= 1e-9 {
        0.0
    } else if (max - rf).abs() <= 1e-9 {
        60.0 * (((gf - bf) / delta).rem_euclid(6.0))
    } else if (max - gf).abs() <= 1e-9 {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    let s = if max <= 1e-9 { 0.0 } else { delta / max };
    (h.rem_euclid(360.0), s, max)
}

pub fn hsv_to_rgb_f64(h: f64, s: f64, v: f64) -> Rgb {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match h {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r1 + m, g1 + m, b1 + m)
}

pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let (r, g, b) = hsv_to_rgb_f64(h, s, v);
    (
        (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (b.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

#[derive(Clone)]
pub struct InputState {
    pub title: String,
    pub text: String,
    pub password: bool,
    pub cursor: usize,
    pub selection: Option<(usize, usize)>,
    pub selected_button: Option<usize>,
    pub pressed_button: Option<usize>,
    pub shortcuts: bool,
    pub accent: Rgb,
    pub color: Option<ColorPickerState>,
}

fn char_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map_or(s.len(), |(byte_idx, _)| byte_idx)
}

#[allow(clippy::too_many_arguments)]
fn draw_input_button(
    cr: &Context,
    rect: (f64, f64, f64, f64),
    btn_label: &str,
    badge: Option<&str>,
    primary: bool,
    focus: f64,
    selected: bool,
    pressed: bool,
    accent: Rgb,
    high_contrast: bool,
    p: &Palette,
    alpha: f64,
) -> Result<(), cairo::Error> {
    let (bx, by, bw, bh) = rect;
    let cx = bx + bw * 0.5;
    let cy = by + bh * 0.5;
    let zoom = if pressed {
        0.97
    } else {
        1.0 + 0.018 * focus
    };

    cr.save()?;
    cr.translate(cx, cy);
    if (zoom - 1.0).abs() >= 1e-4 {
        cr.scale(zoom, zoom);
    }

    rounded_rect(cr, -bw * 0.5, -bh * 0.5, bw, bh, 9.0);
    if primary {
        let bg = LinearGradient::new(0.0, -bh * 0.5, 0.0, bh * 0.5);
        let top_a = if pressed {
            0.38
        } else if selected {
            0.28 + 0.08 * focus
        } else {
            0.18 + 0.12 * focus
        } * alpha;
        let bot_a = if pressed {
            0.26
        } else if selected {
            0.18 + 0.06 * focus
        } else {
            0.10 + 0.08 * focus
        } * alpha;
        bg.add_color_stop_rgba(0.0, accent.0, accent.1, accent.2, top_a);
        bg.add_color_stop_rgba(1.0, accent.0, accent.1, accent.2, bot_a);
        cr.set_source(&bg)?;
        cr.fill_preserve()?;
        cr.set_line_width(if high_contrast { 1.6 } else { 1.0 });
        color(
            cr,
            accent,
            if high_contrast {
                alpha
            } else if pressed || selected {
                0.82 * alpha
            } else {
                (0.48 + 0.28 * focus) * alpha
            },
        );
        cr.stroke()?;
    } else {
        let bg = LinearGradient::new(0.0, -bh * 0.5, 0.0, bh * 0.5);
        let boost = if pressed {
            0.16
        } else if selected {
            0.10
        } else {
            0.06 * focus
        };
        bg.add_color_stop_rgba(
            0.0,
            p.capsule_top.0,
            p.capsule_top.1,
            p.capsule_top.2,
            (0.62 + boost) * alpha,
        );
        bg.add_color_stop_rgba(
            1.0,
            p.capsule_bottom.0,
            p.capsule_bottom.1,
            p.capsule_bottom.2,
            (0.74 + boost) * alpha,
        );
        cr.set_source(&bg)?;
        cr.fill_preserve()?;
        cr.set_line_width(if high_contrast { 1.5 } else { 0.95 });
        let border_tint = lerp_rgb(p.capsule_border, p.accent_windows, 0.35 * focus);
        color(
            cr,
            border_tint,
            if high_contrast {
                0.95 * alpha
            } else if pressed || selected {
                0.68 * alpha
            } else {
                (0.32 + 0.26 * focus) * alpha
            },
        );
        cr.stroke()?;
    }

    draw_cached_centered_text(
        cr,
        btn_label,
        13.0,
        true,
        (0.0, 0.0),
        bw - 36.0,
        p.text_primary,
        p.text_shadow,
        alpha,
    )?;

    if let Some(badge_str) = badge {
        let badge_w = 18.0;
        let badge_h = 16.0;
        let badge_x = bw * 0.5 - badge_w - 8.0;
        let badge_y = -badge_h * 0.5;
        rounded_rect(cr, badge_x, badge_y, badge_w, badge_h, 4.5);
        let pill_tint = if primary { accent } else { p.capsule_border };
        color(
            cr,
            pill_tint,
            (if selected || pressed { 0.28 } else { 0.16 }) * alpha,
        );
        cr.fill_preserve()?;
        cr.set_line_width(0.75);
        color(
            cr,
            pill_tint,
            (if selected || pressed { 0.62 } else { 0.36 }) * alpha,
        );
        cr.stroke()?;

        draw_cached_centered_text(
            cr,
            badge_str,
            10.0,
            true,
            (badge_x + badge_w * 0.5, 0.0),
            badge_w,
            if primary { p.rim_top } else { p.text_primary },
            p.text_shadow,
            (if selected || pressed { 0.96 } else { 0.84 }) * alpha,
        )?;
    }

    cr.restore()
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct ColorSvKey {
    hue_tenths: u32,
    w: u64,
    h: u64,
    sx: u64,
    sy: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct ColorHueKey {
    w: u64,
    h: u64,
    border: [u64; 3],
    sx: u64,
    sy: u64,
}

fn draw_color_sv_cached(
    cr: &Context,
    hue: f64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    alpha: f64,
) -> Result<(), cairo::Error> {
    let (sx, sy) = surface_device_scale(cr);
    // Quantize hue to 0.1° (3600 steps) so morph frames and SV drags always hit cache.
    let hue_tenths = (hue.rem_euclid(360.0) * 10.0).round() as u32 % 3600;
    let key = ColorSvKey {
        hue_tenths,
        w: w.to_bits(),
        h: h.to_bits(),
        sx: sx.to_bits(),
        sy: sy.to_bits(),
    };
    thread_local! {
        static CACHE: RefCell<Option<(ColorSvKey, ImageSurface)>> = const { RefCell::new(None) };
    }
    let surf = CACHE.with(|cache| -> Result<ImageSurface, cairo::Error> {
        let mut slot = cache.borrow_mut();
        if let Some((cached_key, surf)) = slot.as_ref()
            && *cached_key == key
        {
            return Ok(surf.clone());
        }
        let surf = ImageSurface::create(
            Format::ARgb32,
            (w * sx).ceil().max(1.0) as i32,
            (h * sy).ceil().max(1.0) as i32,
        )?;
        surf.set_device_scale(sx, sy);
        {
            let ctx = Context::new(&surf)?;
            rounded_rect(&ctx, 0.0, 0.0, w, h, 10.0);
            ctx.clip();
            let (hr, hg, hb) = hsv_to_rgb_f64(f64::from(hue_tenths) * 0.1, 1.0, 1.0);
            ctx.set_source_rgb(hr, hg, hb);
            ctx.rectangle(0.0, 0.0, w, h);
            ctx.fill()?;

            let sat_grad = LinearGradient::new(0.0, 0.0, w, 0.0);
            sat_grad.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 1.0);
            sat_grad.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.0);
            ctx.set_source(&sat_grad)?;
            ctx.rectangle(0.0, 0.0, w, h);
            ctx.fill()?;

            let val_grad = LinearGradient::new(0.0, 0.0, 0.0, h);
            val_grad.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
            val_grad.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 1.0);
            ctx.set_source(&val_grad)?;
            ctx.rectangle(0.0, 0.0, w, h);
            ctx.fill()?;
        }
        *slot = Some((key, surf.clone()));
        Ok(surf)
    })?;
    paint_surface(cr, &surf, x, y, alpha)
}

fn draw_color_hue_cached(
    cr: &Context,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    border: Rgb,
    alpha: f64,
) -> Result<(), cairo::Error> {
    let (sx, sy) = surface_device_scale(cr);
    let key = ColorHueKey {
        w: w.to_bits(),
        h: h.to_bits(),
        border: rgb_bits(border),
        sx: sx.to_bits(),
        sy: sy.to_bits(),
    };
    thread_local! {
        static CACHE: RefCell<Option<(ColorHueKey, ImageSurface)>> = const { RefCell::new(None) };
    }
    const PAD: f64 = 2.0;
    let surf = CACHE.with(|cache| -> Result<ImageSurface, cairo::Error> {
        let mut slot = cache.borrow_mut();
        if let Some((cached_key, surf)) = slot.as_ref()
            && *cached_key == key
        {
            return Ok(surf.clone());
        }
        let surf = ImageSurface::create(
            Format::ARgb32,
            ((w + PAD * 2.0) * sx).ceil().max(1.0) as i32,
            ((h + PAD * 2.0) * sy).ceil().max(1.0) as i32,
        )?;
        surf.set_device_scale(sx, sy);
        {
            let ctx = Context::new(&surf)?;
            rounded_rect(&ctx, PAD, PAD, w, h, h * 0.5);
            let hue_grad = LinearGradient::new(PAD, PAD + h * 0.5, PAD + w, PAD + h * 0.5);
            hue_grad.add_color_stop_rgb(0.0, 1.0, 0.0, 0.0);
            hue_grad.add_color_stop_rgb(1.0 / 6.0, 1.0, 1.0, 0.0);
            hue_grad.add_color_stop_rgb(2.0 / 6.0, 0.0, 1.0, 0.0);
            hue_grad.add_color_stop_rgb(3.0 / 6.0, 0.0, 1.0, 1.0);
            hue_grad.add_color_stop_rgb(4.0 / 6.0, 0.0, 0.0, 1.0);
            hue_grad.add_color_stop_rgb(5.0 / 6.0, 1.0, 0.0, 1.0);
            hue_grad.add_color_stop_rgb(1.0, 1.0, 0.0, 0.0);
            ctx.set_source(&hue_grad)?;
            ctx.fill_preserve()?;
            ctx.set_line_width(0.9);
            color(&ctx, border, 0.38);
            ctx.stroke()?;
        }
        *slot = Some((key, surf.clone()));
        Ok(surf)
    })?;
    paint_surface(cr, &surf, x - PAD, y - PAD, alpha)
}

pub fn draw_input(
    cr: &Context,
    b: Bounds,
    view: &View<'_>,
    input: &InputState,
) -> Result<Region, cairo::Error> {
    if b.scale <= 0.0 {
        return Ok(Region::create());
    }
    let p = view.palette;
    let material = Material::for_view(view);
    let settled = b.t >= 0.9999 && b.hole == 0.0 && b.input_t >= 0.9999;
    cr.save()?;
    cr.translate(b.cx, b.cy);
    cr.scale(b.scale, b.scale);

    if settled {
        draw_settled_box_shell_cached(cr, b, material, view.high_contrast, p)?;
    } else {
        draw_box_shadow(cr, b, p, true)?;
        let prev_tol = cr.tolerance();
        cr.set_tolerance(0.35);
        draw_box_glass(cr, b, material, p)?;
        cr.set_tolerance(prev_tol);
    }

    let ring_chrome = (1.0 - b.t / 0.28).clamp(0.0, 1.0);
    if ring_chrome > 0.0 {
        cr.save()?;
        b.path(cr);
        cr.clip();
        for sector_view in view.motion.sectors() {
            if !sector_view.slot.born_in_box() {
                draw_sector_chrome(cr, &sector_view, view, p, ring_chrome)?;
            }
        }
        cr.restore()?;
    }

    // Smoothly glide and fade out previous items while transforming into the input rectangle.
    let item_fade = (1.0 - smoothstep(0.04, 0.44, b.input_t)).clamp(0.0, 1.0);
    if item_fade > 0.008 {
        cr.save()?;
        b.path(cr);
        cr.clip();
        let pull = lerp(1.0, 0.72, b.input_t);
        let grid_total = b.box_t + b.list_t;
        let has_scrollbar = view.count > LIST_ROWS;
        for sector_view in view.motion.sectors() {
            let slot = sector_view.slot;
            if slot.born_in_box() && grid_total <= 0.01 {
                continue;
            }
            let angle = slot
                .ring_angle()
                .unwrap_or((sector_view.start + sector_view.end) * 0.5);
            let ring_x = ITEM_RADIUS * angle.cos();
            let ring_y = ITEM_RADIUS * angle.sin();
            let index = slot.grid_index();
            let gx = LEFT + (index % GRID_COLUMNS) as f64 * CELL_W + CELL_W / 2.0 - WIDTH / 2.0;
            let gy = b.header + ((index / GRID_COLUMNS) as f64 + 0.5) * CELL_H
                - (b.header + GRID_H + FOOTER) * 0.5;
            let right_pad = if has_scrollbar { 20.0 } else { 14.0 };
            let rx = (14.0 - right_pad) * 0.5;
            let ry = b.header + (index as f64 + 0.5) * LIST_ROW_H
                - (b.header + b.list_body_height() + LIST_FOOTER) * 0.5;
            let base_x = if slot.born_in_box() {
                if b.list_t > b.box_t { rx } else { gx }
            } else {
                ring_x * (1.0 - grid_total).max(0.0) + gx * b.box_t + rx * b.list_t
            };
            let base_y = if slot.born_in_box() {
                if b.list_t > b.box_t { ry } else { gy }
            } else {
                ring_y * (1.0 - grid_total).max(0.0) + gy * b.box_t + ry * b.list_t
            };
            let x = base_x * pull;
            let y = base_y * pull;
            let list_like = b.list_t > 0.45 && b.list_t >= b.box_t;
            let row_w = LIST_W - 14.0 - right_pad;
            for face in &slot.faces {
                let alpha = face.opacity * item_fade;
                if alpha <= 0.008 {
                    continue;
                }
                cr.save()?;
                cr.translate(x, y);
                cr.scale(pull, pull);
                if list_like {
                    draw_cached_label(
                        cr,
                        &face.item.label,
                        13.5,
                        false,
                        -row_w / 2.0 + 14.0,
                        0.0,
                        (row_w - 28.0).max(24.0),
                        EllipsizeMode::End,
                        p.text_primary,
                        alpha,
                    )?;
                } else {
                    let plate = lerp(32.0, 48.0, b.box_t);
                    let icon_size = lerp(20.0, 26.0, b.box_t);
                    let icon_y = lerp(-9.0, -15.0, b.box_t);
                    let label_y = lerp(16.5, 19.5, b.box_t);
                    let label_w = lerp(84.0, CELL_W - 20.0, b.box_t);
                    draw_icon_plate(cr, &face.item, icon_y, plate, icon_size, p, alpha)?;
                    draw_cached_centered_text(
                        cr,
                        &face.item.label,
                        12.0,
                        true,
                        (0.0, label_y),
                        label_w,
                        p.text_primary,
                        p.text_shadow,
                        alpha,
                    )?;
                }
                cr.restore()?;
            }
        }
        cr.restore()?;
    }

    let chrome = smoothstep(0.28, 0.92, b.input_t);
    if chrome > 0.005 {
        let top = -b.height / 2.0;
        let x = -b.width / 2.0 + 20.0;

        if !settled && !view.high_contrast {
            draw_top_sheen(cr, b, p, chrome)?;
        }

        let title = if input.title.is_empty() {
            view.title
        } else {
            &input.title
        };
        let title_w = if input.color.is_some() {
            (b.width - 112.0).max(40.0)
        } else {
            (b.width - 40.0).max(40.0)
        };
        draw_cached_label(
            cr,
            title,
            15.5,
            true,
            x,
            top + 23.0,
            title_w,
            EllipsizeMode::End,
            p.text_primary,
            chrome,
        )?;

        if let Some(cp) = input.color {
            let (rf, gf, bf) = hsv_to_rgb_f64(cp.h, cp.s, cp.v);
            let sw = 54.0;
            let sh = 22.0;
            let sx = b.width * 0.5 - sw - 20.0;
            let sy = top + 12.0;
            rounded_rect(cr, sx, sy, sw, sh, 7.0);
            cr.set_source_rgba(rf, gf, bf, chrome);
            cr.fill_preserve()?;
            cr.set_line_width(1.2);
            color(cr, p.text_primary, 0.55 * chrome);
            cr.stroke()?;
        }

        if !settled {
            draw_soft_divider(
                cr,
                b.width,
                b.grid_top() - 4.0,
                p.divider,
                (if view.high_contrast { 0.45 } else { 0.14 }) * chrome,
            )?;
        }

        if let Some(cp) = input.color {
            let (rf, gf, bf) = hsv_to_rgb_f64(cp.h, cp.s, cp.v);
            let (hr, hg, hb) = cp.hue_rgb();

            // 2D Saturation / Value color square (cached per quantized hue & dimensions).
            let (sx, sy, sw, sh) = b.color_sv_rect();
            draw_color_sv_cached(cr, cp.h, sx, sy, sw, sh, chrome)?;

            rounded_rect(cr, sx, sy, sw, sh, 10.0);
            cr.set_line_width(if view.high_contrast { 1.4 } else { 0.9 });
            color(
                cr,
                p.capsule_border,
                (if view.high_contrast { 0.9 } else { 0.36 }) * chrome,
            );
            cr.stroke()?;

            // SV reticle thumb.
            let sv_x = sx + cp.s.clamp(0.0, 1.0) * sw;
            let sv_y = sy + (1.0 - cp.v.clamp(0.0, 1.0)) * sh;
            cr.new_path();
            cr.arc(sv_x, sv_y + 1.0, 9.5, 0.0, TAU);
            color(cr, p.shadow, 0.48 * chrome);
            cr.fill()?;

            cr.new_path();
            cr.arc(sv_x, sv_y, 8.0, 0.0, TAU);
            cr.set_source_rgba(rf, gf, bf, chrome);
            cr.fill_preserve()?;
            cr.set_line_width(2.4);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.96 * chrome);
            cr.stroke()?;

            cr.new_path();
            cr.arc(sv_x, sv_y, 9.2, 0.0, TAU);
            cr.set_line_width(0.8);
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35 * chrome);
            cr.stroke()?;

            // Rainbow Hue bar (cached per dimensions & border color).
            let (hx, hy, hw, hh) = b.color_hue_rect();
            let hue_mid_y = hy + hh * 0.5;
            draw_color_hue_cached(cr, hx, hy, hw, hh, p.capsule_border, chrome)?;

            // Hue thumb.
            let hue_x = hx + (cp.h.rem_euclid(360.0) / 360.0) * hw;
            cr.new_path();
            cr.arc(hue_x, hue_mid_y + 1.0, 10.0, 0.0, TAU);
            color(cr, p.shadow, 0.48 * chrome);
            cr.fill()?;

            cr.new_path();
            cr.arc(hue_x, hue_mid_y, 8.4, 0.0, TAU);
            cr.set_source_rgba(hr, hg, hb, chrome);
            cr.fill_preserve()?;
            cr.set_line_width(2.4);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.96 * chrome);
            cr.stroke()?;

            cr.new_path();
            cr.arc(hue_x, hue_mid_y, 9.6, 0.0, TAU);
            cr.set_line_width(0.8);
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35 * chrome);
            cr.stroke()?;
        }

        // Recessed text entry capsule with accent focus ring.
        let (fx, fy, fw, fh) = if input.color.is_some() {
            b.color_hex_field_rect()
        } else {
            b.input_field_rect()
        };
        rounded_rect(cr, fx - 1.2, fy - 1.2, fw + 2.4, fh + 2.4, 11.2);
        color(cr, input.accent, 0.20 * chrome);
        cr.set_line_width(2.2);
        cr.stroke()?;

        rounded_rect(cr, fx, fy, fw, fh, 10.0);
        let field_bg = LinearGradient::new(0.0, fy, 0.0, fy + fh);
        field_bg.add_color_stop_rgba(
            0.0,
            p.capsule_bottom.0,
            p.capsule_bottom.1,
            p.capsule_bottom.2,
            material.opacity(0.72, 0.86) * chrome,
        );
        field_bg.add_color_stop_rgba(
            1.0,
            p.capsule_top.0,
            p.capsule_top.1,
            p.capsule_top.2,
            material.opacity(0.64, 0.78) * chrome,
        );
        cr.set_source(&field_bg)?;
        cr.fill_preserve()?;
        cr.set_line_width(if view.high_contrast { 1.6 } else { 1.1 });
        let field_border = lerp_rgb(p.capsule_border, input.accent, 0.58);
        color(
            cr,
            field_border,
            (if view.high_contrast { 1.0 } else { 0.76 }) * chrome,
        );
        cr.stroke()?;

        // Text content, selection highlight, and cursor inside the input field.
        let pad_x = 12.0;
        let avail_w = (fw - pad_x * 2.0).max(16.0);
        let display_text = if input.password {
            "•".repeat(input.text.chars().count())
        } else {
            input.text.clone()
        };
        let text_lay = text_layout(cr, &display_text, 14.0, false);
        let cursor_byte = char_to_byte_index(&display_text, input.cursor);
        let cursor_pos = text_lay.index_to_pos(cursor_byte as i32);
        let cursor_x = f64::from(cursor_pos.x()) / f64::from(SCALE);
        let scroll_x = (cursor_x - (avail_w - 4.0)).max(0.0);

        cr.save()?;
        cr.rectangle(fx + pad_x - 2.0, fy + 2.0, avail_w + 4.0, fh - 4.0);
        cr.clip();

        if let Some((sel_start, sel_end)) = input.selection
            && sel_start != sel_end
        {
            let (c0, c1) = if sel_start < sel_end {
                (sel_start, sel_end)
            } else {
                (sel_end, sel_start)
            };
            let b0 = char_to_byte_index(&display_text, c0);
            let b1 = char_to_byte_index(&display_text, c1);
            let x0 = f64::from(text_lay.index_to_pos(b0 as i32).x()) / f64::from(SCALE);
            let x1 = f64::from(text_lay.index_to_pos(b1 as i32).x()) / f64::from(SCALE);
            if x1 > x0 {
                rounded_rect(
                    cr,
                    fx + pad_x - scroll_x + x0,
                    fy + 6.0,
                    x1 - x0,
                    fh - 12.0,
                    4.0,
                );
                color(cr, input.accent, 0.34 * chrome);
                cr.fill()?;
            }
        }

        if !display_text.is_empty() {
            let (_, th) = text_lay.pixel_size();
            color(cr, p.text_capsule, chrome);
            cr.move_to(fx + pad_x - scroll_x, fy + (fh - f64::from(th)) * 0.5);
            pangocairo::functions::show_layout(cr, &text_lay);
            cr.new_path();
        }

        // Caret bar.
        let caret_x = fx + pad_x - scroll_x + cursor_x;
        let caret_h = fh - 14.0;
        rounded_rect(cr, caret_x, fy + (fh - caret_h) * 0.5, 1.8, caret_h, 0.9);
        color(cr, input.accent, 0.95 * chrome);
        cr.fill()?;
        cr.restore()?;

        if !settled {
            draw_soft_divider(
                cr,
                b.width,
                b.height / 2.0 - b.footer + 2.0,
                p.divider,
                (if view.high_contrast { 0.40 } else { 0.11 }) * chrome,
            )?;
        }

        let [done_rect, cancel_rect] = b.input_button_rects();
        draw_input_button(
            cr,
            done_rect,
            "Done",
            input.shortcuts.then_some("1"),
            true,
            view.motion.input_button_focus(0),
            input.selected_button == Some(0),
            input.pressed_button == Some(0),
            input.accent,
            view.high_contrast,
            p,
            chrome,
        )?;
        draw_input_button(
            cr,
            cancel_rect,
            "Cancel",
            input.shortcuts.then_some("2"),
            false,
            view.motion.input_button_focus(1),
            input.selected_button == Some(1),
            input.pressed_button == Some(1),
            input.accent,
            view.high_contrast,
            p,
            chrome,
        )?;
    }

    if !settled {
        let prev_tol = cr.tolerance();
        cr.set_tolerance(0.35);
        draw_outer_rim(cr, b, view.high_contrast, p)?;
        if b.hole > 2.0 {
            let hole_alpha = smoothstep(2.0, INNER * 0.65, b.hole);
            let hole_rim = LinearGradient::new(0.0, -b.hole, 0.0, b.hole);
            for (at, (r, g, blue), alpha) in [(0.0, p.rim_top, 0.70), (1.0, p.rim_bottom, 0.27)] {
                hole_rim.add_color_stop_rgba(
                    at,
                    r,
                    g,
                    blue,
                    hole_alpha * if view.high_contrast { 1.0 } else { alpha },
                );
            }
            cr.new_path();
            cr.arc(0.0, 0.0, b.hole, 0.0, TAU);
            cr.close_path();
            cr.set_source(&hole_rim)?;
            cr.set_line_width(if view.high_contrast { 1.8 } else { 1.0 });
            cr.stroke()?;
        }
        cr.set_tolerance(prev_tol);
    }

    let mask = if material == Material::Frosted {
        b.blur_region()?
    } else {
        Region::create()
    };
    if ring_chrome > 0.0
        && (!view.title.is_empty() || !view.query.is_empty() || !view.status.is_empty())
    {
        let capsule_blur = (ring_chrome > 0.5).then_some(&mask);
        draw_captions(cr, b.cx, b.cy, b.scale, view, capsule_blur, ring_chrome)?;
    }
    cr.restore()?;
    Ok(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(count: usize) -> Vec<Item> {
        (0..count)
            .map(|i| Item {
                label: format!("Folder {i}"),
                icon: Icon::Folder,
                active: false,
                pixbuf: None,
            })
            .collect()
    }

    #[test]
    fn resizing_ring_uses_one_outline_for_paint_blur_and_input() {
        let size = (510.0, 380.0);
        let anchor = (9.0, 12.0);
        let mut motion = Motion::default();
        motion.update(items(9), None, false);
        let bounds = |m: &Motion| Bounds::new(size, anchor, true, m);
        let initial = bounds(&motion);
        motion.set_box(true);
        motion.advance(0.05);
        let mid = bounds(&motion);
        assert!(mid.width > initial.width && mid.width < WIDTH);
        assert!(mid.radius < initial.radius && mid.hole > 0.0);
        let mask = mid.blur_region().unwrap();
        for (x, y) in [
            (mid.cx, mid.cy),
            (mid.cx + mid.width * mid.scale * 0.41, mid.cy),
            (0.0, 0.0),
        ] {
            let pt = (x.floor() + 0.5, y.floor() + 0.5);
            assert_eq!(
                mid.contains(pt),
                mask.contains_point(pt.0 as i32, pt.1 as i32)
            );
        }
        motion.set_box(false);
        assert_eq!(mid.width, bounds(&motion).width);
        motion.set_box(true);
        motion.advance(10.0);
        let grid = bounds(&motion);
        assert_eq!(grid.hole, 0.0);
        assert!(grid.cx - grid.width * grid.scale / 2.0 >= 0.0);
        assert!(grid.cy - grid.height * grid.scale / 2.0 >= 0.0);
        for i in 0..12 {
            let x = -WIDTH / 2.0 + LEFT + (i % GRID_COLUMNS) as f64 * CELL_W + CELL_W / 2.0;
            let y = grid.grid_top() + (i / GRID_COLUMNS) as f64 * CELL_H + CELL_H / 2.0;
            let pos = (grid.cx + x * grid.scale, grid.cy + y * grid.scale);
            assert_eq!(grid.hit(pos, &motion), (i < 9).then_some(i));
        }
        assert!(grid.thumb(0, 12).is_none());
        let (_, height) = grid.thumb(0, 19).unwrap();
        let track = |y| {
            (
                grid.cx + (WIDTH / 2.0 - 7.0) * grid.scale,
                grid.cy + y * grid.scale,
            )
        };
        let grab = grid
            .grab_thumb(track(grid.grid_top() + 8.0), 0, 19)
            .unwrap();
        assert_eq!(
            grid.dragged_first(
                track(grid.grid_top() + GRID_H - 4.0 - height + grab),
                grab,
                19,
            ),
            8
        );
    }

    #[test]
    fn sweeping_focus_leaves_only_the_current_tile_highlighted() {
        let mut motion = Motion::default();
        motion.set_box(true);
        motion.update(items(12), None, false);
        let palette = Palette::default();
        let bounds = Bounds::new((800.0, 600.0), (400.0, 300.0), false, &motion);
        let sample = |motion: &Motion, selected: Option<usize>| {
            let mut image = cairo::ImageSurface::create(cairo::Format::ARgb32, 800, 600).unwrap();
            let view = View {
                motion,
                palette: &palette,
                count: 12,
                pressed: None,
                title: "Applications",
                query: "",
                suggestion: "",
                status: "",
                page: None,
                high_contrast: true,
                solid: true,
                blur_available: false,
                loading_accent: None,
            };
            draw(
                &Context::new(&image).unwrap(),
                bounds,
                &view,
                &GridState {
                    first: 0,
                    total: 12,
                    selected,
                    shortcuts: true,
                    dragging_thumb: false,
                },
            )
            .unwrap();
            let stride = image.stride() as usize;
            let data = image.data().unwrap();
            motion
                .sectors()
                .map(|s| {
                    let tile = bounds.tile(&s);
                    let x = (bounds.cx + tile.x - tile.width / 2.0 + 5.0) as usize;
                    let y = (bounds.cy + tile.y) as usize;
                    let at = y * stride + x * 4;
                    u32::from_ne_bytes(data[at..at + 4].try_into().unwrap())
                })
                .collect::<Vec<_>>()
        };
        let idle = sample(&motion, None);
        for i in 0..12 {
            motion.select(Some(i));
            motion.advance(0.009);
        }
        let active = sample(&motion, Some(11));
        assert_eq!(idle[..11], active[..11]);
        assert_ne!(idle[11], active[11]);
    }

    #[test]
    fn closing_box_spring_never_overshoots_or_rebounds() {
        let mut motion = Motion::default();
        motion.set_box(true);
        motion.update(items(12), None, false);
        assert_eq!(motion.box_mix(), 1.0);
        motion.set_box(false);
        let mut prev = motion.box_mix();
        for _ in 0..120 {
            motion.advance(1.0 / 120.0);
            let cur = motion.box_mix();
            assert!(cur <= prev + 1e-12, "closing box rebounded from {prev} to {cur}");
            assert!(cur >= 0.0, "closing box overshot below 0: {cur}");
            prev = cur;
        }
        assert_eq!(motion.box_mix(), 0.0);
    }

    #[test]
    fn explorer_damage_rect_contains_every_painted_pixel_across_morph() {
        let palette = Palette::default();
        let mut motion = Motion::default();
        motion.update(items(6), Some(0), false);
        motion.set_box(true);
        motion.update(items(12), Some(0), true);

        let zero_bounds = Bounds::new((0.0, 0.0), (0.0, 0.0), true, &motion);
        let empty = zero_bounds.damage_rect();
        assert_eq!((empty.width(), empty.height()), (0, 0));

        let grid = GridState {
            first: 0,
            total: 36,
            selected: Some(0),
            shortcuts: true,
            dragging_thumb: false,
        };
        for step in [0.0, 0.03, 0.06, 0.12, 3.0] {
            if step > 0.0 {
                motion.advance(step);
            }
            let bounds = Bounds::new((900.0, 900.0), (450.0, 450.0), true, &motion);
            let damage = bounds.damage_rect();
            let mut surface = ImageSurface::create(Format::ARgb32, 900, 900).unwrap();
            let cr = Context::new(&surface).unwrap();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: 12,
                pressed: Some(0),
                title: "Applications",
                query: "folder",
                suggestion: " 0",
                status: "36 items found",
                page: None,
                high_contrast: true,
                solid: false,
                blur_available: true,
                loading_accent: None,
            };
            draw(&cr, bounds, &view, &grid).unwrap();
            drop(cr);

            let stride = surface.stride() as usize;
            let data = surface.data().unwrap();
            let (mut min_x, mut min_y) = (i32::MAX, i32::MAX);
            let (mut max_x, mut max_y) = (i32::MIN, i32::MIN);
            for py in 0..900 {
                let row = py as usize * stride;
                for px in 0..900 {
                    if data[row + px as usize * 4 + 3] != 0 {
                        min_x = min_x.min(px);
                        min_y = min_y.min(py);
                        max_x = max_x.max(px + 1);
                        max_y = max_y.max(py + 1);
                    }
                }
            }
            assert!(
                min_x >= damage.x()
                    && min_y >= damage.y()
                    && max_x <= damage.x() + damage.width()
                    && max_y <= damage.y() + damage.height(),
                "morph t={:.2} painted ({min_x},{min_y})..({max_x},{max_y}) escaped damage ({},{})..({},{})",
                bounds.t,
                damage.x(),
                damage.y(),
                damage.x() + damage.width(),
                damage.y() + damage.height()
            );
        }
    }

    #[test]
    fn tile_clip_guard_keeps_center_hole_transparent_and_rejects_boundary_tiles() {
        let palette = Palette::default();
        let mut motion = Motion::default();
        motion.update(items(6), Some(0), false);
        motion.set_box(true);
        motion.update(items(12), Some(0), true);
        motion.advance(0.05);
        let mid = Bounds::new((900.0, 900.0), (450.0, 450.0), true, &motion);
        assert!(mid.hole > 10.0);

        // A tile overlapping the center hole or a rounded corner must NOT skip clipping.
        let hole_tile = Tile {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 90.0,
            emerge_alpha: 1.0,
            emerge_scale: 1.0,
        };
        assert!(!mid.tile_strictly_inside(&hole_tile, false));
        let corner_tile = Tile {
            x: mid.width * 0.5 - 10.0,
            y: -mid.height * 0.5 + 10.0,
            width: 120.0,
            height: 90.0,
            emerge_alpha: 1.0,
            emerge_scale: 1.0,
        };
        assert!(!mid.tile_strictly_inside(&corner_tile, true));

        // Render mid-morph with hole > 10px and assert the center hole stays completely clear.
        let mut surface = ImageSurface::create(Format::ARgb32, 900, 900).unwrap();
        let cr = Context::new(&surface).unwrap();
        let view = View {
            motion: &motion,
            palette: &palette,
            count: 12,
            pressed: Some(0),
            title: "Files",
            query: "test",
            suggestion: "",
            status: "",
            page: None,
            high_contrast: false,
            solid: true,
            blur_available: false,
            loading_accent: None,
        };
        let grid = GridState {
            first: 0,
            total: 12,
            selected: Some(0),
            shortcuts: true,
            dragging_thumb: false,
        };
        draw(&cr, mid, &view, &grid).unwrap();
        drop(cr);
        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        let safe_r = (mid.hole - 2.5) * mid.scale;
        for dy in -20..=20 {
            for dx in -20..=20 {
                if f64::from(dx).hypot(f64::from(dy)) < safe_r {
                    let px = (mid.cx as i32 + dx) as usize;
                    let py = (mid.cy as i32 + dy) as usize;
                    let alpha = data[py * stride + px * 4 + 3];
                    assert_eq!(
                        alpha, 0,
                        "tile leaked into center hole at offset ({dx},{dy}), alpha={alpha}"
                    );
                }
            }
        }

        // Once settled, an interior tile qualifies for the unclipped fast path.
        motion.advance(5.0);
        let settled = Bounds::new((900.0, 900.0), (450.0, 450.0), true, &motion);
        let interior = Tile {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 90.0,
            emerge_alpha: 1.0,
            emerge_scale: 1.0,
        };
        assert!(settled.tile_strictly_inside(&interior, true));
    }

    #[test]
    fn settled_box_cache_invalidates_on_query_material_and_palette_changes() {
        let mut motion = Motion::default();
        motion.set_box(true);
        motion.update(items(12), None, false);

        let sample = |has_query: bool, material: Material, high_contrast: bool, palette: &Palette| {
            let bounds = Bounds::new((900.0, 900.0), (450.0, 450.0), has_query, &motion);
            let mut surface = ImageSurface::create(Format::ARgb32, 900, 900).unwrap();
            {
                let cr = Context::new(&surface).unwrap();
                cr.translate(bounds.cx, bounds.cy);
                draw_settled_box_shell_cached(&cr, bounds, material, high_contrast, palette)
                    .unwrap();
            }
            let stride = surface.stride() as usize;
            let data = surface.data().unwrap();
            let px = |x: usize, y: usize| {
                let at = y * stride + x * 4;
                [data[at], data[at + 1], data[at + 2], data[at + 3]]
            };
            // Center pixel (material/color) and top-edge pixel at query-box height (size check).
            let query_height = HEADER_QUERY + GRID_H + FOOTER;
            let top_query_y = (bounds.cy - query_height * 0.5 + 3.0) as usize;
            (px(450, 450), px(450, top_query_y))
        };

        let base = Palette::default();
        let (center_frosted, edge_no_query) = sample(false, Material::Frosted, false, &base);
        let (center_solid, edge_with_query) = sample(true, Material::Solid, false, &base);
        assert!(center_solid[3] > center_frosted[3]);
        // With query=true the box is taller, so the pixel near its top edge is opaque; without query it's outside the box (shadow only).
        assert!(edge_with_query[3] > edge_no_query[3] + 100);

        let mut blue = base;
        blue.ring_top = (0.05, 0.10, 0.90);
        blue.ring_mid = (0.05, 0.10, 0.90);
        blue.ring_bottom = (0.05, 0.10, 0.90);
        let (center_blue, _) = sample(true, Material::Solid, false, &blue);
        // Cairo ARGB32 on little-endian stores [B, G, R, A].
        assert!(center_blue[0] > center_solid[0] + 50);
    }

    #[test]
    fn input_box_morphs_from_ring_hits_done_and_cancel_buttons_and_stays_within_damage() {
        let palette = Palette::default();
        let mut motion = Motion::default();
        motion.update(items(5), None, false);
        motion.set_input(true);

        let input = InputState {
            title: "Adapter Name".into(),
            text: "testhost".into(),
            password: false,
            cursor: 8,
            selection: Some((0, 4)),
            selected_button: Some(0),
            pressed_button: None,
            shortcuts: true,
            accent: palette.accent_action,
            color: None,
        };

        for step in [0.0, 0.04, 0.08, 3.0] {
            if step > 0.0 {
                motion.advance(step);
            }
            let bounds = Bounds::new((800.0, 600.0), (400.0, 300.0), false, &motion);
            let damage = bounds.damage_rect();
            let mut surface = ImageSurface::create(Format::ARgb32, 800, 600).unwrap();
            let cr = Context::new(&surface).unwrap();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: 5,
                pressed: None,
                title: "Bluetooth Settings",
                query: "",
                suggestion: "",
                status: "Adapter: testhost",
                page: None,
                high_contrast: false,
                solid: false,
                blur_available: true,
                loading_accent: None,
            };
            let region = draw_input(&cr, bounds, &view, &input).unwrap();
            assert!(!region.is_empty());
            drop(cr);

            let stride = surface.stride() as usize;
            let data = surface.data().unwrap();
            for py in 0..600 {
                let row = py as usize * stride;
                for px in 0..800 {
                    if data[row + px as usize * 4 + 3] != 0 {
                        assert!(
                            px >= damage.x()
                                && py >= damage.y()
                                && px < damage.x() + damage.width()
                                && py < damage.y() + damage.height(),
                            "input morph t={:.2} pixel ({px},{py}) escaped damage",
                            bounds.t
                        );
                    }
                }
            }
        }

        let settled = Bounds::new((800.0, 600.0), (400.0, 300.0), false, &motion);
        assert_eq!(settled.hole, 0.0);
        let [done, cancel] = settled.input_button_rects();
        let done_center = (
            settled.cx + (done.0 + done.2 * 0.5) * settled.scale,
            settled.cy + (done.1 + done.3 * 0.5) * settled.scale,
        );
        let cancel_center = (
            settled.cx + (cancel.0 + cancel.2 * 0.5) * settled.scale,
            settled.cy + (cancel.1 + cancel.3 * 0.5) * settled.scale,
        );
        assert_eq!(settled.hit_input_button(done_center), Some(0));
        assert_eq!(settled.hit_input_button(cancel_center), Some(1));
        assert_eq!(settled.hit_input_button((settled.cx, settled.cy)), None);
    }

    #[test]
    fn list_box_morphs_from_ring_hits_plain_text_rows_and_drags_scrollbar() {
        let palette = Palette::default();
        let mut motion = Motion::default();
        motion.update(items(4), None, false);
        motion.set_list(true, 6);
        let mut list_items = items(6);
        list_items[0].active = true;
        motion.update(list_items, Some(1), true);

        let list = ListState {
            first: 0,
            total: 10,
            selected: Some(1),
            dragging_thumb: false,
        };

        for step in [0.0, 0.04, 0.08, 3.0] {
            if step > 0.0 {
                motion.advance(step);
            }
            let bounds = Bounds::new((800.0, 600.0), (400.0, 300.0), false, &motion);
            let damage = bounds.damage_rect();
            let mut surface = ImageSurface::create(Format::ARgb32, 800, 600).unwrap();
            let cr = Context::new(&surface).unwrap();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: 6,
                pressed: None,
                title: "Choose Profile",
                query: "",
                suggestion: "",
                status: "Click to choose · Esc to go back",
                page: None,
                high_contrast: false,
                solid: false,
                blur_available: true,
                loading_accent: None,
            };
            let region = draw_list(&cr, bounds, &view, &list).unwrap();
            assert!(!region.is_empty());
            drop(cr);

            let stride = surface.stride() as usize;
            let data = surface.data().unwrap();
            for py in 0..600 {
                let row = py as usize * stride;
                for px in 0..800 {
                    if data[row + px as usize * 4 + 3] != 0 {
                        assert!(
                            px >= damage.x()
                                && py >= damage.y()
                                && px < damage.x() + damage.width()
                                && py < damage.y() + damage.height(),
                            "list morph t={:.2} pixel ({px},{py}) escaped damage",
                            bounds.t
                        );
                    }
                }
            }
        }

        let settled = Bounds::new((800.0, 600.0), (400.0, 300.0), false, &motion);
        assert_eq!(settled.hole, 0.0);
        for i in 0..6 {
            let y = settled.grid_top() + (i as f64 + 0.5) * LIST_ROW_H;
            let pos = (settled.cx, settled.cy + y * settled.scale);
            assert_eq!(settled.hit_list(pos, 10, &motion), Some(i));
        }

        let (thumb_top, thumb_h) = settled.list_thumb(0, 10).unwrap();
        let track_x = settled.cx + (settled.width / 2.0 - 7.0) * settled.scale;
        let grab = settled
            .grab_list_thumb((track_x, settled.cy + (thumb_top + 4.0) * settled.scale), 0, 10)
            .unwrap();
        let end_y = settled.grid_top() + settled.list_body_height() - 4.0 - thumb_h + grab;
        assert_eq!(
            settled.dragged_list_first((track_x, settled.cy + end_y * settled.scale), grab, 10),
            10 - LIST_ROWS
        );
    }

    #[test]
    fn grid_close_and_midflight_reversal_preserve_all_tile_positions_within_damage() {
        let palette = Palette::default();
        let mut motion = Motion::default();
        motion.update(items(5), None, false);
        motion.set_box(true);
        motion.update(items(12), None, false);
        assert_eq!(motion.box_mix(), 1.0);

        // Start closing from 12 grid items to 5 ring items.
        motion.set_box(false);
        motion.update(items(5), None, true);
        motion.advance(0.05);
        let mid_bounds = Bounds::new((900.0, 900.0), (450.0, 450.0), false, &motion);
        assert!((0.1..0.9).contains(&mid_bounds.box_t));

        let mut pos_before = [(0.0_f64, 0.0_f64); 12];
        for s in motion.sectors() {
            let t = mid_bounds.tile(&s);
            pos_before[s.slot.grid_index()] = (t.x, t.y);
        }

        // Reversing mid-flight back to 12 grid items must not jump any tile.
        motion.set_box(true);
        motion.update(items(12), None, true);
        let rev_bounds = Bounds::new((900.0, 900.0), (450.0, 450.0), false, &motion);
        for s in motion.sectors() {
            let t = rev_bounds.tile(&s);
            let (bx, by) = pos_before[s.slot.grid_index()];
            assert!(
                (t.x - bx).abs() < 1e-6 && (t.y - by).abs() < 1e-6,
                "tile {} jumped on mid-flight reversal: ({bx},{by}) -> ({},{})",
                s.slot.grid_index(),
                t.x,
                t.y
            );
        }

        // Settle back in grid, then verify closing morph stays inside damage_rect on every frame.
        motion.advance(3.0);
        motion.set_box(false);
        motion.update(items(5), None, true);
        let grid = GridState {
            first: 0,
            total: 12,
            selected: None,
            shortcuts: true,
            dragging_thumb: false,
        };
        for step in [0.0, 0.03, 0.06, 0.12, 0.25] {
            if step > 0.0 {
                motion.advance(step);
            }
            let bounds = Bounds::new((900.0, 900.0), (450.0, 450.0), false, &motion);
            let damage = bounds.damage_rect();
            let mut surface = ImageSurface::create(Format::ARgb32, 900, 900).unwrap();
            let cr = Context::new(&surface).unwrap();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: 5,
                pressed: None,
                title: "",
                query: "",
                suggestion: "",
                status: "",
                page: None,
                high_contrast: false,
                solid: true,
                blur_available: false,
                loading_accent: None,
            };
            draw(&cr, bounds, &view, &grid).unwrap();
            drop(cr);

            let stride = surface.stride() as usize;
            let data = surface.data().unwrap();
            for py in 0..900 {
                let row = py as usize * stride;
                for px in 0..900 {
                    if data[row + px as usize * 4 + 3] != 0 {
                        assert!(
                            px >= damage.x()
                                && py >= damage.y()
                                && px < damage.x() + damage.width()
                                && py < damage.y() + damage.height(),
                            "closing morph t={:.2} pixel ({px},{py}) escaped damage",
                            bounds.t
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn rgb_color_picker_renders_hits_sliders_and_stays_within_damage() {
        let palette = Palette::default();
        let mut motion = Motion::default();
        motion.set_input(true);
        motion.advance(10.0);

        let bounds = Bounds::new_with_input_height(
            (800.0, 600.0),
            (400.0, 300.0),
            false,
            &motion,
            COLOR_INPUT_H,
        );
        let input = InputState {
            title: "Ring Top".into(),
            text: "#422614".into(),
            password: false,
            cursor: 7,
            selection: None,
            selected_button: None,
            pressed_button: None,
            shortcuts: true,
            accent: palette.accent_action,
            color: Some(ColorPickerState::from_rgb(0x42, 0x26, 0x14)),
        };
        let image = ImageSurface::create(Format::ARgb32, 800, 600).unwrap();
        let cr = Context::new(&image).unwrap();
        let view = View {
            motion: &motion,
            palette: &palette,
            count: 0,
            pressed: None,
            title: "Ring Top",
            query: "",
            suggestion: "",
            status: "",
            page: None,
            high_contrast: false,
            solid: false,
            blur_available: true,
            loading_accent: None,
        };
        let region = draw_input(&cr, bounds, &view, &input).unwrap();
        assert!(!region.is_empty());

        let (sx, sy, sw, sh) = bounds.color_sv_rect();
        let sv_center = (
            bounds.cx + (sx + sw * 0.5) * bounds.scale,
            bounds.cy + (sy + sh * 0.5) * bounds.scale,
        );
        let (s, v) = bounds.hit_color_sv(sv_center).unwrap();
        assert!((s - 0.5).abs() < 0.02);
        assert!((v - 0.5).abs() < 0.02);

        let (hx, hy, hw, hh) = bounds.color_hue_rect();
        let hue_mid = (
            bounds.cx + (hx + hw * 0.5) * bounds.scale,
            bounds.cy + (hy + hh * 0.5) * bounds.scale,
        );
        let h = bounds.hit_color_hue(hue_mid).unwrap();
        assert!((h - 180.0).abs() < 2.0);

        // RGB -> HSV -> RGB round-trip
        for (r, g, b) in [(255, 0, 0), (0, 255, 128), (0x42, 0x26, 0x14), (255, 255, 255), (0, 0, 0)] {
            let cp = ColorPickerState::from_rgb(r, g, b);
            assert_eq!(cp.to_rgb(), (r, g, b));
        }
    }

    #[test]
    fn tile_strictly_inside_guards_hover_zoom_wrapped_labels_and_row_dividers() {
        let mut motion = Motion::default();
        motion.set_box(true);
        motion.advance(0.06);
        let b = Bounds::new((900.0, 900.0), (450.0, 450.0), true, &motion);
        assert!(b.hole > 1.0, "expected active center hole mid-morph");

        // A tile whose unpadded rect misses the hole by 1px must STILL be clipped
        // once hover zoom (1.04×) and padding are accounted for.
        let near_hole = Tile {
            x: b.hole + 40.0 + 1.0,
            y: 0.0,
            width: 80.0,
            height: 60.0,
            emerge_alpha: 1.0,
            emerge_scale: 1.0,
        };
        assert!(
            !b.tile_strictly_inside(&near_hole, false),
            "tile whose hover-zoomed / padded footprint touches center hole must be clipped"
        );

        // A tile placed flush near a rounded corner must be rejected for both
        // card and non-card modes (regression for unsafe shrinking inset).
        let near_corner = Tile {
            x: b.width * 0.5 - 32.0,
            y: b.height * 0.5 - 32.0,
            width: 60.0,
            height: 60.0,
            emerge_alpha: 1.0,
            emerge_scale: 1.0,
        };
        assert!(!b.tile_strictly_inside(&near_corner, false));
        assert!(!b.tile_strictly_inside(&near_corner, true));

        // A small tile well inside a settled panel with no hole needs no clip.
        motion.advance(3.0);
        let settled = Bounds::new((900.0, 900.0), (450.0, 450.0), true, &motion);
        let safe = Tile {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 60.0,
            emerge_alpha: 1.0,
            emerge_scale: 1.0,
        };
        assert!(settled.tile_strictly_inside(&safe, true));
    }

    #[test]
    fn settled_box_shell_and_color_picker_caches_invalidate_across_radius_hue_and_scales() {
        let palette = Palette::default();
        let mut motion = Motion::default();
        motion.set_box(true);
        motion.advance(10.0);
        let base_bounds = Bounds::new((900.0, 900.0), (450.0, 450.0), true, &motion);

        // Verify SettledBoxKey distinguishes corner radius changes even when
        // width, height, header, footer, material, and palette match.
        let sample_corner = |radius: f64| {
            let mut b = base_bounds;
            b.radius = radius;
            let mut surf = ImageSurface::create(Format::ARgb32, 900, 900).unwrap();
            {
                let cr = Context::new(&surf).unwrap();
                cr.translate(b.cx, b.cy);
                draw_settled_box_shell_cached(&cr, b, Material::Solid, false, &palette).unwrap();
            }
            let stride = surf.stride() as usize;
            let data = surf.data().unwrap();
            // Point inside the square corner of a sharp radius (10) but outside a huge radius (140).
            let px = (b.cx - b.width * 0.5 + 18.0) as usize;
            let py = (b.cy - b.height * 0.5 + 18.0) as usize;
            data[py * stride + px * 4 + 3]
        };
        let sharp_alpha = sample_corner(10.0);
        let rounded_alpha = sample_corner(140.0);
        assert!(
            sharp_alpha > 150 && rounded_alpha < 40,
            "SettledBoxKey must invalidate on radius change: sharp={sharp_alpha} rounded={rounded_alpha}"
        );

        // Verify ColorPicker SV/Hue caches invalidate on hue & scale (1.0, 1.5, 2.0).
        let sample_sv_corner = |hue: f64, scale: f64| {
            let mut surf = ImageSurface::create(
                Format::ARgb32,
                (300.0 * scale).ceil() as i32,
                (200.0 * scale).ceil() as i32,
            )
            .unwrap();
            surf.set_device_scale(scale, scale);
            {
                let cr = Context::new(&surf).unwrap();
                draw_color_sv_cached(&cr, hue, 10.0, 10.0, 240.0, 140.0, 1.0).unwrap();
                draw_color_hue_cached(&cr, 10.0, 160.0, 240.0, 14.0, palette.rim_top, 1.0)
                    .unwrap();
            }
            let stride = surf.stride() as usize;
            let data = surf.data().unwrap();
            // Near top-right (high saturation, high value): strongly tracks hue.
            let px = (230.0 * scale) as usize;
            let py = (25.0 * scale) as usize;
            let at = py * stride + px * 4;
            // ARGB32 on little-endian: [B, G, R, A]
            [data[at + 2], data[at + 1], data[at], data[at + 3]]
        };
        for scale in [1.0, 1.5, 2.0] {
            let red = sample_sv_corner(0.0, scale);
            let blue = sample_sv_corner(240.0, scale);
            assert!(
                red[0] > 180 && red[2] < 80,
                "scale={scale} red SV sample unexpected: {red:?}"
            );
            assert!(
                blue[2] > 180 && blue[0] < 80,
                "scale={scale} blue SV sample unexpected: {blue:?}"
            );
        }
    }

    #[test]
    fn explorer_modes_stay_within_damage_rect_at_fractional_hidpi_scale() {
        let palette = Palette::default();
        let scale = 1.5_f64;
        let w = 900.0;
        let h = 900.0;

        let check_pixels_in_damage = |surf: &mut ImageSurface, dmg: RectangleInt, label: &str| {
            let pw = surf.width();
            let ph = surf.height();
            let stride = surf.stride() as usize;
            let data = surf.data().unwrap();
            let mut painted = 0usize;
            for dy in 0..ph {
                let row = dy as usize * stride;
                for dx in 0..pw {
                    if data[row + dx as usize * 4 + 3] != 0 {
                        painted += 1;
                        let lx = (f64::from(dx) / scale).floor() as i32;
                        let ly = (f64::from(dy) / scale).floor() as i32;
                        assert!(
                            lx >= dmg.x()
                                && ly >= dmg.y()
                                && lx < dmg.x() + dmg.width()
                                && ly < dmg.y() + dmg.height(),
                            "{label}: device pixel ({dx},{dy}) logical ({lx},{ly}) escaped damage {dmg:?}"
                        );
                    }
                }
            }
            assert!(painted > 100, "{label}: expected painted pixels");
        };

        // Box morph mid at 1.5x
        let mut motion = Motion::default();
        motion.update(items(5), Some(0), false);
        motion.set_box(true);
        motion.update(items(12), Some(0), true);
        motion.advance(0.06);
        let bounds = Bounds::new((w, h), (450.0, 450.0), true, &motion);
        let grid = GridState {
            first: 0,
            total: 24,
            selected: Some(0),
            shortcuts: true,
            dragging_thumb: false,
        };
        let mut surf = ImageSurface::create(
            Format::ARgb32,
            (w * scale) as i32,
            (h * scale) as i32,
        )
        .unwrap();
        surf.set_device_scale(scale, scale);
        {
            let cr = Context::new(&surf).unwrap();
            let view = View {
                motion: &motion,
                palette: &palette,
                count: 12,
                pressed: None,
                title: "Applications",
                query: "app",
                suggestion: "s",
                status: "24 apps",
                page: None,
                high_contrast: false,
                solid: false,
                blur_available: true,
                loading_accent: None,
            };
            draw(&cr, bounds, &view, &grid).unwrap();
        }
        check_pixels_in_damage(&mut surf, bounds.damage_rect(), "box 1.5x mid");

        // ColorPicker input at 1.5x
        let mut imotion = Motion::default();
        imotion.set_input(true);
        imotion.advance(0.08);
        let ibounds = Bounds::new_with_input_height(
            (w, h),
            (450.0, 450.0),
            false,
            &imotion,
            COLOR_INPUT_H,
        );
        let input = InputState {
            title: "Ring Top".into(),
            text: "#422614".into(),
            password: false,
            cursor: 7,
            selection: None,
            selected_button: Some(0),
            pressed_button: None,
            shortcuts: true,
            accent: palette.accent_action,
            color: Some(ColorPickerState::from_rgb(0x42, 0x26, 0x14)),
        };
        let mut isurf = ImageSurface::create(
            Format::ARgb32,
            (w * scale) as i32,
            (h * scale) as i32,
        )
        .unwrap();
        isurf.set_device_scale(scale, scale);
        {
            let cr = Context::new(&isurf).unwrap();
            let view = View {
                motion: &imotion,
                palette: &palette,
                count: 0,
                pressed: None,
                title: "Ring Top",
                query: "",
                suggestion: "",
                status: "",
                page: None,
                high_contrast: false,
                solid: false,
                blur_available: true,
                loading_accent: None,
            };
            draw_input(&cr, ibounds, &view, &input).unwrap();
        }
        check_pixels_in_damage(&mut isurf, ibounds.damage_rect(), "color picker 1.5x mid");
    }
}
