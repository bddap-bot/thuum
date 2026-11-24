use std::collections::VecDeque;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub const CHUNK: usize = 1280;
pub const PREROLL_CHUNKS: usize = 4;
pub const MAX_HOLD_CHUNKS: usize = 50;
pub const TAIL_CHUNKS: u32 = 2;
const TAIL_US: u64 = 250_000;

pub fn clock_us() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_micros() as u64 + 1
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    Press,
    Held,
    Release,
}

pub fn edge(value: f32, held_secs: f32) -> Edge {
    if value == 0.0 {
        Edge::Release
    } else if held_secs == 0.0 {
        Edge::Press
    } else {
        Edge::Held
    }
}

#[derive(Default)]
pub struct Signals {
    state: AtomicU64,
    cancelled: AtomicU64,
    released_us: AtomicU64,
}

pub static SIGNALS: Signals = Signals {
    state: AtomicU64::new(0),
    cancelled: AtomicU64::new(0),
    released_us: AtomicU64::new(0),
};

impl Signals {
    pub fn press(&self) {
        let id = (self.state.load(Ordering::Acquire) >> 1) + 1;
        self.state.store(id << 1 | 1, Ordering::Release);
    }

    pub fn release(&self, cancel: bool) {
        let id = self.state.load(Ordering::Acquire) >> 1;
        if cancel {
            self.cancelled.store(id, Ordering::Release);
        }
        self.released_us.store(clock_us(), Ordering::Release);
        self.state.store(id << 1, Ordering::Release);
    }

