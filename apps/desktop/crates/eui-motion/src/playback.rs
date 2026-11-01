//! Playback (EXP-1604): elapsed time → composition frame, the ONE clock every
//! renderer uses (WASM for the web, uniffi for SwiftUI/Compose, native in
//! gpui), so the same props show the same frame everywhere. Pinned by
//! `fixtures/motion-playback.json`.

use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Forward,
    Reverse,
    /// Forward, then backward (each leg counts as one play).
    Alternate,
}

/// The playback props of a Motion node, resolved.
#[derive(Clone, Debug)]
pub struct Playback {
    pub autoplay: bool,
    /// `true` = forever, `false` = once, a number = that many plays.
    pub r#loop: LoopCount,
    pub direction: Direction,
    pub speed: f64,
    /// `[from, to]` in ms of the composition, or a marker name.
    pub segment: Option<Segment>,
    /// The still frame (ms) shown before playing, when paused off-screen,
    /// and under reduced motion. `None` = the segment's first frame.
    pub poster: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoopCount {
    Flag(bool),
    Count(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Range([f64; 2]),
    Marker(String),
}

impl Default for Playback {
    fn default() -> Self {
        Self { autoplay: true, r#loop: LoopCount::Flag(true), direction: Direction::Forward, speed: 1.0, segment: None, poster: None }
    }
}

impl Playback {
    /// From the Motion node's props (JSON): `autoplay`, `loop`, `plays`,
    /// `direction`, `speed`, `segment` ([from, to] ms), `marker`, `poster` (ms).
    /// Unknown or malformed values fall back to the defaults.
    pub fn from_json(v: &Value) -> Self {
        let d = Self::default();
        let num = |k: &str| v.get(k).and_then(Value::as_f64);
        let r#loop = match (num("plays"), v.get("loop")) {
            (Some(n), _) => LoopCount::Count(n),
            (None, Some(Value::Bool(b))) => LoopCount::Flag(*b),
            (None, Some(Value::Number(n))) => LoopCount::Count(n.as_f64().unwrap_or(1.0)),
            _ => d.r#loop,
        };
        let segment = match (v.get("segment").and_then(Value::as_array), v.get("marker").and_then(Value::as_str)) {
            (Some(a), _) if a.len() == 2 => Some(Segment::Range([a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0)])),
            (_, Some(m)) => Some(Segment::Marker(m.to_string())),
            _ => None,
        };
        Self {
            autoplay: v.get("autoplay").and_then(Value::as_bool).unwrap_or(d.autoplay),
            r#loop,
            direction: match v.get("direction").and_then(Value::as_str) {
                Some("reverse") => Direction::Reverse,
                Some("alternate") => Direction::Alternate,
                _ => Direction::Forward,
            },
            speed: num("speed").unwrap_or(d.speed),
            segment,
            poster: num("poster"),
        }
    }
}

/// Where the clock is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tick {
    /// The composition frame to sample.
    pub frame: f64,
    /// True once a finite loop count has played out (the frame then holds).
    pub done: bool,
}

/// The composition facts playback needs.
#[derive(Clone, Debug)]
pub struct Timeline {
    pub frame_rate: f64,
    pub in_point: f64,
    pub out_point: f64,
    /// (name, start frame, duration frames).
    pub markers: Vec<(String, f64, f64)>,
}

impl Timeline {
    /// The playing range `[a, b)` in frames.
    pub fn range(&self, segment: Option<&Segment>) -> (f64, f64) {
        let (ip, op) = (self.in_point, self.out_point);
        let r = match segment {
            Some(Segment::Range([from, to])) => (ip + from / 1000.0 * self.frame_rate, ip + to / 1000.0 * self.frame_rate),
            Some(Segment::Marker(name)) => match self.markers.iter().find(|m| &m.0 == name) {
                Some((_, t, d)) => (*t, t + d.max(1.0)),
                None => (ip, op),
            },
            None => (ip, op),
        };
        let a = r.0.clamp(ip, op);
        let b = r.1.clamp(ip, op);
        if b > a {
            (a, b)
        } else {
            (ip, op)
        }
    }

