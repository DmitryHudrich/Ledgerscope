use std::sync::Arc;

use domain::eth::{
    EthAddress, EthTx,
    graph::{Interaction, NativeTransfer},
};

use crate::eth::EthRpcSource;

#[derive(Debug)]
pub enum ClassificateError {
    InvariantNarushen,
}

pub struct FulliestEthTxClassificator {
    rpc_service: Arc<dyn EthRpcSource>,
}

impl FulliestEthTxClassificator {
    pub fn new(rpc_service: Arc<dyn EthRpcSource>) -> Self {
        Self { rpc_service }
    }
}

pub trait EthTxClassificator: Send + Sync {
    fn classificate(&self, tx: EthTx) -> Result<Interaction, ClassificateError>;
}

impl EthTxClassificator for FulliestEthTxClassificator {
    fn classificate(&self, tx: EthTx) -> Result<Interaction, ClassificateError> {
        const ZEROXWALLET: [u8; 20] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        if tx.to() == Some(&EthAddress::from(ZEROXWALLET)) {
            return Ok(Interaction::Protocol);
        };

        if tx.to().is_some() && tx.data().is_empty() {
            return Ok(Interaction::NativeTransfer(
                NativeTransfer::try_from(tx).map_err(|_| ClassificateError::InvariantNarushen)?,
            ));
        };

        unimplemented!()
    }
}
