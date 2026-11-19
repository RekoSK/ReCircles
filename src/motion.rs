//! Presentation state for the ring. Retarget the values already on screen so
//! that another keystroke can interrupt a transition at any frame.
use crate::ring::Item;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

#[derive(Clone, Copy)]
struct Spring {
    value: f64,
    velocity: f64,
    goal: f64,
}

impl Default for Spring {
    fn default() -> Self {
        Self::at(0.0)
    }
}

impl Spring {
    fn at(value: f64) -> Self {
        Self {
            value,
            velocity: 0.0,
            goal: value,
        }
    }

    fn advance(&mut self, dt: f64) -> bool {
        // Closed-form damped spring, independent of the monitor refresh rate.
        let offset = self.value - self.goal;
        let b = (self.velocity + 15.0 * offset) / 21.0;
        let (sin, cos) = (21.0 * dt).sin_cos();
        let decay = (-15.0 * dt).exp();
        let wave = offset * cos + b * sin;
        self.value = self.goal + decay * wave;
        self.velocity = decay * (21.0 * (b * cos - offset * sin) - 15.0 * wave);
        if (self.value - self.goal).abs() < 0.0005 && self.velocity.abs() < 0.01 {
            *self = Self::at(self.goal);
            return false;
        }
        true
    }

    fn advance_critical(&mut self, dt: f64, omega: f64) -> bool {
        // Closed-form critically damped spring (damping = 1.0): zero overshoot,
        // velocity-preserving on mid-flight reversals, independent of refresh rate.
        let offset = self.value - self.goal;
        let slope = self.velocity + omega * offset;
        let decay = (-omega * dt).exp();
        self.value = self.goal + decay * (offset + slope * dt);
        self.velocity = decay * (self.velocity - omega * slope * dt);
        if (self.value - self.goal).abs() < 0.0005 && self.velocity.abs() < 0.01 {
            *self = Self::at(self.goal);
            return false;
        }
        true
    }
}

fn ease(value: &mut f64, goal: f64, blend: f64) -> bool {
    *value += (goal - *value) * blend;
    if (*value - goal).abs() < 0.002 {
        *value = goal;
        return false;
    }
    true
}

pub struct Face {
    pub item: Item,
    pub opacity: f64,
}

#[derive(Default)]
pub struct Slot {
    pub faces: Vec<Face>,
    pub index: Option<usize>,
    last_index: usize,
    grid_index: usize,
    ring_angle: Option<f64>,
    ring_sector: Option<(f64, f64)>,
    ring_sibling: bool,
    merging_to_ring: bool,
    born_in_box: bool,
    width: f64,
    width_goal: f64,
    focus: Spring,
    pop: Spring,
}

impl Slot {
    fn matches(&self, item: &Item) -> bool {
        self.faces.last().is_some_and(|face| face.item == *item)
    }

    fn show(&mut self, index: usize, item: Item) {
        if !self.matches(&item) {
            let face = self
                .faces
                .iter()
                .position(|f| f.item == item)
                .map(|i| self.faces.remove(i))
                .unwrap_or(Face { item, opacity: 0.0 });
            self.faces.push(face);
            // Kick the new icon without resetting its current position.
            self.pop.velocity = -5.0;
        }
        self.index = Some(index);
        self.last_index = index;
    }

    pub fn focus(&self) -> f64 {
        self.focus.value
    }

    pub fn focused(&self) -> bool {
        self.focus.goal > 0.0
    }

    pub fn last_index(&self) -> usize {
        self.last_index
    }

    pub fn grid_index(&self) -> usize {
        self.grid_index
    }

    pub fn ring_angle(&self) -> Option<f64> {
        self.ring_angle
    }

    pub fn ring_sector(&self) -> Option<(f64, f64)> {
        self.ring_sector
    }

    pub fn ring_sibling(&self) -> bool {
        self.ring_sibling
    }

    pub fn merging_to_ring(&self) -> bool {
        self.merging_to_ring
    }

    pub fn born_in_box(&self) -> bool {
        self.born_in_box
    }

    pub fn tilt(&self) -> f64 {
        (self.pop.value + self.focus.velocity * 0.006).clamp(-0.16, 0.16)
    }

    fn advance(&mut self, layout_blend: f64, fade_blend: f64, dt: f64) -> bool {
        let active = self.index.is_some() || self.merging_to_ring;
        let mut moving = ease(&mut self.width, self.width_goal, layout_blend);
        let last = self.faces.len().saturating_sub(1);
        let mut i = 0;
        self.faces.retain_mut(|face| {
            let incoming = active && i == last;
            moving |= ease(&mut face.opacity, f64::from(incoming), fade_blend);
            i += 1;
            incoming || face.opacity > 0.0
        });
        moving | self.focus.advance_critical(dt, 26.0) | self.pop.advance(dt)
    }
}

pub struct Sector<'a> {
    pub slot: &'a Slot,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct LoadingPose {
    /// Orthonormal 3×3 world rotation matrix for the 3D ring.
    pub rotation: [[f64; 3]; 3],
    /// 0.0 = flat 2D ring, 1.0 = full 3D depth/thickness.
    pub depth: f64,
}

#[derive(Clone, Copy, Debug)]
struct Quat {
    w: f64,
    x: f64,
    y: f64,
    z: f64,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    fn from_axis_angle(axis: [f64; 3], angle: f64) -> Self {
        let (s, c) = (angle * 0.5).sin_cos();
        Self {
            w: c,
            x: axis[0] * s,
            y: axis[1] * s,
            z: axis[2] * s,
        }
    }

    fn from_omega(omega: [f64; 3], dt: f64) -> Self {
        let speed = (omega[0] * omega[0] + omega[1] * omega[1] + omega[2] * omega[2]).sqrt();
        let angle = speed * dt;
        if angle < 1e-9 {
            Self::IDENTITY
        } else {
            Self::from_axis_angle([omega[0] / speed, omega[1] / speed, omega[2] / speed], angle)
        }
    }

    fn mul(self, rhs: Self) -> Self {
        Self {
            w: self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
            x: self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            y: self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            z: self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
        }
    }

    fn normalize(self) -> Self {
        let norm = (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if norm > 0.0 {
            Self {
                w: self.w / norm,
                x: self.x / norm,
                y: self.y / norm,
                z: self.z / norm,
            }
        } else {
            Self::IDENTITY
        }
    }

    fn rotate_z(self) -> [f64; 3] {
        [
            2.0 * (self.x * self.z + self.w * self.y),
            2.0 * (self.y * self.z - self.w * self.x),
            1.0 - 2.0 * (self.x * self.x + self.y * self.y),
        ]
    }

    fn to_matrix(self) -> [[f64; 3]; 3] {
        let (w, x, y, z) = (self.w, self.x, self.y, self.z);
        [
            [
                1.0 - 2.0 * (y * y + z * z),
                2.0 * (x * y - w * z),
                2.0 * (x * z + w * y),
            ],
            [
                2.0 * (x * y + w * z),
                1.0 - 2.0 * (x * x + z * z),
                2.0 * (y * z - w * x),
            ],
            [
                2.0 * (x * z - w * y),
                2.0 * (y * z + w * x),
                1.0 - 2.0 * (x * x + y * y),
            ],
        ]
    }
}

struct Rng(u64);

impl Default for Rng {
    fn default() -> Self {
        Self(0x9e37_79b9_7f4a_7c15)
    }
}

impl Rng {
    fn from_entropy() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        let mut x = nanos
            .wrapping_add(n.wrapping_mul(0x9e37_79b9_7f4a_7c15))
            ^ 0xa076_1d64_78bd_642f;
        // SplitMix64 finalizer so close timestamps start far apart.
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^= x >> 31;
        Self(if x == 0 { 1 } else { x })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

#[derive(Default)]
struct LoadingSpin {
    active: bool,
    pending: bool,
    orientation: Quat,
    omega: [f64; 3],
    omega_target: [f64; 3],
    azimuth: f64,
    drift: f64,
    xy_speed: f64,
    retarget_in: f64,
    settle_goal: Quat,
    settle_axis: [f64; 3],
    settle: Spring,
    depth: f64,
    rng: Rng,
}

impl LoadingSpin {
    fn is_idle(&self) -> bool {
        !self.active
            && self.depth == 0.0
            && (self.settle.value - self.settle.goal).abs() < 0.0005
            && self.settle.velocity.abs() < 0.01
    }

