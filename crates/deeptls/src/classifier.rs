use serde::{Deserialize, Serialize};

use crate::features;
use crate::knn::Knn;
use crate::sim::net::{self, Defense};
use crate::sim::rng::Rng;
use crate::sim::wallet::{self, Transaction};
use crate::tls::{Packet, ParseError, Record, parse_records};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Intent {
    Broadcast,
    Query,
}

impl Intent {
    pub fn as_str(self) -> &'static str {
        match self {
            Intent::Broadcast => "broadcast",
            Intent::Query => "query",
        }
    }

    fn of(tx: Transaction) -> Self {
        match tx {
            Transaction::EthTransfer
            | Transaction::Erc20Transfer
            | Transaction::TrxTransfer
            | Transaction::Trc20Transfer => Intent::Broadcast,
            Transaction::EthPortfolio
            | Transaction::EthLogScan
            | Transaction::TronAccount => Intent::Query,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Verdict {
    pub intent: Intent,
    pub confidence: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct TrainConfig {
    pub seed: u64,
    pub per_class: usize,
    pub k: usize,
}

impl Default for TrainConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            per_class: 150,
            k: 5,
        }
    }
}

pub struct Classifier {
    model: Knn<Intent>,
}

impl Classifier {
    pub fn train(config: TrainConfig) -> Self {
        let mut rng = Rng::new(config.seed);
        let mut samples: Vec<(Vec<f64>, Intent)> = Vec::new();
        for &tx in &Transaction::ALL {
            for _ in 0..config.per_class.max(4) {
                let exchanges = wallet::run(tx, &mut rng);
                let packets = net::capture(&exchanges, Defense::None, &mut rng);
                let records = parse_records(&packets).expect("simulated capture is valid TLS");
                samples.push((features::extract(&records), Intent::of(tx)));
            }
        }
        rng.shuffle(&mut samples);
        Self {
            model: Knn::fit(&samples, config.k.max(1)),
        }
    }

    pub fn classify(&self, records: &[Record]) -> Verdict {
        let (intent, confidence) = self.model.predict_scored(&features::extract(records));
        Verdict { intent, confidence }
    }

    pub fn classify_packets(&self, packets: &[Packet]) -> Result<Verdict, ParseError> {
        Ok(self.classify(&parse_records(packets)?))
    }
}
