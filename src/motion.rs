//! Emil Kowalski Motion & Physics Engine (emil-design-eng + review-animations/STANDARDS.md)
//! - Never ease-in on UI; strong custom cubic-bezier curves only
//! - Interruptible springs that preserve velocity on direction reversal
//! - Asymmetric enter/exit timing (exits always faster than enters)
//! - Zero animation on 100+/day keyboard actions (e.g. Alt+Space instant open)
//! - Instant subsequent tooltips after the first hover in a toolbar/dock group

use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl CubicBezier {
    /// Emil Kowalski strong ease-out for UI interactions: cubic-bezier(0.23, 1, 0.32, 1)
    pub const EASE_OUT: Self = Self { x1: 0.23, y1: 1.0, x2: 0.32, y2: 1.0 };
    /// Strong ease-in-out for on-screen morphing / pill sliding: cubic-bezier(0.77, 0, 0.175, 1)
    pub const EASE_IN_OUT: Self = Self { x1: 0.77, y1: 0.0, x2: 0.175, y2: 1.0 };
    /// iOS/macOS drawer curve: cubic-bezier(0.32, 0.72, 0, 1)
    pub const EASE_DRAWER: Self = Self { x1: 0.32, y1: 0.72, x2: 0.0, y2: 1.0 };

    pub fn eval(&self, t: f32) -> f32 {
        let x = t.clamp(0.0, 1.0);
        if x <= 0.0 { return 0.0; }
        if x >= 1.0 { return 1.0; }
        // Solve bezier_x(u) = x via Newton-Raphson (5 fast iterations)
        let mut u = x;
        for _ in 0..5 {
            let bx = sample_curve(self.x1, self.x2, u) - x;
            let dx = sample_curve_deriv(self.x1, self.x2, u);
            if dx.abs() < 1e-6 { break; }
            u = (u - bx / dx).clamp(0.0, 1.0);
        }
        sample_curve(self.y1, self.y2, u)
    }
}

#[inline]
fn sample_curve(p1: f32, p2: f32, t: f32) -> f32 {
    let inv = 1.0 - t;
    3.0 * inv * inv * t * p1 + 3.0 * inv * t * t * p2 + t * t * t
}

#[inline]
fn sample_curve_deriv(p1: f32, p2: f32, t: f32) -> f32 {
    let inv = 1.0 - t;
    3.0 * inv * inv * p1 + 6.0 * inv * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
}

/// Interruptible physical spring (preserves velocity when target changes mid-flight)
#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    pub stiffness: f32,
    pub damping: f32,
}

impl Spring {
    pub fn new(initial: f32, stiffness: f32, damping: f32) -> Self {
        Self { value: initial, velocity: 0.0, target: initial, stiffness, damping }
    }

    /// Snappy tactile UI spring (subtle settle, zero excessive bounce)
    pub fn ui_snappy(initial: f32) -> Self {
        Self::new(initial, 420.0, 34.0)
    }

    /// Dock magnification & hover spring (responsive, organic feel)
    pub fn dock_hover(initial: f32) -> Self {
        Self::new(initial, 360.0, 28.0)
    }

    /// Smooth drawer/dock reveal spring
    pub fn drawer(initial: f32) -> Self {
        Self::new(initial, 290.0, 30.0)
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    pub fn snap(&mut self, value: f32) {
        self.value = value;
        self.target = value;
        self.velocity = 0.0;
    }

    /// Advance spring by dt seconds (returns true while still moving)
    pub fn step(&mut self, dt_sec: f32) -> bool {
        let dt = dt_sec.clamp(0.001, 0.033);
        let disp = self.value - self.target;
        let force = -self.stiffness * disp - self.damping * self.velocity;
        self.velocity += force * dt;
        self.value += self.velocity * dt;

        if (self.value - self.target).abs() < 0.0012 && self.velocity.abs() < 0.004 {
            self.value = self.target;
            self.velocity = 0.0;
            false
        } else {
            true
        }
    }

    pub fn is_moving(&self) -> bool {
        (self.value - self.target).abs() >= 0.0012 || self.velocity.abs() >= 0.004
    }
}

/// Interruptible duration-based tween with asymmetric enter/exit support
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    pub value: f32,
    start: f32,
    target: f32,
    elapsed_ms: f32,
    duration_ms: f32,
    curve: CubicBezier,
    active: bool,
}