    fn is_requested(&self) -> bool {
        self.active || self.pending
    }

    fn engage(&mut self) {
        self.pending = false;
        if !self.active {
            self.active = true;
            self.rng = Rng::from_entropy();
            self.pick_target(true);
        }
    }

    fn pick_target(&mut self, initial: bool) {
        if initial {
            self.azimuth = self.rng.range(0.0, TAU);
        } else {
            let sign = if self.rng.next_u64() & 1 == 0 {
                1.0
            } else {
                -1.0
            };
            self.azimuth = (self.azimuth + sign * self.rng.range(1.15, 2.75)).rem_euclid(TAU);
        }
        self.xy_speed = self.rng.range(2.6, 3.7);
        self.drift = self.rng.range(-1.3, 1.3);
        let wz = self.rng.range(-1.15, 1.15);
        self.omega_target = [
            self.xy_speed * self.azimuth.cos(),
            self.xy_speed * self.azimuth.sin(),
            wz,
        ];
        self.retarget_in = self.rng.range(0.50, 1.05);
    }

    fn set_active(&mut self, active: bool, animate: bool, defer: bool) -> bool {
        if !animate {
            *self = Self::default();
            return false;
        }
        if active {
            if defer && self.is_idle() {
                self.pending = true;
                return true;
            }
            self.engage();
            return true;
        }
        self.pending = false;
        if self.active {
            self.active = false;
            let n = self.orientation.rotate_z();
            let sign = if n[2] >= 0.0 { 1.0 } else { -1.0 };
            let ax = [n[1] * sign, -n[0] * sign, 0.0];
            let sin_a = ax[0].hypot(ax[1]);
            let cos_a = n[2] * sign;
            let angle = sin_a.atan2(cos_a);
            if sin_a > 1e-8 {
                let ux = ax[0] / sin_a;
                let uy = ax[1] / sin_a;
                let dq = Quat::from_axis_angle([ux, uy, 0.0], angle);
                self.settle_goal = dq.mul(self.orientation).normalize();
                self.settle_axis = [-ux, -uy, 0.0];
                let vel = (self.omega[0] * self.settle_axis[0]
                    + self.omega[1] * self.settle_axis[1])
                    .clamp(-4.0, 4.0);
                self.settle = Spring {
                    value: angle,
                    velocity: vel,
                    goal: 0.0,
                };
            } else {
                self.settle_goal = self.orientation;
                self.settle_axis = [1.0, 0.0, 0.0];
                self.settle = Spring::at(0.0);
            }
        }
        !self.is_idle()
    }

    fn advance(&mut self, dt: f64, panel_waiting: bool) -> bool {
        if self.active {
            if dt > 0.0 {
                let dt_spin = dt.min(0.05);
                self.retarget_in -= dt_spin;
                if self.retarget_in <= 0.0 {
                    self.pick_target(false);
                } else {
                    self.azimuth = (self.azimuth + self.drift * dt_spin).rem_euclid(TAU);
                    self.omega_target[0] = self.xy_speed * self.azimuth.cos();
                    self.omega_target[1] = self.xy_speed * self.azimuth.sin();
                }
                let steer = 1.0 - (-3.5 * dt_spin).exp();
                for i in 0..3 {
                    self.omega[i] += (self.omega_target[i] - self.omega[i]) * steer;
                }
                self.orientation = Quat::from_omega(self.omega, dt_spin)
                    .mul(self.orientation)
                    .normalize();
                let depth_blend = 1.0 - (-10.0 * dt_spin).exp();
                ease(&mut self.depth, 1.0, depth_blend);
            }
            return true;
        }
        if self.is_idle() {
            return self.pending;
        }
        let (moving, handoff) = if panel_waiting {
            let depth_blend = 1.0 - (-15.0 * dt).exp();
            let moving = self.settle.advance_critical(dt, 20.0)
                | ease(&mut self.depth, 0.0, depth_blend);
            let handoff = self.settle.value.abs() < 0.015 && self.depth < 0.015;
            (moving, handoff)
        } else {
            let depth_blend = 1.0 - (-14.0 * dt).exp();
            let moving = self.settle.advance(dt) | ease(&mut self.depth, 0.0, depth_blend);
            (moving, false)
        };
        if moving && !handoff {
            self.orientation = Quat::from_axis_angle(self.settle_axis, self.settle.value)
                .mul(self.settle_goal)
                .normalize();
            self.omega = [
                self.settle_axis[0] * self.settle.velocity,
                self.settle_axis[1] * self.settle.velocity,
                0.0,
            ];
            true
        } else {
            self.orientation = Quat::IDENTITY;
            self.omega = [0.0; 3];
            self.settle = Spring::at(0.0);
            self.depth = 0.0;
            false
        }
    }

