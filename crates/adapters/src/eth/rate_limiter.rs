use std::time::Duration;

use tokio::{
    sync::{Mutex, Semaphore, SemaphorePermit},
    time::{Instant, sleep_until},
};

const MAX_IN_FLIGHT: usize = 8;
const SLOWEST_INTERVAL: Duration = Duration::from_secs(4);
const SLOW_DOWN_SPACING: Duration = Duration::from_secs(1);
const SPEED_UP_DIVISOR: u32 = 16;

struct Pace {
    next_slot: Instant,
    interval: Duration,
    slowed_at: Option<Instant>,
}

pub struct RateLimiter {
    base: Duration,
    pace: Mutex<Pace>,
    in_flight: Semaphore,
}

impl RateLimiter {
    pub fn new(rps: u32) -> Self {
        let base = Duration::from_secs(1) / rps.max(1);
        Self {
            base,
            pace: Mutex::new(Pace {
                next_slot: Instant::now(),
                interval: base,
                slowed_at: None,
            }),
            in_flight: Semaphore::new(MAX_IN_FLIGHT),
        }
    }

    pub async fn acquire(&self, requests: usize) -> SemaphorePermit<'_> {
        let permit = self
            .in_flight
            .acquire()
            .await
            .expect("the semaphore is never closed");
        let requests = u32::try_from(requests.max(1)).unwrap_or(u32::MAX);

        let slot = {
            let mut pace = self.pace.lock().await;
            let interval = pace.interval;
            let slot = pace.next_slot.max(Instant::now()) + interval * (requests - 1);
            pace.next_slot = slot + interval;
            slot
        };

        sleep_until(slot).await;
        permit
    }

    pub async fn ready_at(&self) -> Instant {
        self.pace.lock().await.next_slot.max(Instant::now())
    }

    pub fn saturated(&self) -> bool {
        self.in_flight.available_permits() == 0
    }

    pub async fn throttle(&self, cooldown: Duration) {
        let resume = Instant::now() + cooldown;
        let mut pace = self.pace.lock().await;
        pace.next_slot = pace.next_slot.max(resume);
    }

    pub async fn slow_down(&self) {
        let now = Instant::now();
        let mut pace = self.pace.lock().await;
        if pace
            .slowed_at
            .is_some_and(|at| now.duration_since(at) < SLOW_DOWN_SPACING)
        {
            return;
        }
        pace.slowed_at = Some(now);
        pace.interval = (pace.interval * 2).min(SLOWEST_INTERVAL.max(self.base));
    }

    pub async fn speed_up(&self) {
        let mut pace = self.pace.lock().await;
        if pace.interval > self.base {
            pace.interval = (pace.interval - pace.interval / SPEED_UP_DIVISOR).max(self.base);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl RateLimiter {
        async fn interval(&self) -> Duration {
            self.pace.lock().await.interval
        }
    }

    #[tokio::test]
    async fn a_refusal_halves_the_pace_and_successes_win_it_back() {
        let limiter = RateLimiter::new(10);
        let base = Duration::from_millis(100);

        limiter.slow_down().await;
        assert_eq!(limiter.interval().await, base * 2);

        limiter.slow_down().await;
        assert_eq!(limiter.interval().await, base * 2);

        for _ in 0..100 {
            limiter.speed_up().await;
        }
        assert_eq!(limiter.interval().await, base);
    }

    #[tokio::test]
    async fn the_pace_never_drops_below_the_slowest() {
        let limiter = RateLimiter::new(1);
        let mut pace = limiter.pace.lock().await;
        pace.interval = SLOWEST_INTERVAL;
        drop(pace);

        limiter.slow_down().await;
        assert_eq!(limiter.interval().await, SLOWEST_INTERVAL);
    }
}
