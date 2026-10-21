//! Frame-clock motion with one shared scheduler, cancellation and accessibility overrides.
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::LazyLock,
    time::Duration,
};
pub static DURATIONS: LazyLock<BTreeMap<String, u32>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("durations.json")).expect("embedded durations")
});
pub static EASING_POINTS: LazyLock<BTreeMap<String, [f64; 4]>> =
    LazyLock::new(|| serde_json::from_str(include_str!("easings.json")).expect("embedded easings"));
#[derive(Clone, Debug)]
pub struct MotionError(pub &'static str);
impl std::fmt::Display for MotionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for MotionError {}
#[derive(Clone, Copy, Debug)]
pub struct Curve {
    points: [f64; 4],
}
pub fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Curve, MotionError> {
    if ![x1, y1, x2, y2].iter().all(|v| v.is_finite()) {
        return Err(MotionError("motion values must be finite"));
    }
    if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
        return Err(MotionError("CSS bezier x controls must be in [0, 1]"));
    }
    Ok(Curve {
        points: [x1, y1, x2, y2],
    })
}
impl Curve {
    pub fn sample(self, t: f64) -> f64 {
        let [x1, y1, x2, y2] = self.points;
        let t = t.clamp(0.0, 1.0);
        if t == 0.0 || t == 1.0 || (x1 == y1 && x2 == y2) {
            return t;
        }
        fn sample(s: f64, a: f64, b: f64) -> f64 {
            ((1.0 - 3.0 * b + 3.0 * a) * s + 3.0 * b - 6.0 * a) * s * s + 3.0 * a * s
        }
        let mut s = t;
        for _ in 0..8 {
            let error = sample(s, x1, x2) - t;
            if error.abs() < 1e-9 {
                return sample(s, y1, y2);
            }
            let slope =
                (3.0 * (1.0 - 3.0 * x2 + 3.0 * x1) * s + 6.0 * x2 - 12.0 * x1) * s + 3.0 * x1;
            if slope.abs() < 1e-7 {
                break;
            }
            let candidate = s - error / slope;
            if !(0.0..=1.0).contains(&candidate) {
                break;
            }
            s = candidate;
        }
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..40 {
            s = (low + high) / 2.0;
            let x = sample(s, x1, x2);
            if (x - t).abs() < 1e-10 {
                break;
            }
            if x < t {
                low = s
            } else {
                high = s
            }
        }
        sample(s, y1, y2)
    }
    pub fn evaluate(self, t: f64) -> Result<f64, MotionError> {
        if !t.is_finite() {
            Err(MotionError("motion values must be finite"))
        } else {
            Ok(self.sample(t))
        }
    }
}
pub fn easing(name: &str) -> Result<Curve, MotionError> {
    let p = EASING_POINTS
        .get(name)
        .ok_or(MotionError("Unknown easing"))?;
    cubic_bezier(p[0], p[1], p[2], p[3])
}
#[derive(Clone, Copy, Debug)]
pub enum Milliseconds<'a> {
    Token(&'a str),
    Number(f64),
}
impl<'a> From<&'a str> for Milliseconds<'a> {
    fn from(s: &'a str) -> Self {
        Self::Token(s)
    }
}
impl From<f64> for Milliseconds<'_> {
    fn from(n: f64) -> Self {
        Self::Number(n)
    }
}
impl From<i32> for Milliseconds<'_> {
    fn from(n: i32) -> Self {
        Self::Number(n as f64)
    }
}
impl From<u32> for Milliseconds<'_> {
    fn from(n: u32) -> Self {
        Self::Number(n as f64)
    }
}
pub fn duration_ms<'a>(value: impl Into<Milliseconds<'a>>) -> Result<f64, MotionError> {
    let value = match value.into() {
        Milliseconds::Token(s) => *DURATIONS
            .get(s)
            .ok_or(MotionError("Unknown duration token"))? as f64,
        Milliseconds::Number(n) => n,
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(MotionError("motion values must be finite"))
    }
}
thread_local! {static REDUCE:Cell<bool>=const{Cell::new(false)};static NEXT:Cell<u64>=const{Cell::new(1)};static ENGINE:RefCell<Engine>=RefCell::new(Engine::default());}
pub fn set_reduce_motion(flag: bool) {
    REDUCE.set(flag);
}
pub fn animations_enabled() -> bool {
    !REDUCE.get()
        && std::env::var(impro_core::identity::REDUCE_MOTION_ENV).as_deref() != Ok("1")
        && gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_animations())
}
fn next_id() -> u64 {
    NEXT.with(|n| {
        let v = n.get();
        n.set(v.wrapping_add(1));
        v
    })
}
/// Independent keys allow opacity, size, and custom motion on one widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionKey {
    Default,
    Opacity,
    Width,
    Height,
    Custom(u64),
}
impl MotionKey {
    pub fn unique() -> Self {
        Self::Custom(next_id())
    }
}
pub struct AnimateOptions {
    pub easing: Curve,
    pub key: MotionKey,
    pub delay_ms: f64,
    pub on_done: Option<Box<dyn FnOnce()>>,
}
impl Default for AnimateOptions {
    fn default() -> Self {
        Self {
            easing: easing("smooth").expect("built-in easing"),
            key: MotionKey::Default,
            delay_ms: 0.0,
            on_done: None,
        }
    }
}
struct Inner {
    widget: glib::WeakRef<gtk::Widget>,
    key: MotionKey,
    duration: f64,
    delay: f64,
    curve: Curve,
    update: Rc<dyn Fn(f64)>,
    done: RefCell<Option<Box<dyn FnOnce()>>>,
    finished: Cell<bool>,
    origin: Cell<Option<i64>>,
    last_time: Cell<i64>,
    signals: RefCell<Vec<glib::SignalHandlerId>>,
}
/// Dropping a handle leaves the animation running; cancel explicitly to stop it.
#[derive(Clone)]
pub struct Tween(Rc<Inner>);
impl Tween {
    pub fn finished(&self) -> bool {
        self.0.finished.get()
    }
    pub fn cancel(&self) {
        self.0.cancel();
        stop_if_idle();
    }
}
impl Inner {
    fn cancel(&self) {
        if self.finished.replace(true) {
            return;
        }
        let signals = std::mem::take(&mut *self.signals.borrow_mut());
        if let Some(widget) = self.widget.upgrade() {
            for signal in signals {
                widget.disconnect(signal);
            }
        }
        self.done.borrow_mut().take();
    }
    fn complete(&self) {
        let done = self.done.borrow_mut().take();
        self.cancel();
        if let Some(done) = done {
            done();
        }
    }
    fn advance(&self, now: i64) {
        if self.finished.get() {
            return;
        }
        if self.widget.upgrade().is_none() {
            self.cancel();
            return;
        }
        let now = now.max(self.last_time.get());
        self.last_time.set(now);
        let origin = self.origin.get().unwrap_or_else(|| {
            self.origin.set(Some(now));
            now
        });
        let elapsed = (now - origin) as f64 - self.delay;
        if elapsed < 0.0 {
            return;
        }
        let p = (elapsed / self.duration).clamp(0.0, 1.0);
        (self.update)(if p == 1.0 { 1.0 } else { self.curve.sample(p) });
        if p == 1.0 && !self.finished.get() {
            self.complete();
        }
    }
}
#[derive(Default)]
struct Engine {
    active: Vec<Rc<Inner>>,
    tick: Option<gtk::TickCallbackId>,
    host: glib::WeakRef<gtk::Widget>,
    timer: Option<glib::SourceId>,
    pumping: bool,
    current: Option<Rc<Inner>>,
}
fn stop_if_idle() {
    let idle = ENGINE.with(|e| {
        let e = e.borrow();
        !e.pumping && e.active.iter().all(|i| i.finished.get())
    });
    if idle {
        let (tick, timer) = ENGINE.with(|e| {
            let mut e = e.borrow_mut();
            e.active.clear();
            (e.tick.take(), e.timer.take())
        });
        if let Some(tick) = tick {
            tick.remove();
        }
        if let Some(timer) = timer {
            timer.remove();
        }
    }
}
fn pump(now: i64) -> bool {
    // Move the vector out: no borrows span signal-emitting updates, no frame allocations.
    let mut active = ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.pumping = true;
        std::mem::take(&mut e.active)
    });
    for inner in &active {
        ENGINE.with(|e| e.borrow_mut().current = Some(inner.clone()));
        inner.advance(now);
        ENGINE.with(|e| {
            e.borrow_mut().current.take();
        });
    }
    active.retain(|i| !i.finished.get());
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        active.append(&mut e.active);
        e.active = active;
        e.pumping = false;
        !e.active.is_empty()
    })
}
fn schedule() {
    let host = ENGINE.with(|e| {
        let e = e.borrow();
        if e.tick.is_some() || e.timer.is_some() || e.pumping {
            return None;
        }
        e.active
            .iter()
            .find(|i| !i.finished.get())
            .and_then(|i| i.widget.upgrade())
            .map(|w| w.toplevel().unwrap_or(w))
    });
    let Some(host) = host else { return };
    if host.is_mapped() {
        let tick = host.add_tick_callback(|_, clock| {
            if pump(clock.frame_time()) {
                glib::ControlFlow::Continue
            } else {
                ENGINE.with(|e| {
                    e.borrow_mut().tick.take();
                });
                glib::ControlFlow::Break
            }
        });
        ENGINE.with(|e| {
            let mut e = e.borrow_mut();
            e.host = host.downgrade();
            e.tick = Some(tick);
        });
    } else {
        let timer = glib::timeout_add_local(Duration::from_millis(16), || {
            if !pump(glib::monotonic_time()) {
                ENGINE.with(|e| {
                    e.borrow_mut().timer.take();
                });
                return glib::ControlFlow::Break;
            }
            let mapped = ENGINE.with(|e| {
                e.borrow().active.iter().any(|i| {
                    i.widget
                        .upgrade()
                        .is_some_and(|w| w.toplevel().unwrap_or(w).is_mapped())
                })
            });
            if mapped {
                ENGINE.with(|e| {
                    e.borrow_mut().timer.take();
                });
                schedule();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        ENGINE.with(|e| e.borrow_mut().timer = Some(timer));
    }
}
/// One shared clock across all widgets. Replacement is registered before the first update.
pub fn animate<'a>(
    widget: &impl IsA<gtk::Widget>,
    duration: impl Into<Milliseconds<'a>>,
    update: impl Fn(f64) + 'static,
    options: AnimateOptions,
) -> Tween {
    let duration = duration_ms(duration).expect("finite duration");
    assert!(options.delay_ms.is_finite(), "finite delay");
    let widget = widget.as_ref();
    let previous: Vec<_> = ENGINE.with(|e| {
        let e = e.borrow();
        e.active
            .iter()
            .chain(e.current.iter())
            .filter(|i| i.key == options.key && i.widget.upgrade().as_ref() == Some(widget))
            .cloned()
            .collect()
    });
    for i in previous {
        i.cancel();
    }
    let inner = Rc::new(Inner {
        widget: widget.downgrade(),
        key: options.key,
        duration: duration * 1000.0,
        delay: options.delay_ms.max(0.0) * 1000.0,
        curve: options.easing,
        update: Rc::new(update),
        done: RefCell::new(options.on_done),
        finished: Cell::new(false),
        origin: Cell::new(None),
        last_time: Cell::new(0),
        signals: RefCell::new(Vec::new()),
    });
    ENGINE.with(|e| e.borrow_mut().active.push(inner.clone()));
    for name in ["destroy", "unrealize", "unmap"] {
        let weak = Rc::downgrade(&inner);
        let signal = widget.connect_local(name, false, move |_| {
            if let Some(i) = weak.upgrade() {
                i.cancel();
                stop_if_idle();
            }
            None
        });
        inner.signals.borrow_mut().push(signal);
    }
    if duration <= 0.0 || !animations_enabled() {
        (inner.update)(1.0);
        if !inner.finished.get() {
            inner.complete();
        }
        stop_if_idle();
    } else {
        (inner.update)(0.0);
        if !inner.finished.get() {
            schedule();
        }
    }
    Tween(inner)
}
pub fn active_count() -> usize {
    ENGINE.with(|e| {
        e.borrow()
            .active
            .iter()
            .filter(|i| !i.finished.get())
            .count()
    })
}
pub fn clock_attached() -> bool {
    ENGINE.with(|e| {
        let e = e.borrow();
        e.tick.is_some() || e.timer.is_some()
    })
}
// Baselines survive replacement so an interrupted slide/resize restores its original value.
thread_local! {static BASELINES:RefCell<Vec<(glib::WeakRef<gtk::Widget>,MotionKey,i32)>>=const{RefCell::new(Vec::new())};}
fn baseline(widget: &gtk::Widget, key: MotionKey, current: i32) -> i32 {
    BASELINES.with(|b| {
        let mut b = b.borrow_mut();
        b.retain(|(w, _, _)| w.upgrade().is_some());
        if let Some((_, _, v)) = b
            .iter()
            .find(|(w, k, _)| *k == key && w.upgrade().as_ref() == Some(widget))
        {
            *v
        } else {
            b.push((widget.downgrade(), key, current));
            current
        }
    })
}
fn take_baseline(widget: &gtk::Widget, key: MotionKey) -> Option<i32> {
    BASELINES.with(|b| {
        let mut b = b.borrow_mut();
        b.iter()
            .position(|(w, k, _)| *k == key && w.upgrade().as_ref() == Some(widget))
            .map(|i| b.swap_remove(i).2)
    })
}
pub fn fade<'a>(
    widget: &impl IsA<gtk::Widget>,
    show: bool,
    duration: impl Into<Milliseconds<'a>>,
    hide_when_done: bool,
    on_done: Option<Box<dyn FnOnce()>>,
) -> Tween {
    let widget = widget.as_ref();
    if show && !widget.is_visible() {
        if widget.opacity() == 1.0 {
            widget.set_opacity(0.0);
        }
        widget.show();
    }
    let start = widget.opacity();
    let margin = widget.margin_top();
    let original = BASELINES.with(|b| {
        b.borrow()
            .iter()
            .find(|(w, k, _)| *k == MotionKey::Opacity && w.upgrade().as_ref() == Some(widget))
            .map(|(_, _, v)| *v)
    });
    let weak = widget.downgrade();
    let done_weak = weak.clone();
    animate(
        widget,
        duration,
        move |p| {
            if let Some(w) = weak.upgrade() {
                w.set_opacity((start + (f64::from(show) - start) * p).clamp(0.0, 1.0));
                if let Some(original) = original {
                    w.set_margin_top(
                        (margin as f64 + (original - margin) as f64 * p)
                            .round_ties_even()
                            .max(0.0) as i32,
                    );
                }
            }
        },
        AnimateOptions {
            key: MotionKey::Opacity,
            on_done: Some(Box::new(move || {
                if let Some(w) = done_weak.upgrade() {
                    if let Some(original) = take_baseline(&w, MotionKey::Opacity) {
                        w.set_margin_top(original);
                    }
                    if !show && hide_when_done {
                        w.hide();
                    }
                }
                if let Some(done) = on_done {
                    done();
                }
            })),
            ..Default::default()
        },
    )
}
pub fn slide_in<'a>(
    widget: &impl IsA<gtk::Widget>,
    offset: i32,
    duration: impl Into<Milliseconds<'a>>,
    on_done: Option<Box<dyn FnOnce()>>,
) -> Tween {
    let widget = widget.as_ref();
    let original = baseline(widget, MotionKey::Opacity, widget.margin_top());
    let margin = widget.margin_top();
    if margin == original {
        widget.set_margin_top((original + offset).max(0));
    }
    if !widget.is_visible() {
        if widget.opacity() == 1.0 {
            widget.set_opacity(0.0);
        }
        widget.show();
    }
    let margin = widget.margin_top();
    let opacity = widget.opacity();
    let weak = widget.downgrade();
    let done_weak = weak.clone();
    animate(
        widget,
        duration,
        move |p| {
            if let Some(w) = weak.upgrade() {
                w.set_opacity((opacity + (1.0 - opacity) * p).clamp(0.0, 1.0));
                w.set_margin_top(
                    (margin as f64 + (original - margin) as f64 * p)
                        .round_ties_even()
                        .max(0.0) as i32,
                );
            }
        },
        AnimateOptions {
            key: MotionKey::Opacity,
            easing: easing("ease_in_out").expect("built-in easing"),
            on_done: Some(Box::new(move || {
                if let Some(w) = done_weak.upgrade() {
                    if let Some(original) = take_baseline(&w, MotionKey::Opacity) {
                        w.set_margin_top(original);
                    }
                }
                if let Some(done) = on_done {
                    done();
                }
            })),
            ..Default::default()
        },
    )
}
#[derive(Clone, Copy)]
pub enum Axis {
    Width,
    Height,
}
pub fn animate_size_request<'a>(
    widget: &impl IsA<gtk::Widget>,
    axis: Axis,
    to: i32,
    duration: impl Into<Milliseconds<'a>>,
    restore: bool,
    on_done: Option<Box<dyn FnOnce()>>,
) -> Result<Tween, MotionError> {
    if to < 0 {
        return Err(MotionError("target size must be nonnegative"));
    }
    let widget = widget.as_ref();
    let (key, current, allocated) = match axis {
        Axis::Width => (
            MotionKey::Width,
            widget.width_request(),
            widget.allocated_width(),
        ),
        Axis::Height => (
            MotionKey::Height,
            widget.height_request(),
            widget.allocated_height(),
        ),
    };
    baseline(widget, key, current);
    let start = if current < 0 { allocated } else { current };
    let weak = widget.downgrade();
    let done_weak = weak.clone();
    Ok(animate(
        widget,
        duration,
        move |p| {
            if let Some(w) = weak.upgrade() {
                let value = (start as f64 + (to - start) as f64 * p)
                    .round_ties_even()
                    .max(0.0) as i32;
                match axis {
                    Axis::Width => w.set_width_request(value),
                    Axis::Height => w.set_height_request(value),
                }
            }
        },
        AnimateOptions {
            key,
            on_done: Some(Box::new(move || {
                if let Some(w) = done_weak.upgrade() {
                    if let Some(original) = take_baseline(&w, key) {
                        if restore {
                            match axis {
                                Axis::Width => w.set_width_request(original),
                                Axis::Height => w.set_height_request(original),
                            }
                        }
                    }
                }
                if let Some(done) = on_done {
                    done();
                }
            })),
            ..Default::default()
        },
    ))
}
fn color(value: &str) -> Result<[f64; 4], MotionError> {
    let value = value.trim();
    if let Some(digits) = value.strip_prefix('#') {
        let digits = if digits.len() == 3 || digits.len() == 4 {
            digits.chars().flat_map(|c| [c, c]).collect::<String>()
        } else {
            digits.into()
        };
        if [6, 8].contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            let mut rgba = [1.0; 4];
            for (i, chunk) in digits.as_bytes().chunks(2).enumerate() {
                let s =
                    std::str::from_utf8(chunk).map_err(|_| MotionError("unsupported colour"))?;
                rgba[i] = u8::from_str_radix(s, 16)
                    .map_err(|_| MotionError("unsupported colour"))?
                    as f64
                    / 255.0;
            }
            return Ok(rgba);
        }
    }
    let lower = value.to_ascii_lowercase();
    if let Some((kind, body)) = lower.split_once('(') {
        if let Some(body) = body.strip_suffix(')') {
            let parts: Vec<_> = body.split(',').collect();
            if (kind == "rgb" && parts.len() == 3) || (kind == "rgba" && parts.len() == 4) {
                let mut rgba = [1.0; 4];
                for (i, p) in parts.iter().enumerate() {
                    let p = p.trim();
                    let (value, divisor) = if let Some(p) = p.strip_suffix('%') {
                        (p, 100.0)
                    } else {
                        (p, if i < 3 { 255.0 } else { 1.0 })
                    };
                    let number = value
                        .parse::<f64>()
                        .map_err(|_| MotionError("unsupported colour"))?;
                    if !number.is_finite() {
                        return Err(MotionError("motion values must be finite"));
                    }
                    rgba[i] = (number / divisor).clamp(0.0, 1.0);
                }
                return Ok(rgba);
            }
        }
    }
    Err(MotionError("unsupported colour"))
}
pub fn lerp_color(a: &str, b: &str, t: f64) -> Result<String, MotionError> {
    if !t.is_finite() {
        return Err(MotionError("motion values must be finite"));
    }
    let (a, b) = (color(a)?, color(b)?);
    let c: [f64; 4] = std::array::from_fn(|i| (a[i] + (b[i] - a[i]) * t).clamp(0.0, 1.0));
    let alpha = format!("{:.6}", c[3])
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string();
    Ok(format!(
        "rgba({}, {}, {}, {})",
        (c[0] * 255.0).round_ties_even() as u8,
        (c[1] * 255.0).round_ties_even() as u8,
        (c[2] * 255.0).round_ties_even() as u8,
        alpha
    ))
}
struct FaderInner {
    widget: glib::WeakRef<gtk::Widget>,
    update: Rc<dyn Fn(f64)>,
    value: Cell<f64>,
    duration: f64,
    curve: Curve,
    key: MotionKey,
    signals: RefCell<Vec<glib::SignalHandlerId>>,
    disconnected: Cell<bool>,
}
#[derive(Clone)]
pub struct StateFader(Rc<FaderInner>);
impl StateFader {
    pub fn new(
        widget: &impl IsA<gtk::Widget>,
        update: impl Fn(f64) + 'static,
        duration: f64,
        initial: f64,
    ) -> Self {
        let widget = widget.as_ref();
        let inner = Rc::new(FaderInner {
            widget: widget.downgrade(),
            update: Rc::new(update),
            value: Cell::new(initial.clamp(0.0, 1.0)),
            duration,
            curve: easing("smooth").expect("built-in easing"),
            key: MotionKey::unique(),
            signals: RefCell::new(Vec::new()),
            disconnected: Cell::new(false),
        });
        let this = Self(inner);
        widget.add_events(gdk::EventMask::ENTER_NOTIFY_MASK | gdk::EventMask::LEAVE_NOTIFY_MASK);
        // Signals own the fader; fader holds only a weak widget, avoiding a reference cycle.
        let f = this.clone();
        let a = widget.connect_enter_notify_event(move |_, e| {
            if e.detail() != gdk::NotifyType::Inferior {
                f.set(1.0);
            }
            glib::Propagation::Proceed
        });
        let f = this.clone();
        let b = widget.connect_leave_notify_event(move |_, e| {
            if e.detail() != gdk::NotifyType::Inferior {
                f.set(0.0);
            }
            glib::Propagation::Proceed
        });
        let f = this.clone();
        let c = widget.connect_destroy(move |_| f.disconnect());
        this.0.signals.borrow_mut().extend([a, b, c]);
        (this.0.update)(this.value());
        this
    }
    pub fn value(&self) -> f64 {
        self.0.value.get()
    }
    pub fn set(&self, target: f64) -> Option<Tween> {
        if self.0.disconnected.get() {
            return None;
        }
        let widget = self.0.widget.upgrade()?;
        let start = self.value();
        let target = target.clamp(0.0, 1.0);
        let inner = self.0.clone();
        Some(animate(
            &widget,
            self.0.duration,
            move |p| {
                let value = (start + (target - start) * p).clamp(0.0, 1.0);
                inner.value.set(value);
                (inner.update)(value);
            },
            AnimateOptions {
                key: self.0.key,
                easing: self.0.curve,
                ..Default::default()
            },
        ))
    }
    pub fn cancel(&self) {
        let items: Vec<_> = ENGINE.with(|e| {
            e.borrow()
                .active
                .iter()
                .filter(|i| i.key == self.0.key)
                .cloned()
                .collect()
        });
        for i in items {
            i.cancel();
        }
        stop_if_idle();
    }
    pub fn disconnect(&self) {
        if self.0.disconnected.replace(true) {
            return;
        }
        self.cancel();
        let signals = std::mem::take(&mut *self.0.signals.borrow_mut());
        if let Some(widget) = self.0.widget.upgrade() {
            for signal in signals {
                widget.disconnect(signal);
            }
        }
    }
}
