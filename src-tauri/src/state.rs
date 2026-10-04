use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetState {
    Idle,
    Typing,
}

/// Image 0 is idle; images 1..=frames are typing. Time is monotonic and injected.
pub struct Animation {
    state: PetState,
    frames: usize,
    frame: usize,
    shown: usize,
    timeout: Duration,
    hold: Duration,
    idle_at: Option<Duration>,
    show_at: Option<Duration>,
    last_shown: Option<Duration>,
}

impl Animation {
    pub fn new(frames: usize, timeout_ms: u64, hold_ms: u64) -> Self {
        assert!(frames > 0);
        Self {
            state: PetState::Idle,
            frames,
            frame: 0,
            shown: 0,
            timeout: Duration::from_millis(timeout_ms),
            hold: Duration::from_millis(hold_ms),
            idle_at: None,
            show_at: None,
            last_shown: None,
        }
    }

    pub fn key(&mut self, now: Duration) -> Option<usize> {
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
        self.idle_at = Some(now + self.timeout);
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
        let image = self.frame + 1;
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
        self.idle_at = None;
        self.show_at = None;
        self.last_shown = None;
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