    pub fn observe(&self) -> (u64, u64, u64) {
        let state = self.state.load(Ordering::Acquire);
        (
            state,
            self.cancelled.load(Ordering::Acquire),
            self.released_us.load(Ordering::Acquire),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Press,
    Release,
    Cancel,
}

pub fn steps(previous: u64, current: u64, cancelled: u64) -> Vec<Step> {
    let (was, was_held) = (previous >> 1, previous & 1 == 1);
    let (id, held) = (current >> 1, current & 1 == 1);
    let end = |id| if cancelled == id { Step::Cancel } else { Step::Release };
    let mut out = vec![];
    if id != was {
        if was_held {
            out.push(end(was));
        }
        out.push(Step::Press);
        if !held {
            out.push(end(id));
        }
    } else if was_held && !held {
        out.push(end(id));
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hold {
    pub ranked: [(usize, f64); 2],
    pub chunks: u32,
    pub peak_rms: f32,
    pub released_us: u64,
    pub classify_ms: f32,
}

impl Hold {
    pub fn phrase(&self) -> usize {
        self.ranked[0].0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Closed {
    pub pcm: Vec<i16>,
    pub chunks: u32,
    pub peak_rms: f32,
    pub released_us: u64,
}

struct Window {
    chunks: VecDeque<(Option<f32>, Vec<i16>)>,
    closing: Option<(u64, u32)>,
}

impl Window {
    fn add(&mut self, rms: Option<f32>, pcm: &[i16]) {
        self.chunks.push_back((rms, pcm.to_vec()));
        if self.chunks.len() > MAX_HOLD_CHUNKS {
            self.chunks.pop_front();
        }
    }

    fn close(self) -> Option<Closed> {
        let (released_us, _) = self.closing?;
        Some(Closed {
            chunks: self.chunks.len() as u32,
            peak_rms: self.chunks.iter().filter_map(|(rms, _)| *rms).fold(0.0, f32::max),
            pcm: self.chunks.into_iter().flat_map(|(_, pcm)| pcm).collect(),
            released_us,
        })
    }
}

#[derive(Default)]
pub struct Holds {
    preroll: VecDeque<Vec<i16>>,
    window: Option<Window>,
}

impl Holds {
    pub fn chunk(&mut self, rms: f32, pcm: &[i16]) -> Option<Closed> {
        assert_eq!(pcm.len(), CHUNK);
        self.preroll.push_back(pcm.to_vec());
        if self.preroll.len() > PREROLL_CHUNKS {
            self.preroll.pop_front();
        }
        let window = self.window.as_mut()?;
        window.add(Some(rms), pcm);
        let (_, tail) = window.closing.as_mut()?;
        *tail -= 1;
        if *tail > 0 {
            return None;
        }
        self.window.take()?.close()
    }

    pub fn press(&mut self) -> Option<Closed> {
        let closed = self.window.take().and_then(Window::close);
        let mut window = Window {
            chunks: VecDeque::with_capacity(MAX_HOLD_CHUNKS + 1),
            closing: None,
        };
        for pcm in &self.preroll {
            window.add(None, pcm);
        }
        self.window = Some(window);
        closed
    }

    pub fn release(&mut self, released_us: u64) {
        if let Some(window) = &mut self.window {
            window.closing.get_or_insert((released_us, TAIL_CHUNKS));
        }
    }

    pub fn cancel(&mut self) {
        self.window = None;
    }

    pub fn overdue(&mut self, now_us: u64) -> Option<Closed> {
        let (released_us, _) = self.window.as_ref()?.closing?;
        if now_us.saturating_sub(released_us) < TAIL_US {
            return None;
        }
        self.window.take()?.close()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOUD: f32 = 500.0;

    fn chunk(v: i16) -> Vec<i16> {
        vec![v; CHUNK]
    }

    fn release(holds: &mut Holds, at: u64, tail: &[i16]) -> Option<Closed> {
        holds.release(at);
        let mut out = None;
        for (i, &v) in tail.iter().enumerate() {
            let got = holds.chunk(0.0, &chunk(v));
            assert_eq!(got.is_some(), i + 1 == TAIL_CHUNKS as usize);
            out = out.or(got);
        }
        out
    }

    fn values(closed: &Closed) -> Vec<i16> {
        closed.pcm.chunks(CHUNK).map(|c| c[0]).collect()
    }

    #[test]
    fn button_edges_follow_value_and_held_time() {
        assert_eq!(edge(1.0, 0.0), Edge::Press);
        assert_eq!(edge(1.0, 0.4), Edge::Held);
        assert_eq!(edge(0.0, 0.4), Edge::Release);
        assert_eq!(edge(0.0, 0.0), Edge::Release);
    }

    #[test]
    fn a_hold_keeps_preroll_hold_and_tail_audio_in_order() {
        let mut holds = Holds::default();
        for v in 1..=6 {
            holds.chunk(0.0, &chunk(v));
        }
        assert_eq!(holds.press(), None);
        assert_eq!(holds.chunk(LOUD, &chunk(7)), None);
        assert_eq!(holds.chunk(2.0 * LOUD, &chunk(8)), None);
        let closed = release(&mut holds, 42, &[9, 10]).unwrap();
        assert_eq!(values(&closed), vec![3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!((closed.chunks, closed.released_us), (8, 42));
        assert_eq!(closed.peak_rms, 2.0 * LOUD);
        assert_eq!(holds.chunk(LOUD, &chunk(11)), None);
    }

    #[test]
    fn a_hold_whose_tail_audio_never_arrives_is_closed_on_time() {
        let mut holds = Holds::default();
        holds.press();
        holds.chunk(LOUD, &chunk(1));
        assert_eq!(holds.overdue(1_000_000), None);
        holds.release(1_000_000);
        assert_eq!(holds.overdue(1_000_000 + TAIL_US - 1), None);
        let closed = holds.overdue(1_000_000 + TAIL_US).unwrap();
        assert_eq!((values(&closed), closed.released_us), (vec![1], 1_000_000));
        assert_eq!(holds.overdue(2_000_000), None);
    }

    #[test]
    fn the_preroll_does_not_count_toward_the_peak() {
        let mut holds = Holds::default();
        for _ in 0..PREROLL_CHUNKS {
            holds.chunk(LOUD, &chunk(5));
        }
        holds.press();
        let closed = release(&mut holds, 1, &[0, 0]).unwrap();
        assert_eq!(closed.peak_rms, 0.0);
        assert_eq!(closed.chunks as usize, PREROLL_CHUNKS + TAIL_CHUNKS as usize);
    }

    #[test]
    fn a_long_hold_keeps_its_last_seconds_and_tail() {
        let mut holds = Holds::default();
        holds.press();
        holds.chunk(LOUD, &chunk(-1));
        for v in 0..MAX_HOLD_CHUNKS as i16 + 10 {
            holds.chunk(0.0, &chunk(v));
        }
        let closed = release(&mut holds, 1, &[90, 91]).unwrap();
        assert_eq!(closed.chunks as usize, MAX_HOLD_CHUNKS);
        assert_eq!(closed.pcm.len(), MAX_HOLD_CHUNKS * CHUNK);
        assert_eq!(values(&closed)[MAX_HOLD_CHUNKS - 2..], [90, 91]);
        assert_eq!(closed.peak_rms, 0.0);
    }

    #[test]
    fn a_press_while_closing_closes_the_previous_hold() {
        let mut holds = Holds::default();
        holds.press();
        holds.chunk(LOUD, &chunk(4));
        holds.release(9);
        let previous = holds.press().unwrap();
        assert_eq!((values(&previous), previous.released_us), (vec![4], 9));
        holds.chunk(LOUD, &chunk(6));
        assert_eq!(values(&release(&mut holds, 10, &[0, 0]).unwrap()), vec![4, 6, 0, 0]);
    }

    #[test]
    fn an_unreleased_or_cancelled_hold_gives_no_decision() {
        let mut holds = Holds::default();
        holds.press();
        holds.chunk(LOUD, &chunk(2));
        assert_eq!(holds.press(), None);
        holds.chunk(LOUD, &chunk(2));
        holds.cancel();
        holds.release(9);
        assert_eq!(holds.chunk(LOUD, &chunk(2)), None);
        assert_eq!(holds.chunk(LOUD, &chunk(2)), None);
    }

    #[test]
    fn steps_recover_press_and_release_order_between_observations() {
        let held = |id: u64| id << 1 | 1;
        let up = |id: u64| id << 1;
        assert_eq!(steps(up(0), up(0), 0), vec![]);
        assert_eq!(steps(up(0), held(1), 0), vec![Step::Press]);
        assert_eq!(steps(held(1), held(1), 0), vec![]);
        assert_eq!(steps(held(1), up(1), 0), vec![Step::Release]);
        assert_eq!(steps(held(1), up(1), 1), vec![Step::Cancel]);
        assert_eq!(steps(up(1), up(1), 1), vec![]);
        assert_eq!(steps(up(0), up(1), 0), vec![Step::Press, Step::Release]);
        assert_eq!(steps(held(1), held(2), 0), vec![Step::Release, Step::Press]);
        assert_eq!(steps(held(1), up(2), 1), vec![Step::Cancel, Step::Press, Step::Release]);
        assert_eq!(steps(up(1), up(3), 0), vec![Step::Press, Step::Release]);
    }

    #[test]
    fn signals_follow_a_hold() {
        let signals = Signals::default();
        let (start, _, _) = signals.observe();
        signals.press();
        let (pressed, _, _) = signals.observe();
        assert_eq!(steps(start, pressed, 0), vec![Step::Press]);
        signals.release(false);
        let (released, cancelled, at) = signals.observe();
        assert_eq!(steps(pressed, released, cancelled), vec![Step::Release]);
        assert!(at > 0);
        signals.press();
        let (again, _, _) = signals.observe();
        signals.release(true);
        signals.release(false);
        let (end, cancelled, _) = signals.observe();
        assert_eq!(steps(again, end, cancelled), vec![Step::Cancel]);
    }
}
