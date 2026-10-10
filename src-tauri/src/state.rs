use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetState {
    Idle,
    Typing,
}

/// Image 0 is idle; images 1..=frames are typing; mappings may select any image.
/// Time is monotonic and injected.
pub struct Animation {
    state: PetState,
    frames: usize,
    frame: usize,
    shown: usize,
    requested: usize,
    timeout: Duration,
    hold: Duration,
    idle_at: Option<Duration>,
    show_at: Option<Duration>,
    last_shown: Option<Duration>,
    held: bool,
}

impl Animation {
    pub fn new(frames: usize, timeout_ms: u64, hold_ms: u64) -> Self {
        assert!(frames > 0);
        Self {
            state: PetState::Idle,
            frames,
            frame: 0,
            shown: 0,
            requested: 0,
            timeout: Duration::from_millis(timeout_ms),
            hold: Duration::from_millis(hold_ms),
            idle_at: None,
            show_at: None,
            last_shown: None,
            held: false,
        }
    }

    pub fn key(&mut self, now: Duration) -> Option<usize> {
        self.key_frame(now, None)
    }

    pub fn key_frame(&mut self, now: Duration, mapped: Option<usize>) -> Option<usize> {
        // Even if delivery races with a timeout, a new burst starts at frame 0.
        if self.idle_at.is_some_and(|at| now >= at) {
            self.reset();
        }
        if self.state == PetState::Idle {
            self.state = PetState::Typing;
            self.frame = 0;
        } else {
            self.frame = (self.frame + 1) % self.frames;
        }
        self.requested = mapped.unwrap_or(self.frame + 1);
        self.idle_at = (!self.held).then_some(now + self.timeout);
        if self.last_shown.is_none_or(|last| now >= last + self.hold) {
            self.show_at = None;
            self.show(now)
        } else {
            self.show_at = self.last_shown.map(|last| last + self.hold);
            None
        }
    }

    pub fn tick(&mut self, now: Duration) -> Option<usize> {
        if self.idle_at.is_some_and(|at| now >= at) {
            self.reset();
            return Some(0);
        }
        if self.show_at.is_some_and(|at| now >= at) {
            self.show_at = None;
            return self.show(now);
        }
        None
    }

    fn show(&mut self, now: Duration) -> Option<usize> {
        let image = self.requested;
        if self.shown == image {
            return None;
        }
        self.shown = image;
        self.last_shown = Some(now);
        Some(image)
    }

    pub fn reset(&mut self) {
        self.state = PetState::Idle;
        self.frame = 0;
        self.shown = 0;
        self.requested = 0;
        self.idle_at = None;
        self.show_at = None;
        self.last_shown = None;
        self.held = false;
    }

    pub fn set_timing(&mut self, timeout_ms: u64, hold_ms: u64) {
        let timeout = Duration::from_millis(timeout_ms);
        if let Some(at) = self.idle_at {
            self.idle_at = Some(at.saturating_sub(self.timeout) + timeout);
        }
        self.timeout = timeout;
        self.hold = Duration::from_millis(hold_ms);
        if self.show_at.is_some() {
            self.show_at = self.last_shown.map(|last| last + self.hold);
        }
    }

    pub fn set_held(&mut self, held: bool, now: Duration) {
        if self.held == held {
            return;
        }
        if held && self.idle_at.is_some_and(|at| now >= at) {
            self.reset();
        }
        self.held = held;
        if held {
            self.idle_at = None;
        } else if self.state == PetState::Typing {
            self.idle_at = Some(now + self.timeout);
        }
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        match (self.idle_at, self.show_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn timing_updates_recompute_pending_deadlines_without_resetting_frame() {
        let mut a = Animation::new(2, 180, 60);
        assert_eq!(a.key(ms(0)), Some(1));
        assert_eq!(a.key(ms(10)), None);
        a.set_timing(500, 100);
        assert_eq!(a.next_deadline(), Some(ms(100)));
        assert_eq!(a.tick(ms(100)), Some(2));
        assert_eq!(a.tick(ms(190)), None);
        assert_eq!(a.tick(ms(510)), Some(0));
        a.key(ms(600));
        a.key(ms(610));
        a.set_timing(50, 0);
        assert_eq!(a.tick(ms(610)), Some(2));
        assert_eq!(a.tick(ms(660)), Some(0));
    }

    #[test]
    fn mapped_frames_repeat_extend_timeout_and_fall_back() {
        let mut a = Animation::new(2, 180, 0);
        assert_eq!(a.key_frame(ms(0), Some(3)), Some(3));
        assert_eq!(a.key_frame(ms(100), Some(3)), None);
        assert_eq!(a.tick(ms(180)), None);
        assert_eq!(a.tick(ms(280)), Some(0));
        assert_eq!(a.key_frame(ms(300), Some(4)), Some(4));
        assert_eq!(a.key(ms(310)), Some(2));
        assert_eq!(a.tick(ms(490)), Some(0));
    }

    #[test]
    fn latest_mapping_wins_during_frame_hold_and_reset_clears_it() {
        let mut a = Animation::new(2, 180, 60);
        assert_eq!(a.key_frame(ms(0), Some(3)), Some(3));
        assert_eq!(a.key_frame(ms(10), Some(4)), None);
        assert_eq!(a.key_frame(ms(20), Some(5)), None);
        assert_eq!(a.tick(ms(60)), Some(5));
        assert_eq!(a.tick(ms(200)), Some(0));
        assert_eq!(a.key(ms(210)), Some(1));
        a.key_frame(ms(220), Some(4));
        a.reset();
        assert_eq!(a.next_deadline(), None);
        assert_eq!(a.tick(ms(280)), None);
    }

    #[test]
    fn alternates_and_returns_to_idle() {
        let mut a = Animation::new(2, 180, 0);
        assert_eq!(a.next_deadline(), None);
        assert_eq!(a.key(ms(0)), Some(1));
        assert_eq!(a.key(ms(10)), Some(2));
        assert_eq!(a.key(ms(20)), Some(1));
        assert_eq!(a.tick(ms(199)), None);
        assert_eq!(a.tick(ms(200)), Some(0));
        assert_eq!(a.state, PetState::Idle);
        assert_eq!(a.next_deadline(), None);
    }

    #[test]
    fn each_key_extends_deadline_and_new_burst_starts_at_first_frame() {
        let mut a = Animation::new(2, 180, 0);
        a.key(ms(0));
        a.key(ms(170));
        assert_eq!(a.tick(ms(180)), None);
        assert_eq!(a.next_deadline(), Some(ms(350)));
        assert_eq!(a.key(ms(400)), Some(1));
    }

    #[test]
    fn holds_visual_frame_but_counts_every_press() {
        let mut a = Animation::new(2, 180, 60);
        assert_eq!(a.key(ms(0)), Some(1));
        assert_eq!(a.key(ms(10)), None);
        assert_eq!(a.tick(ms(59)), None);
        assert_eq!(a.tick(ms(60)), Some(2));
        assert_eq!(a.tick(ms(190)), Some(0));
        assert_eq!(a.next_deadline(), None);
    }

    #[test]
    fn bursts_do_not_create_free_running_animation() {
        let mut a = Animation::new(2, 180, 60);
        a.key(ms(0));
        a.key(ms(10));
        a.key(ms(20)); // latest frame is again typing_1
        assert_eq!(a.tick(ms(60)), None);
        assert_eq!(a.next_deadline(), Some(ms(200)));
        assert_eq!(a.tick(ms(200)), Some(0));
        assert_eq!(a.tick(ms(10_000)), None);
    }
}
