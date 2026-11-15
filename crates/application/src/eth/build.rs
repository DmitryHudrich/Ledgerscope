use std::sync::Arc;

use domain::eth::BlockRange;
use tokio::sync::broadcast;

use crate::eth::{
    jobs::{Job, JobBoard, now_ms},
    scan::STOPPED,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildStage {
    Coverage,
    Walk,
    Index,
    Classify,
    Actors,
    Tags,
}

impl BuildStage {
    pub fn as_str(self) -> &'static str {
        match self {
            BuildStage::Coverage => "coverage",
            BuildStage::Walk => "walk",
            BuildStage::Index => "index",
            BuildStage::Classify => "classify",
            BuildStage::Actors => "actors",
            BuildStage::Tags => "tags",
        }
    }

    fn share(self) -> (f64, f64) {
        match self {
            BuildStage::Coverage => (0.0, 0.05),
            BuildStage::Walk => (0.05, 0.6),
            BuildStage::Index => (0.05, 0.35),
            BuildStage::Classify => (0.35, 0.6),
            BuildStage::Actors => (0.6, 0.95),
            BuildStage::Tags => (0.95, 1.0),
        }
    }
}

pub trait BuildObserver: Send + Sync {
    fn begin(&self, spans: &[BlockRange]) -> u64;

    fn stage(&self, job: u64, stage: BuildStage, total: u64);

    fn grow(&self, job: u64, more: u64);

    fn advance(&self, job: u64, done: u64, txs: u64);

    fn finish(&self, job: u64, error: Option<String>);
}

pub struct NoBuilds;

impl BuildObserver for NoBuilds {
    fn begin(&self, _spans: &[BlockRange]) -> u64 {
        0
    }

    fn stage(&self, _job: u64, _stage: BuildStage, _total: u64) {}

    fn grow(&self, _job: u64, _more: u64) {}

    fn advance(&self, _job: u64, _done: u64, _txs: u64) {}

    fn finish(&self, _job: u64, _error: Option<String>) {}
}

pub struct BuildRun<'a> {
    builds: &'a dyn BuildObserver,
    job: Option<u64>,
}

impl<'a> BuildRun<'a> {
    pub fn begin(builds: &'a dyn BuildObserver, spans: &[BlockRange]) -> Self {
        Self {
            job: Some(builds.begin(spans)),
            builds,
        }
    }

    pub fn stage(&self, stage: BuildStage, total: u64) {
        if let Some(job) = self.job {
            self.builds.stage(job, stage, total);
        }
    }

    pub fn grow(&self, more: u64) {
        if let Some(job) = self.job {
            self.builds.grow(job, more);
        }
    }

    pub fn advance(&self, done: u64, txs: u64) {
        if let Some(job) = self.job {
            self.builds.advance(job, done, txs);
        }
    }

    pub fn finish(mut self, error: Option<String>) {
        if let Some(job) = self.job.take() {
            self.builds.finish(job, error);
        }
    }
}

impl Drop for BuildRun<'_> {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            self.builds.finish(job, Some(STOPPED.to_owned()));
        }
    }
}

#[derive(Clone, Debug)]
pub struct BuildJob {
    pub id: u64,
    pub chain_id: u64,
    pub spans: Vec<BlockRange>,
    pub stage: BuildStage,
    pub done: u64,
    pub total: u64,
    pub txs: u64,
    pub started_ms: u64,
    pub updated_ms: u64,
    pub finished_ms: Option<u64>,
    pub error: Option<String>,
}

impl BuildJob {
    pub fn running(&self) -> bool {
        self.finished_ms.is_none()
    }

    pub fn percent(&self) -> f64 {
        if !self.running() && self.error.is_none() {
            return 100.0;
        }
        let (from, to) = self.stage.share();
        let within = if self.total == 0 {
            0.0
        } else {
            (self.done as f64 / self.total as f64).min(1.0)
        };
        (from + (to - from) * within) * 100.0
    }
}

impl Job for BuildJob {
    fn id(&self) -> u64 {
        self.id
    }

    fn chain_id(&self) -> u64 {
        self.chain_id
    }

    fn running(&self) -> bool {
        BuildJob::running(self)
    }

    fn touched(&mut self, now_ms: u64) {
        self.updated_ms = now_ms;
    }
}

#[derive(Default)]
pub struct BuildBoard {
    board: JobBoard<BuildJob>,
}

impl BuildBoard {
    pub fn for_chain(self: &Arc<Self>, chain_id: u64) -> Arc<dyn BuildObserver> {
        Arc::new(ChainBuilds {
            board: self.clone(),
            chain_id,
        })
    }

    pub fn jobs(&self, chain_id: Option<u64>) -> Vec<BuildJob> {
        self.board.jobs(chain_id)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BuildJob> {
        self.board.subscribe()
    }
}

struct ChainBuilds {
    board: Arc<BuildBoard>,
    chain_id: u64,
}

impl BuildObserver for ChainBuilds {
    fn begin(&self, spans: &[BlockRange]) -> u64 {
        let now = now_ms();
        self.board.board.open(|id| BuildJob {
            id,
            chain_id: self.chain_id,
            spans: spans.to_vec(),
            stage: BuildStage::Coverage,
            done: 0,
            total: 0,
            txs: 0,
            started_ms: now,
            updated_ms: now,
            finished_ms: None,
            error: None,
        })
    }

    fn stage(&self, job: u64, stage: BuildStage, total: u64) {
        self.board.board.change(job, |job| {
            job.stage = stage;
            job.done = 0;
            job.total = total;
        });
    }

    fn grow(&self, job: u64, more: u64) {
        self.board.board.change(job, |job| job.total += more);
    }

    fn advance(&self, job: u64, done: u64, txs: u64) {
        self.board.board.change(job, |job| {
            job.done = (job.done + done).min(job.total);
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

    #[test]
    fn stages_move_the_percent_forward() {
        let board = Arc::new(BuildBoard::default());
        let builds = board.for_chain(1);
        let run = BuildRun::begin(builds.as_ref(), &[BlockRange::new(0, 9)]);

        run.stage(BuildStage::Index, 4);
        run.advance(2, 100);
        let halfway = board.jobs(None)[0].percent();
        run.stage(BuildStage::Actors, 10);
        let later = board.jobs(None)[0].percent();
        run.finish(None);

        let job = &board.jobs(Some(1))[0];
        assert!(halfway > 5.0 && halfway < 35.0);
        assert!(later > halfway);
        assert_eq!(job.percent(), 100.0);
        assert_eq!(job.txs, 100);
    }

    #[test]
    fn a_walk_can_learn_its_size_on_the_way() {
        let board = Arc::new(BuildBoard::default());
        let builds = board.for_chain(1);
        let run = BuildRun::begin(builds.as_ref(), &[BlockRange::new(0, 9)]);

        run.stage(BuildStage::Walk, 1);
        run.advance(1, 0);
        run.grow(3);

        let job = &board.jobs(None)[0];
        assert_eq!((job.done, job.total), (1, 4));
    }

    #[test]
    fn a_dropped_build_ends_as_stopped() {
        let board = Arc::new(BuildBoard::default());
        let builds = board.for_chain(1);

        drop(BuildRun::begin(builds.as_ref(), &[BlockRange::new(0, 9)]));

        let job = &board.jobs(None)[0];
        assert!(!job.running());
        assert_eq!(job.error.as_deref(), Some(STOPPED));
    }
}