impl Tween {
    pub fn new(initial: f32) -> Self {
        Self {
            value: initial,
            start: initial,
            target: initial,
            elapsed_ms: 0.0,
            duration_ms: 180.0,
            curve: CubicBezier::EASE_OUT,
            active: false,
        }
    }

    /// Retargets seamlessly from current value without restarting from zero
    pub fn animate_to(&mut self, target: f32, duration_ms: f32, curve: CubicBezier) {
        if (self.target - target).abs() < 0.0005 && self.active {
            return;
        }
        if (self.value - target).abs() < 0.0005 {
            self.value = target;
            self.target = target;
            self.active = false;
            return;
        }
        if duration_ms <= 0.5 {
            self.snap(target);
            return;
        }
        self.start = self.value;
        self.target = target;
        self.elapsed_ms = 0.0;
        self.duration_ms = duration_ms;
        self.curve = curve;
        self.active = true;
    }

    pub fn snap(&mut self, val: f32) {
        self.value = val;
        self.start = val;
        self.target = val;
        self.elapsed_ms = self.duration_ms;
        self.active = false;
    }

    pub fn step(&mut self, dt_ms: f32) -> bool {
        if !self.active {
            return false;
        }
        self.elapsed_ms += dt_ms.clamp(1.0, 50.0);
        let t = (self.elapsed_ms / self.duration_ms).clamp(0.0, 1.0);
        let eased = self.curve.eval(t);
        self.value = self.start + (self.target - self.start) * eased;
        if t >= 1.0 {
            self.value = self.target;
            self.active = false;
            false
        } else {
            true
        }
    }

    pub fn is_moving(&self) -> bool {
        self.active
    }

    pub fn target(&self) -> f32 {
        self.target
    }
}

/// Emil Kowalski Tooltip Group: 380ms delay on first hover, 0ms instant when hopping adjacent items
#[derive(Debug, Clone)]
pub struct TooltipGroup {
    pub active_id: Option<String>,
    pub text: String,
    pub anchor_x: f32,
    pub anchor_y: f32,
    hover_start: Option<Instant>,
    last_shown: Option<Instant>,
    pub visible: bool,
}

impl Default for TooltipGroup {
    fn default() -> Self {
        Self {
            active_id: None,
            text: String::new(),
            anchor_x: 0.0,
            anchor_y: 0.0,
            hover_start: None,
            last_shown: None,
            visible: false,
        }
    }
}

impl TooltipGroup {
    pub fn on_hover(&mut self, id: &str, text: &str, x: f32, y: f32) {
        let now = Instant::now();
        let recently_open = self
            .last_shown
            .map(|t| now.duration_since(t).as_millis() < 450)
            .unwrap_or(false);

        if self.active_id.as_deref() != Some(id) {
            self.active_id = Some(id.to_string());
            self.text = text.to_string();
            self.anchor_x = x;
            self.anchor_y = y;
            self.hover_start = Some(now);
            if self.visible || recently_open {
                self.visible = true;
                self.last_shown = Some(now);
            }
        } else {
            self.anchor_x = x;
            self.anchor_y = y;
            if !self.visible {
                if let Some(st) = self.hover_start {
                    if now.duration_since(st).as_millis() >= 360 {
                        self.visible = true;
                        self.last_shown = Some(now);
                    }
                }
            } else {
                self.last_shown = Some(now);
            }
        }
    }

    pub fn clear(&mut self) {
        if self.visible {
            self.last_shown = Some(Instant::now());
        }
        self.active_id = None;
        self.hover_start = None;
        self.visible = false;
    }
}
