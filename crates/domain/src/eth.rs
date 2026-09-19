pub mod actor;
pub mod address;
pub mod block;
pub mod coverage;
pub mod graph;
pub mod lowlevel;
pub mod receipt;
pub mod tx;
pub mod label {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct AddressLabel {
        value: String,
        source: String,
    }

    impl AddressLabel {
        pub fn new(value: String, source: String) -> Self {
            Self { value, source }
        }

        pub fn value(&self) -> &str {
            &self.value
        }

        pub fn source(&self) -> &str {
            &self.source
        }
    }
}

pub use actor::{Actor, ActorHint, ActorKind, ContractKind};
pub use address::{EthAddress, EthAddressParseError};
pub use block::BlockRef;
pub use coverage::{BlockBucket, BlockRange, IndexCoverage};
pub use graph::{
    ContractAction, Interaction, InteractionEdge, InteractionId, InteractionKind, TxMeta,
};
pub use label::*;
pub use lowlevel::{LowLevelActor, LowLevelInteraction, LowLevelTarget};
pub use receipt::{EthLog, EthReceipt};
pub use tx::{EthTx, MinedTx};
