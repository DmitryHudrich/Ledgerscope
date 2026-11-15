use std::time::Duration;

use tokio::{
    sync::{Mutex, Semaphore, SemaphorePermit},
    time::{Instant, sleep_until},
};

const MAX_IN_FLIGHT: usize = 8;

pub struct RateLimiter {
    interval: Duration,
    next_slot: Mutex<Instant>,
    in_flight: Semaphore,
}

impl RateLimiter {
    pub fn new(rps: u32) -> Self {
        Self {
            interval: Duration::from_secs(1) / rps.max(1),
            next_slot: Mutex::new(Instant::now()),
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
            let mut next_slot = self.next_slot.lock().await;
            let slot = (*next_slot).max(Instant::now()) + self.interval * (requests - 1);
            *next_slot = slot + self.interval;
            slot
        };

        sleep_until(slot).await;
        permit
    }

    pub async fn ready_at(&self) -> Instant {
        (*self.next_slot.lock().await).max(Instant::now())
    }

    pub fn saturated(&self) -> bool {
        self.in_flight.available_permits() == 0
    }

    pub async fn throttle(&self, cooldown: Duration) {
        let resume = Instant::now() + cooldown;
        let mut next_slot = self.next_slot.lock().await;
        *next_slot = (*next_slot).max(resume);
    }
}