    /// The last frame strictly inside `[a, b)` (one frame before `b`).
    fn last(a: f64, b: f64) -> f64 {
        (b - 1.0).max(a)
    }

    /// The frame `elapsed_ms` after play started.
    pub fn tick(&self, p: &Playback, elapsed_ms: f64) -> Tick {
        let (a, b) = self.range(p.segment.as_ref());
        let len = b - a;
        if !p.autoplay || len <= 0.0 || self.frame_rate <= 0.0 {
            return Tick { frame: self.poster(p), done: false };
        }
        let speed = if p.speed.is_finite() && p.speed > 0.0 { p.speed } else { 1.0 };
        let t = elapsed_ms.max(0.0) / 1000.0 * self.frame_rate * speed;
        let plays = match p.r#loop {
            LoopCount::Flag(true) => f64::INFINITY,
            LoopCount::Flag(false) => 1.0,
            LoopCount::Count(n) if n >= 1.0 => n.floor(),
            LoopCount::Count(_) => 1.0,
        };
        let leg = (t / len).floor();
        let done = leg >= plays;
        let (leg, within) = if done { (plays - 1.0, len) } else { (leg, t - leg * len) };
        let backwards = match p.direction {
            Direction::Forward => false,
            Direction::Reverse => true,
            Direction::Alternate => leg as i64 % 2 == 1,
        };
        let frame = if done {
            if backwards {
                a
            } else {
                Self::last(a, b)
            }
        } else if backwards {
            (b - within).min(Self::last(a, b)).max(a)
        } else {
            a + within
        };
        Tick { frame, done }
    }

    /// The frame a bound `progress` (0–1) shows within the segment.
    pub fn at_progress(&self, p: &Playback, progress: f64) -> f64 {
        let (a, b) = self.range(p.segment.as_ref());
        a + (b - a) * progress.clamp(0.0, 1.0).min(1.0 - 1e-9)
    }

    /// The poster frame (reduced motion, paused, before play).
    pub fn poster(&self, p: &Playback) -> f64 {
        let (a, b) = self.range(p.segment.as_ref());
        match p.poster {
            Some(ms) => (self.in_point + ms / 1000.0 * self.frame_rate).clamp(a, Self::last(a, b)),
            None => a,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tl() -> Timeline {
        Timeline { frame_rate: 30.0, in_point: 0.0, out_point: 30.0, markers: vec![("pop".into(), 10.0, 5.0)] }
    }

    #[test]
    fn loops_forward() {
        let p = Playback::default();
        assert_eq!(tl().tick(&p, 500.0).frame, 15.0);
        assert_eq!(tl().tick(&p, 1500.0).frame, 15.0);
    }

    #[test]
    fn plays_once_and_holds_the_last_frame() {
        let p = Playback { r#loop: LoopCount::Flag(false), ..Default::default() };
        assert_eq!(tl().tick(&p, 2000.0), Tick { frame: 29.0, done: true });
    }

    #[test]
    fn alternates_and_reverses() {
        let p = Playback { direction: Direction::Alternate, ..Default::default() };
        assert_eq!(tl().tick(&p, 1500.0).frame, 15.0);
        assert_eq!(tl().tick(&p, 1250.0).frame, 22.5);
        let r = Playback { direction: Direction::Reverse, ..Default::default() };
        assert_eq!(tl().tick(&r, 0.0).frame, 29.0);
    }

    #[test]
    fn segments_by_marker_and_progress() {
        let p = Playback { segment: Some(Segment::Marker("pop".into())), ..Default::default() };
        assert_eq!(tl().range(p.segment.as_ref()), (10.0, 15.0));
        assert_eq!(tl().at_progress(&p, 0.5), 12.5);
    }
}
