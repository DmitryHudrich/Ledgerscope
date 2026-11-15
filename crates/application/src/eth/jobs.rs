use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tokio::sync::broadcast;

pub(crate) const KEPT_JOBS: usize = 50;
const STREAM_BUFFER: usize = 256;

pub trait Job: Clone + Send + 'static {
    fn id(&self) -> u64;

    fn chain_id(&self) -> u64;

    fn running(&self) -> bool;

    fn touched(&mut self, now_ms: u64);
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

pub struct JobBoard<J> {
    next_id: AtomicU64,
    jobs: Mutex<VecDeque<J>>,
    stream: broadcast::Sender<J>,
}

impl<J: Job> Default for JobBoard<J> {
    fn default() -> Self {
        let (stream, _) = broadcast::channel(STREAM_BUFFER);
        Self {
            next_id: AtomicU64::new(1),
            jobs: Mutex::new(VecDeque::new()),
            stream,
        }
    }
}

impl<J: Job> JobBoard<J> {
    pub fn jobs(&self, chain_id: Option<u64>) -> Vec<J> {
        self.jobs
            .lock()
            .expect("the lock is never poisoned")
            .iter()
            .rev()
            .filter(|job| chain_id.is_none_or(|chain_id| job.chain_id() == chain_id))
            .cloned()
            .collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<J> {
        self.stream.subscribe()
    }

    pub(crate) fn open(&self, make: impl FnOnce(u64) -> J) -> u64 {
        let job = make(self.next_id.fetch_add(1, Ordering::Relaxed));
        let id = job.id();
        {
            let mut jobs = self.jobs.lock().expect("the lock is never poisoned");
            jobs.push_back(job.clone());
            while jobs.len() > KEPT_JOBS {
                let Some(finished) = jobs.iter().position(|job| !job.running()) else {
                    break;
                };
                jobs.remove(finished);
            }
        }
        let _ = self.stream.send(job);
        id
    }

    pub(crate) fn change(&self, id: u64, apply: impl FnOnce(&mut J)) {
        let changed = {
            let mut jobs = self.jobs.lock().expect("the lock is never poisoned");
            let Some(job) = jobs.iter_mut().find(|job| job.id() == id) else {
                return;
            };
            apply(job);
            job.touched(now_ms());
            job.clone()
        };
        let _ = self.stream.send(changed);
    }
}
