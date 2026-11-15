use std::sync::Arc;

use domain::eth::BlockRange;
use tokio::sync::broadcast;

use crate::eth::jobs::{Job, JobBoard, now_ms};

pub const STOPPED: &str = "stopped before it finished";

pub trait ScanObserver: Send + Sync {
    fn begin(&self, ranges: &[BlockRange]) -> u64;

    fn advance(&self, job: u64, blocks: u64, txs: u64);

    fn finish(&self, job: u64, error: Option<String>);
}

pub struct ScanRun<'a> {
    scans: &'a dyn ScanObserver,
    job: Option<u64>,
}

impl<'a> ScanRun<'a> {
    pub fn begin(scans: &'a dyn ScanObserver, ranges: &[BlockRange]) -> Self {
        Self {
            job: Some(scans.begin(ranges)),
            scans,
        }
    }

    pub fn id(&self) -> u64 {
        self.job.unwrap_or_default()
    }

    pub fn finish(mut self, error: Option<String>) {
        if let Some(job) = self.job.take() {
            self.scans.finish(job, error);
        }
    }
}

impl Drop for ScanRun<'_> {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            self.scans.finish(job, Some(STOPPED.to_owned()));
        }
    }
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

impl Job for ScanJob {
    fn id(&self) -> u64 {
        self.id
    }

    fn chain_id(&self) -> u64 {
        self.chain_id
    }

    fn running(&self) -> bool {
        self.finished_ms.is_none()
    }

    fn touched(&mut self, now_ms: u64) {
        self.updated_ms = now_ms;
    }
}

#[derive(Default)]
pub struct ScanBoard {
    board: JobBoard<ScanJob>,
}

impl ScanBoard {
    pub fn for_chain(self: &Arc<Self>, chain_id: u64) -> Arc<dyn ScanObserver> {
        Arc::new(ChainScans {
            board: self.clone(),
            chain_id,
        })
    }

    pub fn jobs(&self, chain_id: Option<u64>) -> Vec<ScanJob> {
        self.board.jobs(chain_id)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ScanJob> {
        self.board.subscribe()
    }
}

struct ChainScans {
    board: Arc<ScanBoard>,
    chain_id: u64,
}

impl ScanObserver for ChainScans {
    fn begin(&self, ranges: &[BlockRange]) -> u64 {
        let now = now_ms();
        self.board.board.open(|id| ScanJob {
            id,
            chain_id: self.chain_id,
            ranges: ranges.to_vec(),
            total_blocks: ranges.iter().map(BlockRange::block_count).sum(),
            done_blocks: 0,
            txs: 0,
            started_ms: now,
            updated_ms: now,
            finished_ms: None,
            error: None,
        })
    }

    fn advance(&self, job: u64, blocks: u64, txs: u64) {
        self.board.board.change(job, |job| {
            job.done_blocks = (job.done_blocks + blocks).min(job.total_blocks);
            job.txs += txs;
        });
    }

    fn finish(&self, job: u64, error: Option<String>) {
        self.board.board.change(job, |job| {
            job.finished_ms = Some(now_ms());
            job.error = error;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eth::jobs::KEPT_JOBS;

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
    fn a_run_dropped_midway_ends_as_stopped() {
        let board = Arc::new(ScanBoard::default());
        let scans = board.for_chain(1);

        let run = ScanRun::begin(scans.as_ref(), &[BlockRange::new(0, 9)]);
        assert!(board.jobs(None)[0].running());
        drop(run);

        let job = &board.jobs(None)[0];
        assert!(!job.running());
        assert_eq!(job.error.as_deref(), Some(STOPPED));
    }

    #[test]
    fn a_finished_run_keeps_its_own_outcome() {
        let board = Arc::new(ScanBoard::default());
        let scans = board.for_chain(1);

        ScanRun::begin(scans.as_ref(), &[BlockRange::new(0, 9)]).finish(None);

        let job = &board.jobs(None)[0];
        assert!(!job.running());
        assert_eq!(job.error, None);
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
