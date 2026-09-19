use std::time::Duration;

use tokio::{
    sync::Mutex,
    time::{Instant, sleep_until},
};

pub struct RateLimiter {
    interval: Duration,
    next_slot: Mutex<Instant>,
}

impl RateLimiter {
    pub fn new(rps: u32) -> Self {
        Self {
            interval: Duration::from_secs(1) / rps.max(1),
            next_slot: Mutex::new(Instant::now()),
        }
    }

    pub async fn acquire(&self, requests: usize) {
        let requests = u32::try_from(requests.max(1)).unwrap_or(u32::MAX);

        let slot = {
            let mut next_slot = self.next_slot.lock().await;
            let slot = (*next_slot).max(Instant::now());
            *next_slot = slot + self.interval * requests;
            slot
        };

        sleep_until(slot).await;
    }

    pub async fn throttle(&self, cooldown: Duration) {
        let resume = Instant::now() + cooldown;
        let mut next_slot = self.next_slot.lock().await;
        *next_slot = (*next_slot).max(resume);
    }
}
