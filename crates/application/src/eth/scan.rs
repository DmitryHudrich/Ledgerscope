use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use domain::eth::BlockRange;
use tokio::sync::broadcast;

const KEPT_JOBS: usize = 50;
const STREAM_BUFFER: usize = 256;

pub trait ScanObserver: Send + Sync {
    fn begin(&self, ranges: &[BlockRange]) -> u64;

    fn advance(&self, job: u64, blocks: u64, txs: u64);

    fn finish(&self, job: u64, error: Option<String>);
}

pub struct NoScans;

impl ScanObserver for NoScans {
    fn begin(&self, _ranges: &[BlockRange]) -> u64 {
        0
    }

    fn advance(&self, _job: u64, _blocks: u64, _txs: u64) {}

    fn finish(&self, _job: u64, _error: Option<String>) {}
}

#[derive(Clone, Debug)]
pub struct ScanJob {
    pub id: u64,
    pub chain_id: u64,
    pub ranges: Vec<BlockRange>,
    pub total_blocks: u64,
    pub done_blocks: u64,
    pub txs: u64,
    pub started_ms: u64,
    pub updated_ms: u64,
    pub finished_ms: Option<u64>,
    pub error: Option<String>,
}

impl ScanJob {
    pub fn running(&self) -> bool {
        self.finished_ms.is_none()
    }

    pub fn blocks_per_second(&self) -> Option<f64> {
        let elapsed = self.updated_ms.saturating_sub(self.started_ms);
        (elapsed > 0 && self.done_blocks > 0)
            .then(|| self.done_blocks as f64 * 1000.0 / elapsed as f64)
    }

    pub fn eta_ms(&self) -> Option<u64> {
        if !self.running() {
            return None;
        }
        let rate = self.blocks_per_second()?;
        let left = self.total_blocks.saturating_sub(self.done_blocks) as f64;
        Some((left / rate * 1000.0) as u64)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

pub struct ScanBoard {
    next_id: AtomicU64,
    jobs: Mutex<VecDeque<ScanJob>>,
    stream: broadcast::Sender<ScanJob>,
}

impl Default for ScanBoard {
    fn default() -> Self {
        let (stream, _) = broadcast::channel(STREAM_BUFFER);
        Self {
            next_id: AtomicU64::new(1),
            jobs: Mutex::new(VecDeque::new()),
            stream,
        }
    }
}

impl ScanBoard {
    pub fn for_chain(self: &Arc<Self>, chain_id: u64) -> Arc<dyn ScanObserver> {
        Arc::new(ChainScans {
            board: self.clone(),
            chain_id,
        })
    }

    pub fn jobs(&self, chain_id: Option<u64>) -> Vec<ScanJob> {
        self.jobs
            .lock()
            .expect("the lock is never poisoned")
            .iter()
            .rev()
            .filter(|job| chain_id.is_none_or(|chain_id| job.chain_id == chain_id))
            .cloned()
            .collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ScanJob> {
        self.stream.subscribe()
    }

    fn change(&self, id: u64, apply: impl FnOnce(&mut ScanJob)) {
        let changed = {
            let mut jobs = self.jobs.lock().expect("the lock is never poisoned");
            let Some(job) = jobs.iter_mut().find(|job| job.id == id) else {
                return;
            };
            apply(job);
            job.updated_ms = now_ms();
            job.clone()
        };
        let _ = self.stream.send(changed);
    }
}

struct ChainScans {
    board: Arc<ScanBoard>,
    chain_id: u64,
}

impl ScanObserver for ChainScans {
    fn begin(&self, ranges: &[BlockRange]) -> u64 {
        let now = now_ms();
        let job = ScanJob {
            id: self.board.next_id.fetch_add(1, Ordering::Relaxed),
            chain_id: self.chain_id,
            ranges: ranges.to_vec(),
            total_blocks: ranges.iter().map(BlockRange::block_count).sum(),
            done_blocks: 0,
            txs: 0,
            started_ms: now,
            updated_ms: now,
            finished_ms: None,
            error: None,
        };

        {
            let mut jobs = self.board.jobs.lock().expect("the lock is never poisoned");
            jobs.push_back(job.clone());
            while jobs.len() > KEPT_JOBS {
                let Some(finished) = jobs.iter().position(|job| !job.running()) else {
                    break;
                };
                jobs.remove(finished);
            }
        }
        let _ = self.board.stream.send(job.clone());
        job.id
    }

    fn advance(&self, job: u64, blocks: u64, txs: u64) {
        self.board.change(job, |job| {
            job.done_blocks = (job.done_blocks + blocks).min(job.total_blocks);
            job.txs += txs;
        });
    }

    fn finish(&self, job: u64, error: Option<String>) {
        self.board.change(job, |job| {
            job.finished_ms = Some(now_ms());
            job.error = error;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_counts_its_blocks_and_finishes() {
        let board = Arc::new(ScanBoard::default());
        let scans = board.for_chain(1);
        let id = scans.begin(&[BlockRange::new(10, 19), BlockRange::new(30, 34)]);

        scans.advance(id, 10, 7);
        let job = &board.jobs(Some(1))[0];
        assert_eq!((job.total_blocks, job.done_blocks, job.txs), (15, 10, 7));
        assert!(job.running());

        scans.advance(id, 50, 0);
        scans.finish(id, None);
        let job = &board.jobs(None)[0];
        assert_eq!(job.done_blocks, 15);
        assert!(!job.running());
        assert_eq!(job.eta_ms(), None);
        assert!(board.jobs(Some(10)).is_empty());
    }

    #[test]
    fn old_finished_jobs_make_room() {
        let board = Arc::new(ScanBoard::default());
        let scans = board.for_chain(1);
        let running = scans.begin(&[BlockRange::new(0, 0)]);
        for _ in 0..KEPT_JOBS + 5 {
            let id = scans.begin(&[BlockRange::new(0, 0)]);
            scans.finish(id, None);
        }
        let jobs = board.jobs(None);
        assert_eq!(jobs.len(), KEPT_JOBS);
        assert!(jobs.iter().any(|job| job.id == running));
    }

    #[tokio::test]
    async fn listeners_hear_progress() {
        let board = Arc::new(ScanBoard::default());
        let mut listener = board.subscribe();
        let scans = board.for_chain(728_126_428);
        let id = scans.begin(&[BlockRange::new(1, 100)]);
        scans.advance(id, 48, 3);

        assert_eq!(listener.recv().await.unwrap().done_blocks, 0);
        assert_eq!(listener.recv().await.unwrap().done_blocks, 48);
    }
}