    fn pose(&self) -> Option<LoadingPose> {
        if self.is_idle() {
            return None;
        }
        Some(LoadingPose {
            rotation: self.orientation.to_matrix(),
            depth: self.depth,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmptyRing {
    /// 0.0 = normal ring, 1.0 = smaller empty ring.
    pub mix: f64,
    /// 0.0 = static smaller ring, 1.0 = full 2D loading pulse amplitude.
    pub pulse: f64,
    /// Pulse wave in [-1.0, 1.0] driving the ring width oscillation (0.0 when not pulsing).
    pub wave: f64,
}

#[derive(Default)]
struct EmptyPulse {
    active: bool,
    pulsing: bool,
    animated: bool,
    mix: Spring,
    pulse: Spring,
    phase: f64,
}

impl EmptyPulse {
    fn is_idle(&self) -> bool {
        !self.active
            && !self.pulsing
            && (self.mix.value - self.mix.goal).abs() < 0.0005
            && self.mix.velocity.abs() < 0.01
            && (self.pulse.value - self.pulse.goal).abs() < 0.0005
            && self.pulse.velocity.abs() < 0.01
    }

    fn set_active(&mut self, active: bool, pulsing: bool, animate: bool) -> bool {
        let pulsing = active && pulsing;
        self.active = active;
        self.pulsing = pulsing;
        self.animated = animate;
        self.mix.goal = f64::from(active);
        self.pulse.goal = f64::from(pulsing);
        if !animate {
            self.mix = Spring::at(self.mix.goal);
            self.pulse = Spring::at(0.0);
            self.phase = 0.0;
            return false;
        }
        if pulsing && self.pulse.value <= 0.001 && self.pulse.velocity.abs() < 0.01 {
            self.phase = 0.0;
        }
        !self.is_idle()
    }

    fn advance(&mut self, dt: f64) -> bool {
        if !self.animated {
            self.mix = Spring::at(self.mix.goal);
            self.pulse = Spring::at(0.0);
            self.phase = 0.0;
            return false;
        }
        if self.is_idle() {
            self.mix = Spring::at(self.mix.goal);
            self.pulse = Spring::at(self.pulse.goal);
            self.phase = 0.0;
            return false;
        }
        let mix_moving = self.mix.advance_critical(dt, 11.0);
        let pulse_moving = self.pulse.advance_critical(dt, 11.0);
        if dt > 0.0 && (self.pulsing || self.pulse.value > 0.0005) {
            self.phase = (self.phase + 3.8 * dt.min(0.05)).rem_euclid(TAU);
        } else if !self.pulsing && !pulse_moving {
            self.phase = 0.0;
        }
        self.pulsing || mix_moving || pulse_moving
    }

    fn state(&self) -> Option<EmptyRing> {
        let mix = self.mix.value.clamp(0.0, 1.0);
        let pulse = self.pulse.value.clamp(0.0, 1.0);
        if mix <= 0.0005 && pulse <= 0.0005 && !self.active {
            return None;
        }
        let wave = if self.animated {
            -self.phase.sin() * pulse
        } else {
            0.0
        };
        Some(EmptyRing { mix, pulse, wave })
    }
}

fn grid_cell_pos(index: usize) -> (f64, f64) {
    use crate::menu::{GRID_COLUMNS, GRID_ROWS};
    const CELL_W: f64 = 142.0;
    const CELL_H: f64 = 110.0;
    const LEFT: f64 = 14.0;
    const RIGHT_PAD: f64 = 20.0;
    const WIDTH: f64 = LEFT + GRID_COLUMNS as f64 * CELL_W + RIGHT_PAD;
    const GRID_H: f64 = CELL_H * GRID_ROWS as f64;
    const HEADER_PLAIN: f64 = 48.0;
    const FOOTER: f64 = 34.0;
    let gx = LEFT + (index % GRID_COLUMNS) as f64 * CELL_W + CELL_W * 0.5 - WIDTH * 0.5;
    let gy = HEADER_PLAIN + ((index / GRID_COLUMNS) as f64 + 0.5) * CELL_H
        - (HEADER_PLAIN + GRID_H + FOOTER) * 0.5;
    (gx, gy)
}

fn ring_item_pos(angle: f64) -> (f64, f64) {
    const ITEM_RADIUS: f64 = 96.0;
    let (sin, cos) = angle.sin_cos();
    (ITEM_RADIUS * cos, ITEM_RADIUS * sin)
}

fn closest_cell_idx(grid_cells: &[usize], ring_angle: f64) -> usize {
    let (rx, ry) = ring_item_pos(ring_angle);
    grid_cells
        .iter()
        .enumerate()
        .min_by(|&(_, &a), &(_, &b)| {
            let (ax, ay) = grid_cell_pos(a);
            let (bx, by) = grid_cell_pos(b);
            (ax - rx)
                .hypot(ay - ry)
                .total_cmp(&(bx - rx).hypot(by - ry))
        })
        .map_or(0, |(idx, _)| idx)
}

/// Pair `grid_cells` with `ring_angles` by spatial proximity: each ring item
/// receives `⌊G / R⌋` closest grid tiles (minimizing total Euclidean distance),
/// and each leftover tile uses its closest ring item.
fn pair_grid_cells_to_ring(
    grid_cells: &[usize],
    ring_angles: &[f64],
) -> (Vec<usize>, Vec<Option<usize>>) {
    let g_count = grid_cells.len();
    let r_count = ring_angles.len();
    if g_count == 0 || r_count == 0 {
        return (vec![0; g_count], vec![None; r_count]);
    }
    let dist: Vec<Vec<f64>> = grid_cells
        .iter()
        .map(|&cell| {
            let (gx, gy) = grid_cell_pos(cell);
            ring_angles
                .iter()
                .map(|&angle| {
                    let (rx, ry) = ring_item_pos(angle);
                    (gx - rx).hypot(gy - ry)
                })
                .collect()
        })
        .collect();

    let mut assigned: Vec<Option<usize>> = vec![None; g_count];
    let quota = g_count / r_count;
    if quota > 0 {
        for _ in 0..quota {
            for r_start in 0..r_count {
                let mut best_cost_r = vec![f64::INFINITY; r_count];
                let mut best_cost_g = vec![f64::INFINITY; g_count];
                let mut prev_r_of_g = vec![0usize; g_count];
                let mut prev_g_of_r = vec![0usize; r_count];
                best_cost_r[r_start] = 0.0;
                for _ in 0..=r_count {
                    let mut changed = false;
                    for r in 0..r_count {
                        let cr = best_cost_r[r];
                        if !cr.is_finite() {
                            continue;
                        }
                        for g in 0..g_count {
                            if assigned[g] == Some(r) {
                                continue;
                            }
                            let c = cr + dist[g][r];
                            if c + 1e-9 < best_cost_g[g] {
                                best_cost_g[g] = c;
                                prev_r_of_g[g] = r;
                                changed = true;
                                if let Some(r2) = assigned[g] {
                                    let c2 = c - dist[g][r2];
                                    if c2 + 1e-9 < best_cost_r[r2] {
                                        best_cost_r[r2] = c2;
                                        prev_g_of_r[r2] = g;
                                    }
                                }
                            }
                        }
                    }
                    if !changed {
                        break;
                    }
                }
                if let Some(mut g_cur) = (0..g_count)
                    .filter(|&g| assigned[g].is_none())
                    .min_by(|&a, &b| best_cost_g[a].total_cmp(&best_cost_g[b]))
                {
                    loop {
                        let r_cur = prev_r_of_g[g_cur];
                        let g_next = prev_g_of_r[r_cur];
                        assigned[g_cur] = Some(r_cur);
                        if r_cur == r_start {
                            break;
                        }
                        g_cur = g_next;
                    }
                }
            }
        }
    } else {
        // Fewer grid cells than ring items: match each grid cell to a distinct closest ring item.
        let mut ring_owner: Vec<Option<usize>> = vec![None; r_count];
        for g_start in 0..g_count {
            let mut best_cost_g = vec![f64::INFINITY; g_count];
            let mut best_cost_r = vec![f64::INFINITY; r_count];
            let mut prev_g_of_r = vec![0usize; r_count];
            let mut prev_r_of_g = vec![0usize; g_count];
            best_cost_g[g_start] = 0.0;
            for _ in 0..=g_count {
                let mut changed = false;
                for g in 0..g_count {
                    let cg = best_cost_g[g];
                    if !cg.is_finite() {
                        continue;
                    }
                    for r in 0..r_count {
                        if ring_owner[r] == Some(g) {
                            continue;
                        }
                        let c = cg + dist[g][r];
                        if c + 1e-9 < best_cost_r[r] {
                            best_cost_r[r] = c;
                            prev_g_of_r[r] = g;
                            changed = true;
                            if let Some(g2) = ring_owner[r] {
                                let c2 = c - dist[g2][r];
                                if c2 + 1e-9 < best_cost_g[g2] {
                                    best_cost_g[g2] = c2;
                                    prev_r_of_g[g2] = r;
                                }
                            }
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
            if let Some(mut r_cur) = (0..r_count)
                .filter(|&r| ring_owner[r].is_none())
                .min_by(|&a, &b| best_cost_r[a].total_cmp(&best_cost_r[b]))
            {
                loop {
                    let g_cur = prev_g_of_r[r_cur];
                    let r_next = prev_r_of_g[g_cur];
                    ring_owner[r_cur] = Some(g_cur);
                    assigned[g_cur] = Some(r_cur);
                    if g_cur == g_start {
                        break;
                    }
                    r_cur = r_next;
                }
            }
        }
    }

    let assignment: Vec<usize> = (0..g_count)
        .map(|g| {
            assigned[g].unwrap_or_else(|| {
                (0..r_count)
                    .min_by(|&a, &b| dist[g][a].total_cmp(&dist[g][b]))
                    .unwrap_or(0)
            })
        })
        .collect();

    let primary_idx: Vec<Option<usize>> = (0..r_count)
        .map(|r| {
            (0..g_count)
                .filter(|&g| assignment[g] == r)
                .min_by(|&a, &b| dist[a][r].total_cmp(&dist[b][r]))
        })
        .collect();

    (assignment, primary_idx)
}

pub struct Motion {
    slots: Vec<Slot>,
    seam: f64,
    seam_goal: f64,
    opening: Spring,
    box_mix: Spring,
    list_mix: Spring,
    input_mix: Spring,
    list_rows: Spring,
    input_focus: [Spring; 2],
    loading: LoadingSpin,
    empty: EmptyPulse,
    in_box_layout: bool,
    last_ring_sectors: Vec<(f64, f64)>,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            seam: 0.0,
            seam_goal: 0.0,
            opening: Spring::at(1.0),
            box_mix: Spring::at(0.0),
            list_mix: Spring::at(0.0),
            input_mix: Spring::at(0.0),
            list_rows: Spring::at(crate::menu::LIST_ROWS as f64),
            input_focus: [Spring::at(0.0); 2],
            loading: LoadingSpin::default(),
            empty: EmptyPulse::default(),
            in_box_layout: false,
            last_ring_sectors: Vec::new(),
        }
    }
}

impl Motion {
    pub fn open(&mut self) {
        self.opening = Spring {
            value: 0.91,
            velocity: 0.0,
            goal: 1.0,
        };
    }

    pub fn scale(&self) -> f64 {
        self.opening.value
    }

    pub fn set_box(&mut self, active: bool) {
        self.box_mix.goal = f64::from(active);
        if active {
            self.list_mix.goal = 0.0;
            self.input_mix.goal = 0.0;
        } else if self.list_mix.goal == 0.0 && self.input_mix.goal == 0.0 {
            for slot in &mut self.slots {
                slot.born_in_box = false;
            }
        }
    }

    pub fn box_mix(&self) -> f64 {
        self.box_mix.value
    }

    pub fn set_list(&mut self, active: bool, rows: usize) {
        self.list_mix.goal = f64::from(active);
        if active {
            self.box_mix.goal = 0.0;
            self.input_mix.goal = 0.0;
            let target_rows = rows.clamp(1, crate::menu::LIST_ROWS) as f64;
            if self.list_mix.value <= 0.001 {
                self.list_rows = Spring::at(target_rows);
            } else {
                self.list_rows.goal = target_rows;
            }
        } else if self.box_mix.goal == 0.0 && self.input_mix.goal == 0.0 {
            for slot in &mut self.slots {
                slot.born_in_box = false;
            }
        }
    }

    pub fn list_mix(&self) -> f64 {
        self.list_mix.value
    }

    pub fn list_rows(&self) -> f64 {
        self.list_rows.value.clamp(1.0, crate::menu::LIST_ROWS as f64)
    }

    pub fn list_rows_settled(&self) -> bool {
        (self.list_rows.value - self.list_rows.goal).abs() < 0.001
            && self.list_rows.velocity.abs() < 0.01
    }

    pub fn set_input(&mut self, active: bool) {
        self.input_mix.goal = f64::from(active);
        if active {
            self.box_mix.goal = 0.0;
            self.list_mix.goal = 0.0;
        } else {
            self.input_focus[0].goal = 0.0;
            self.input_focus[1].goal = 0.0;
            if self.box_mix.goal == 0.0 && self.list_mix.goal == 0.0 {
                for slot in &mut self.slots {
                    slot.born_in_box = false;
                }
            }
        }
    }

    pub fn input_mix(&self) -> f64 {
        self.input_mix.value
    }

    pub fn panel_mix(&self) -> f64 {
        (self.box_mix.value + self.list_mix.value + self.input_mix.value).clamp(0.0, 1.0)
    }

    pub fn select_input_button(&mut self, selected: Option<usize>) -> bool {
        let mut changed = false;
        for (i, spring) in self.input_focus.iter_mut().enumerate() {
            let goal = f64::from(selected == Some(i));
            changed |= spring.goal != goal;
            spring.goal = goal;
        }
        changed
    }

    pub fn input_button_focus(&self, index: usize) -> f64 {
        self.input_focus
            .get(index)
            .map_or(0.0, |s| s.value.clamp(0.0, 1.0))
    }

    pub fn set_loading(&mut self, loading: bool, animate: bool) -> bool {
        let defer = loading && self.panel_mix() > 0.003;
        self.loading.set_active(loading, animate, defer)
    }

    pub fn loading_pose(&self) -> Option<LoadingPose> {
        self.loading.pose()
    }

    pub fn set_empty(&mut self, empty: bool, pulsing: bool, animate: bool) -> bool {
        self.empty.set_active(empty, pulsing, animate)
    }

    pub fn empty_ring(&self) -> Option<EmptyRing> {
        self.empty.state()
    }

    fn update_ring_to_box(&mut self, items: Vec<Item>, selected: Option<usize>, animate: bool) -> bool {
        let count = items.len();
        let ring_spans = self.last_ring_sectors.clone();
        let ring_angles: Vec<f64> = ring_spans.iter().map(|&(s, e)| (s + e) * 0.5).collect();
        let grid_cells: Vec<usize> = (0..count).collect();
        let (assignment, primary_g) = pair_grid_cells_to_ring(&grid_cells, &ring_angles);
        let width = TAU / count as f64;
        let mut old_slots: Vec<Option<Slot>> = self.slots.drain(..).map(Some).collect();
        let was_box_or_merging = self.in_box_layout
            || old_slots
                .iter()
                .flatten()
                .any(|s| s.merging_to_ring);

        for (g, item) in items.into_iter().enumerate() {
            let r = assignment[g];
            let is_primary = primary_g[r] == Some(g);
            let existing = if was_box_or_merging {
                old_slots
                    .iter_mut()
                    .find(|s| s.as_ref().is_some_and(|slot| slot.grid_index == g))
                    .and_then(Option::take)
                    .or_else(|| {
                        is_primary
                            .then(|| {
                                old_slots
                                    .iter_mut()
                                    .find(|s| s.as_ref().is_some_and(|slot| slot.index == Some(r)))
                                    .and_then(Option::take)
                            })
                            .flatten()
                    })
            } else if is_primary {
                old_slots
                    .iter_mut()
                    .find(|s| s.as_ref().is_some_and(|slot| slot.index == Some(r)))
                    .and_then(Option::take)
            } else {
                None
            };

            let mut slot = if let Some(mut slot) = existing {
                slot.show(g, item);
                if !is_primary && slot.faces.len() == 1 {
                    slot.faces[0].opacity = 1.0;
                }
                slot
            } else if is_primary {
                let mut slot = Slot::default();
                slot.show(g, item);
                slot
            } else {
                Slot {
                    faces: vec![Face { item, opacity: 1.0 }],
                    index: Some(g),
                    last_index: g,
                    pop: Spring {
                        value: 0.0,
                        velocity: -5.0,
                        goal: 0.0,
                    },
                    ..Slot::default()
                }
            };
            slot.grid_index = g;
            slot.ring_angle = Some(ring_angles[r]);
            slot.ring_sector = is_primary.then_some(ring_spans[r]);
            slot.ring_sibling = !is_primary;
            slot.merging_to_ring = false;
            slot.born_in_box = false;
            slot.width = width;
            slot.width_goal = width;
            self.slots.push(slot);
        }

        self.seam_goal = -FRAC_PI_2 - PI / count as f64;
        self.seam = self.seam_goal;
        self.in_box_layout = true;
        self.select(selected);
        self.advance(if animate { 0.0 } else { 10.0 })
    }

    fn update_box_to_ring(&mut self, items: Vec<Item>, selected: Option<usize>, animate: bool) -> bool {
        let count = items.len();
        let mut grid_slots: Vec<Slot> = self
            .slots
            .drain(..)
            .filter(|s| s.index.is_some() || s.merging_to_ring)
            .collect();
        grid_slots.sort_by_key(|s| s.grid_index);
        grid_slots.dedup_by_key(|s| s.grid_index);
        let grid_cells: Vec<usize> = grid_slots.iter().map(|s| s.grid_index).collect();

        self.seam_goal = -FRAC_PI_2 - PI / count as f64;
        self.seam = self.seam_goal;
        let width = TAU / count as f64;
        let ring_spans: Vec<(f64, f64)> = (0..count)
            .map(|r| {
                let start = self.seam_goal + r as f64 * width;
                (start, start + width)
            })
            .collect();
        let ring_angles: Vec<f64> = ring_spans.iter().map(|&(s, e)| (s + e) * 0.5).collect();
        self.last_ring_sectors = ring_spans.clone();

        let (assignment, primary_idx) = pair_grid_cells_to_ring(&grid_cells, &ring_angles);
        let mut old_grid_slots: Vec<Option<Slot>> = grid_slots.into_iter().map(Some).collect();

        for (r, item) in items.into_iter().enumerate() {
            let idx = primary_idx[r].unwrap_or_else(|| closest_cell_idx(&grid_cells, ring_angles[r]));
            let g_cell = grid_cells[idx];
            let mut slot = old_grid_slots[idx].take().unwrap_or_default();
            slot.show(r, item);
            slot.grid_index = g_cell;
            slot.ring_angle = Some(ring_angles[r]);
            slot.ring_sector = Some(ring_spans[r]);
            slot.ring_sibling = false;
            slot.merging_to_ring = false;
            slot.born_in_box = false;
            slot.width = width;
            slot.width_goal = width;
            self.slots.push(slot);
        }

        if animate && self.box_mix.value > 0.001 {
            for (idx, maybe_slot) in old_grid_slots.into_iter().enumerate() {
                if let Some(mut slot) = maybe_slot {
                    let r = assignment[idx];
                    slot.index = None;
                    slot.grid_index = grid_cells[idx];
                    slot.ring_angle = Some(ring_angles[r]);
                    slot.ring_sector = None;
                    slot.ring_sibling = true;
                    slot.merging_to_ring = true;
                    slot.born_in_box = false;
                    slot.width = 0.0;
                    slot.width_goal = 0.0;
                    slot.focus.goal = 0.0;
                    self.slots.push(slot);
                }
            }
        }

        self.in_box_layout = false;
        self.select(selected);
        self.advance(if animate { 0.0 } else { 10.0 })
    }

    pub fn update(&mut self, items: Vec<Item>, selected: Option<usize>, animate: bool) -> bool {
        let count = items.len();
        let target_box = self.box_mix.goal > 0.5;
        let target_list = self.list_mix.goal > 0.5;
        let target_input = self.input_mix.goal > 0.5;
        let target_ring = !target_box && !target_list && !target_input;
        let from_other_panel = self.list_mix.value >= 0.5 || self.input_mix.value >= 0.5;

        if target_list || target_input {
            self.in_box_layout = false;
            self.last_ring_sectors.clear();
            self.slots.retain(|s| !s.merging_to_ring);
            for slot in &mut self.slots {
                slot.ring_angle = None;
                slot.ring_sector = None;
                slot.ring_sibling = false;
                slot.merging_to_ring = false;
            }
        }

        if target_box && !from_other_panel && count > 0 {
            let entering_from_ring = !self.in_box_layout
                || (self.box_mix.value < 0.85 && !self.last_ring_sectors.is_empty());
            if entering_from_ring {
                if !self.in_box_layout && !self.slots.iter().any(|s| s.merging_to_ring) {
                    let current_ring: Vec<(f64, f64)> = self
                        .sectors()
                        .filter(|s| s.slot.index.is_some())
                        .map(|s| (s.start, s.end))
                        .collect();
                    if !current_ring.is_empty() {
                        self.last_ring_sectors = current_ring;
                    }
                }
                if !self.last_ring_sectors.is_empty() {
                    return self.update_ring_to_box(items, selected, animate);
                }
            }
        }

        if target_ring && count > 0 && !self.slots.is_empty() {
            let leaving_box = self.in_box_layout
                || (self.box_mix.value > 0.001
                    && self
                        .slots
                        .iter()
                        .any(|s| s.merging_to_ring || s.ring_angle.is_some()));
            if leaving_box
                && self
                    .slots
                    .iter()
                    .any(|s| s.index.is_some() || s.merging_to_ring)
            {
                return self.update_box_to_ring(items, selected, animate);
            }
        }

        let leaving_box_to_empty =
            target_ring && count == 0 && (self.in_box_layout || self.box_mix.value > 0.001);
        self.in_box_layout = target_box && count > 0;
        self.slots.retain(|s| !s.merging_to_ring);
        if count > 0 {
            self.seam_goal = -FRAC_PI_2 - PI / count as f64;
        }
        if self.slots.is_empty() {
            self.seam = self.seam_goal;
        }

        // Preserve matching neighbours. A removed folder shrinks where it was,
        // while a replacement in the same slot crossfades its icon and label.
        let mut pos = 0;
        let expanding_box = target_box || target_list;
        for (i, item) in items.iter().enumerate() {
            if let Some(next) = self.slots[pos..].iter().position(|s| s.matches(item)) {
                for slot in &mut self.slots[pos..pos + next] {
                    slot.index = None;
                }
                pos += next;
            } else if self
                .slots
                .get(pos)
                .is_none_or(|s| items[i + 1..].iter().any(|v| s.matches(v)))
            {
                self.slots.insert(
                    pos,
                    Slot {
                        born_in_box: expanding_box,
                        ..Slot::default()
                    },
                );
            }
            self.slots[pos].show(i, item.clone());
            self.slots[pos].grid_index = i;
            if !target_box {
                self.slots[pos].ring_angle = None;
                self.slots[pos].ring_sector = None;
                self.slots[pos].ring_sibling = false;
            }
            pos += 1;
        }
        for slot in &mut self.slots[pos..] {
            if leaving_box_to_empty && slot.index.is_some() {
                slot.born_in_box = true;
            }
            slot.index = None;
        }
        let width = TAU / count.max(1) as f64;
        for slot in &mut self.slots {
            slot.width_goal = if slot.index.is_some() { width } else { 0.0 };
        }
        // The same layout can end a full turn either way. Keep surviving
        // labels on the shorter arc (e.g. the last of eight becomes the only one).
        let mut displacement = 0.0;
        let mut weight = 0.0;
        for sector in self.sectors() {
            if let Some(i) = sector.slot.index {
                let opacity = sector.slot.faces.last().map_or(0.0, |f| f.opacity);
                let current = (sector.start + sector.end) * 0.5;
                let target = self.seam_goal + (i as f64 + 0.5) * width;
                displacement += opacity * (current - target);
                weight += opacity;
            }
        }
        if weight > 0.0 {
            self.seam_goal += TAU * (displacement / weight / TAU).round();
        } else if count > 0 {
            self.seam_goal += TAU * ((self.seam - self.seam_goal) / TAU).round();
        }
        if target_ring {
            if count > 0 {
                self.last_ring_sectors = (0..count)
                    .map(|r| {
                        let start = self.seam_goal + r as f64 * width;
                        (start, start + width)
                    })
                    .collect();
            } else if self.empty.active || self.loading.is_requested() || !animate {
                self.last_ring_sectors.clear();
            }
        }
        self.select(selected);
        // dt=0 detects unsettled values. A large step snaps reduced motion to rest.
        self.advance(if animate { 0.0 } else { 10.0 })
    }

    pub fn select(&mut self, selected: Option<usize>) -> bool {
        let mut changed = false;
        for slot in &mut self.slots {
            let goal = f64::from(selected.is_some() && slot.index == selected);
            changed |= slot.focus.goal != goal;
            slot.focus.goal = goal;
        }
        changed
    }

    pub fn advance(&mut self, dt: f64) -> bool {
        // Let departing names fade before their wedges become too narrow to read.
        let layout_blend = 1.0 - (-13.0 * dt).exp();
        let fade_blend = 1.0 - (-26.0 * dt).exp();
        let panel_wanted =
            self.box_mix.goal > 0.5 || self.list_mix.goal > 0.5 || self.input_mix.goal > 0.5;
        let mut loading_moving = self.loading.advance(dt, panel_wanted);
        // Hold panel expansion while the 3D loading ring is active or settling flat;
        // once flat, let the panel spring take over immediately on the same frame.
        let panel_moving = if self.loading.pose().is_some() {
            false
        } else {
            self.box_mix.advance_critical(dt, 18.0)
                | self.list_mix.advance_critical(dt, 18.0)
                | self.input_mix.advance_critical(dt, 18.0)
        };
        // Once a collapsing panel reaches the flat ring, hand off seamlessly to 3D spin.
        if self.loading.pending && self.panel_mix() <= 0.003 {
            self.box_mix = Spring::at(0.0);
            self.list_mix = Spring::at(0.0);
            self.input_mix = Spring::at(0.0);
            self.loading.engage();
            loading_moving |= self.loading.advance(dt, false);
        }
        let mut moving = self.opening.advance(dt)
            | panel_moving
            | self.list_rows.advance_critical(dt, 22.0)
            | self.input_focus[0].advance_critical(dt, 26.0)
            | self.input_focus[1].advance_critical(dt, 26.0)
            | ease(&mut self.seam, self.seam_goal, layout_blend)
            | loading_moving
            | self.empty.advance(dt);
        for slot in &mut self.slots {
            moving |= slot.advance(layout_blend, fade_blend, dt);
        }
        let box_active = self.box_mix.value > 0.001;
        self.slots
            .retain(|s| s.index.is_some() || s.width > 0.0 || (box_active && s.merging_to_ring));
        if !box_active && self.box_mix.goal == 0.0 {
            for slot in &mut self.slots {
                slot.ring_angle = None;
                slot.ring_sector = None;
                slot.ring_sibling = false;
                slot.merging_to_ring = false;
                slot.grid_index = slot.last_index;
            }
        }
        moving
    }

    pub fn sectors(&self) -> impl Iterator<Item = Sector<'_>> {
        let total: f64 = self.slots.iter().map(|s| s.width).sum();
        let mut angle = self.seam;
        let box_active = self.box_mix.value > 0.001;
        self.slots
            .iter()
            .filter(move |s| s.width > 0.0 || (box_active && s.merging_to_ring))
            .map(move |slot| {
                let start = angle;
                if total > 0.0 {
                    angle += TAU * slot.width / total;
                }
                Sector {
                    slot,
                    start,
                    end: angle,
                }
            })
    }

    pub fn hit(&self, angle: f64) -> Option<usize> {
        let angle = self.seam + (angle - self.seam).rem_euclid(TAU);
        let sector = self.sectors().find(|s| angle < s.end)?;
        // A fading old label must never launch its replacement.
        sector
            .slot
            .index
            .filter(|_| sector.slot.faces.last().is_some_and(|f| f.opacity >= 0.5))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring::Icon;

    #[test]
    fn box_transition_can_reverse_mid_spring_and_snap_for_reduced_motion() {
        let mut motion = Motion::default();
        motion.set_box(true);
        motion.advance(0.03);
        let mid = motion.box_mix();
        assert!((0.0..1.0).contains(&mid));
        motion.set_box(false);
        assert_eq!(motion.box_mix(), mid);
        for _ in 0..100 {
            motion.advance(0.016);
        }
        assert_eq!(motion.box_mix(), 0.0);
        motion.set_box(true);
        assert!(!motion.update(vec![], None, false));
        assert_eq!(motion.box_mix(), 1.0);
    }

    fn folders(labels: &[&str]) -> Vec<Item> {
        labels
            .iter()
            .map(|&label| Item {
                label: label.into(),
                icon: Icon::Folder,
                pixbuf: None,
                active: false,
            })
            .collect()
    }

    fn settle(motion: &mut Motion) {
        for _ in 0..120 {
            if !motion.advance(1.0 / 120.0) {
                return;
            }
        }
        panic!("ring continued ticking after one second");
    }

    #[test]
    fn filtering_to_the_last_folder_takes_the_short_arc_and_keeps_hits_aligned() {
        let mut motion = Motion::default();
        motion.update(
            folders(&["A", "B", "C", "D", "E", "F", "G", "H"]),
            None,
            false,
        );
        motion.update(folders(&["H"]), None, true);
        for _ in 0..60 {
            motion.advance(1.0 / 120.0);
            let mut span = 0.0;
            for sector in motion.sectors() {
                span += sector.end - sector.start;
                let center = (sector.start + sector.end) * 0.5;
                assert_eq!(motion.hit(center), sector.slot.index);
                if sector.slot.index.is_some() {
                    assert!((PI * 1.25 - 0.002..=PI * 1.5 + 0.002).contains(&center));
                }
            }
            assert!((span - TAU).abs() < 1e-9);
        }
        assert_eq!(motion.sectors().count(), 1);
    }

    #[test]
    fn retargeting_keeps_the_visible_blend_and_ignores_superseded_labels() {
        let mut motion = Motion::default();
        motion.update(folders(&["Projects"]), None, false);
        motion.update(folders(&["Photos"]), None, true);
        motion.advance(0.02);
        let opacity = motion.slots[0].faces[0].opacity;
        assert!(opacity > 0.5 && opacity < 1.0);
        assert_eq!(motion.hit(-FRAC_PI_2), None);
        motion.update(folders(&["Pictures"]), Some(0), true);
        assert_eq!(motion.slots[0].faces[0].opacity, opacity);
        motion.advance(0.05);
        assert_eq!(motion.hit(-FRAC_PI_2), Some(0));
        settle(&mut motion);
        assert_eq!(motion.slots[0].faces.len(), 1);
        assert_eq!(motion.slots[0].faces[0].item.label, "Pictures");
        assert_eq!(motion.slots[0].tilt(), 0.0);
    }

    #[test]
    fn empty_results_can_reverse_mid_fade_and_reduced_motion_settles_immediately() {
        let mut motion = Motion::default();
        motion.update(folders(&["One", "Two"]), None, false);
        motion.update(vec![], None, true);
        motion.advance(0.04);
        assert_eq!(motion.hit(-FRAC_PI_2), None);
        assert_eq!(motion.sectors().count(), 2);
        motion.update(folders(&["Two", "Three", "Four"]), None, true);
        settle(&mut motion);
        assert_eq!(motion.sectors().count(), 3);
        motion.open();
        assert!(!motion.update(folders(&["One"]), Some(0), false));
        assert_eq!(motion.scale(), 1.0);
        assert_eq!(motion.slots[0].focus(), 1.0);
        assert_eq!(motion.slots[0].tilt(), 0.0);
        assert_eq!(motion.hit(PI), Some(0));
        assert!(!motion.update(vec![], None, false));
        assert_eq!(motion.sectors().count(), 0);
    }

    #[test]
    fn loading_spin_tumbles_in_3d_and_settles_flat_when_finished() {
        let mut motion = Motion::default();
        assert!(motion.loading_pose().is_none());

        // Reduced motion never enters the 3D spin.
        assert!(!motion.set_loading(true, false));
        assert!(motion.loading_pose().is_none());

        assert!(motion.set_loading(true, true));
        let mut max_nx = 0.0_f64;
        let mut max_ny = 0.0_f64;
        for _ in 0..180 {
            assert!(motion.advance(1.0 / 60.0));
            let pose = motion.loading_pose().expect("active loading pose");
            let r = pose.rotation;
            let nx = r[0][2];
            let ny = r[1][2];
            let nz = r[2][2];
            assert!((nx * nx + ny * ny + nz * nz - 1.0).abs() < 1e-6);
            max_nx = max_nx.max(nx.abs());
            max_ny = max_ny.max(ny.abs());
        }
        let pose = motion.loading_pose().expect("active loading pose");
        assert!(pose.depth > 0.95);
        assert!(max_nx > 0.25 && max_ny > 0.25, "should tumble across multiple 3D axes");

        assert!(motion.set_loading(false, true));
        settle(&mut motion);
        assert!(motion.loading_pose().is_none());
    }

    #[test]
    fn input_and_list_transitions_reverse_cleanly_and_preserve_panel_unity() {
        let mut motion = Motion::default();
        motion.set_input(true);
        motion.select_input_button(Some(0));
        motion.advance(0.03);
        let mid_input = motion.input_mix();
        let mid_focus = motion.input_button_focus(0);
        assert!((0.0..1.0).contains(&mid_input));
        assert!((0.0..1.0).contains(&mid_focus));

        // Reversing mid-flight preserves current position without jumping.
        motion.set_input(false);
        assert_eq!(motion.input_mix(), mid_input);
        settle(&mut motion);
        assert_eq!(motion.input_mix(), 0.0);
        assert_eq!(motion.input_button_focus(0), 0.0);

        // Transitioning from settled grid box to input box keeps panel_mix == 1.0 throughout.
        motion.set_box(true);
        motion.update(folders(&["A", "B", "C"]), None, false);
        assert_eq!(motion.box_mix(), 1.0);
        motion.set_input(true);
        for _ in 0..30 {
            motion.advance(1.0 / 120.0);
            assert!((motion.panel_mix() - 1.0).abs() < 1e-6);
        }
        settle(&mut motion);
        assert_eq!(motion.input_mix(), 1.0);
        assert_eq!(motion.box_mix(), 0.0);

        // List mode tracks row count smoothly and settles immediately under reduced motion.
        motion.set_list(true, 4);
        assert!(!motion.update(folders(&["1", "2", "3", "4"]), None, false));
        assert_eq!(motion.list_mix(), 1.0);
        assert_eq!(motion.list_rows(), 4.0);
        assert!(motion.list_rows_settled());

        // Canceling input and returning to a grid view keeps slots born_in_box intact.
        let mut motion = Motion::default();
        motion.set_input(true);
        motion.update(folders(&["A", "B"]), None, true);
        motion.advance(0.1);
        motion.set_box(true);
        motion.set_list(false, 1);
        motion.set_input(false);
        motion.update(folders(&["A", "B", "C", "D"]), None, true);
        assert!(
            motion
                .sectors()
                .filter(|s| s.slot.index.unwrap_or(0) >= 2)
                .all(|s| s.slot.born_in_box()),
            "new grid slots after canceling input must stay born_in_box"
        );
    }

    #[test]
    fn empty_ring_shrinks_steadily_and_2d_loading_pulses_width() {
        let mut motion = Motion::default();
        assert!(motion.empty_ring().is_none());

        // Reduced motion snaps directly without continuous ticking.
        assert!(!motion.set_empty(true, false, false));
        assert_eq!(
            motion.empty_ring(),
            Some(EmptyRing {
                mix: 1.0,
                pulse: 0.0,
                wave: 0.0
            })
        );
        assert!(!motion.set_empty(false, false, false));
        assert!(motion.empty_ring().is_none());

        // Empty menu (pulsing = false) smoothly shrinks to a static smaller ring and stops ticking.
        assert!(motion.set_empty(true, false, true));
        settle(&mut motion);
        assert_eq!(
            motion.empty_ring(),
            Some(EmptyRing {
                mix: 1.0,
                pulse: 0.0,
                wave: 0.0
            })
        );

        // 2D loading (pulsing = true) pulses the width continuously.
        assert!(motion.set_empty(true, true, true));
        let mut min_wave = 1.0_f64;
        let mut max_wave = -1.0_f64;
        for _ in 0..120 {
            assert!(motion.advance(1.0 / 60.0));
            let state = motion.empty_ring().expect("active 2D loading ring state");
            min_wave = min_wave.min(state.wave);
            max_wave = max_wave.max(state.wave);
        }
        let state = motion.empty_ring().unwrap();
        assert!(state.mix > 0.99);
        assert!(state.pulse > 0.99);
        assert!(min_wave < -0.9 && max_wave > 0.9);

        assert!(motion.set_empty(false, false, true));
        settle(&mut motion);
        assert!(motion.empty_ring().is_none());
    }

    #[test]
    fn grid_transitions_split_from_and_merge_into_ring_items_evenly_and_with_remainder() {
        // Divisible case: 6 ring items <-> 12 grid items (2 grid items per ring item).
        let mut motion = Motion::default();
        let ring6 = folders(&["R0", "R1", "R2", "R3", "R4", "R5"]);
        let grid12 = folders(&[
            "G0", "G1", "G2", "G3", "G4", "G5", "G6", "G7", "G8", "G9", "G10", "G11",
        ]);
        motion.update(ring6.clone(), None, false);
        let ring_angles: Vec<f64> = motion
            .sectors()
            .map(|s| (s.start + s.end) * 0.5)
            .collect();
        assert_eq!(ring_angles.len(), 6);

        motion.set_box(true);
        motion.update(grid12.clone(), None, true);
        let mut counts = vec![0usize; 6];
        let mut primaries = 0usize;
        for s in motion.sectors() {
            assert!(!s.slot.born_in_box());
            let angle = s.slot.ring_angle().expect("grid tile paired to ring item");
            let r = ring_angles
                .iter()
                .position(|&a| (a - angle).abs() < 1e-9)
                .expect("angle must match one of the 6 ring items");
            counts[r] += 1;
            if !s.slot.ring_sibling() {
                primaries += 1;
            }
        }
        assert_eq!(counts, vec![2, 2, 2, 2, 2, 2]);
        assert_eq!(primaries, 6);
        settle(&mut motion);
        assert_eq!(motion.box_mix(), 1.0);

        // Close grid back to 6 ring items: all 12 tiles animate into the 6 ring items.
        motion.set_box(false);
        motion.update(ring6, None, true);
        motion.advance(0.04);
        assert!(motion.box_mix() > 0.1);
        let closing_sectors: Vec<_> = motion.sectors().collect();
        assert_eq!(closing_sectors.len(), 12);
        let mut seen_cells = [false; 12];
        let mut merging = 0usize;
        let mut active = 0usize;
        for s in &closing_sectors {
            seen_cells[s.slot.grid_index()] = true;
            assert!(s.slot.ring_angle().is_some());
            if s.slot.merging_to_ring() {
                merging += 1;
                assert!(s.slot.index.is_none());
            } else {
                active += 1;
                assert!(s.slot.index.is_some());
            }
        }
        assert!(seen_cells.iter().all(|&seen| seen));
        assert_eq!(active, 6);
        assert_eq!(merging, 6);

        settle(&mut motion);
        assert_eq!(motion.box_mix(), 0.0);
        assert_eq!(motion.sectors().count(), 6);
        assert!(motion.sectors().all(|s| s.slot.ring_angle().is_none()));

        // Non-divisible cases: 12 grid items with 5 or 7 ring items.
        for r_count in [5usize, 7usize, 8usize] {
            let labels: Vec<String> = (0..r_count).map(|i| format!("Ring {i}")).collect();
            let ring_items: Vec<Item> = labels
                .iter()
                .map(|l| Item {
                    label: l.clone(),
                    icon: Icon::Folder,
                    pixbuf: None,
                    active: false,
                })
                .collect();
            let mut m = Motion::default();
            m.update(ring_items, None, false);
            let angles: Vec<f64> = m.sectors().map(|s| (s.start + s.end) * 0.5).collect();
            m.set_box(true);
            m.update(grid12.clone(), None, true);
            let mut per_ring = vec![0usize; r_count];
            for s in m.sectors() {
                assert!(!s.slot.born_in_box());
                let a = s.slot.ring_angle().unwrap();
                let r = angles
                    .iter()
                    .position(|&ra| (ra - a).abs() < 1e-9)
                    .unwrap();
                per_ring[r] += 1;
            }
            let min_quota = 12 / r_count;
            assert_eq!(per_ring.iter().sum::<usize>(), 12);
            assert!(per_ring.iter().all(|&c| c >= min_quota));
            assert!(per_ring.iter().any(|&c| c > min_quota));
        }
    }

    #[test]
    fn grid_and_3d_loading_hand_off_smoothly_in_both_directions() {
        let grid12: Vec<Item> = (0..12)
            .map(|i| Item {
                label: format!("Net {i}"),
                icon: Icon::Folder,
                pixbuf: None,
                active: false,
            })
            .collect();

        // Start settled in Grid View.
        let mut motion = Motion::default();
        motion.set_box(true);
        motion.update(grid12.clone(), Some(0), false);
        assert_eq!(motion.box_mix(), 1.0);
        assert!(motion.loading_pose().is_none());

        // Grid View -> 3D Loading: box collapses first while loading waits flat.
        motion.set_box(false);
        assert!(motion.set_loading(true, true));
        assert!(motion.update(vec![], None, true));
        assert!(motion.loading_pose().is_none());
        assert!(motion.sectors().all(|s| s.slot.born_in_box()));

        let mut saw_collapse = false;
        let mut handoff_pose = None;
        for _ in 0..90 {
            assert!(motion.advance(1.0 / 60.0));
            if let Some(pose) = motion.loading_pose() {
                assert_eq!(motion.box_mix(), 0.0);
                handoff_pose = Some(pose);
                break;
            }
            assert!(motion.box_mix() > 0.003);
            saw_collapse = true;
        }
        assert!(saw_collapse, "grid box must collapse before 3D spin begins");
        let first_pose = handoff_pose.expect("3D loading should engage once flat");
        assert!(
            first_pose.depth < 0.25 && first_pose.rotation[2][2] > 0.99,
            "3D loading must start seamlessly from the flat ring"
        );

        // Spin up to full 3D depth and tumble.
        for _ in 0..60 {
            assert!(motion.advance(1.0 / 60.0));
        }
        assert!(motion.loading_pose().unwrap().depth > 0.95);

        // 3D Loading -> Grid View: 3D ring settles flat before grid box expands.
        motion.set_box(true);
        assert!(motion.set_loading(false, true));
        assert!(motion.update(grid12.clone(), Some(0), true));
        assert!(motion.sectors().all(|s| s.slot.born_in_box()));

        let mut saw_3d_settle = false;
        let mut saw_box_expand = false;
        let mut prev_box = 0.0;
        for _ in 0..120 {
            let moving = motion.advance(1.0 / 60.0);
            if motion.loading_pose().is_some() {
                assert_eq!(
                    motion.box_mix(),
                    0.0,
                    "grid box must wait until 3D ring finishes settling flat"
                );
                saw_3d_settle = true;
            } else {
                let cur = motion.box_mix();
                assert!(cur >= prev_box - 1e-9, "grid expansion must be monotonic");
                if cur > 0.0 {
                    saw_box_expand = true;
                }
                prev_box = cur;
            }
            if !moving {
                break;
            }
        }
        assert!(saw_3d_settle, "3D ring must settle flat before grid expands");
        assert!(saw_box_expand, "grid box must expand after 3D ring settles");
        assert_eq!(motion.box_mix(), 1.0);
        assert!(motion.loading_pose().is_none());

        // Mid-collapse cancellation reverses smoothly back to grid without entering 3D.
        motion.set_box(false);
        assert!(motion.set_loading(true, true));
        motion.update(vec![], None, true);
        motion.advance(0.05);
        let mid = motion.box_mix();
        assert!((0.1..0.95).contains(&mid));
        assert!(motion.loading_pose().is_none());

        motion.set_box(true);
        assert!(!motion.set_loading(false, true));
        motion.update(grid12, Some(0), true);
        assert_eq!(motion.box_mix(), mid);
        settle(&mut motion);
        assert_eq!(motion.box_mix(), 1.0);
        assert!(motion.loading_pose().is_none());
    }
}
