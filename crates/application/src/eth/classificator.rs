use std::{io, sync::Arc};

use domain::eth::{
    EthAddress, EthTx,
    graph::{Interaction, NativeTransfer},
};

use crate::eth::EthRpcSource;

#[derive(Debug)]
pub enum ClassificateError {
    InvariantNarushen,
    Rpc(io::Error),
}

pub struct FulliestEthTxClassificator {
    rpc_service: Arc<dyn EthRpcSource>,
}

impl FulliestEthTxClassificator {
    pub fn new(rpc_service: Arc<dyn EthRpcSource>) -> Self {
        Self { rpc_service }
    }
}

#[async_trait::async_trait]
pub trait EthTxClassificator: Send + Sync {
    async fn classificate(&self, tx: EthTx) -> Result<Interaction, ClassificateError>;
}

#[async_trait::async_trait]
impl EthTxClassificator for FulliestEthTxClassificator {
    async fn classificate(&self, tx: EthTx) -> Result<Interaction, ClassificateError> {
        const ZEROXWALLET: [u8; 20] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        if tx.to() == Some(&EthAddress::from(ZEROXWALLET)) {
            return Ok(Interaction::Protocol);
        };

        if tx.to().is_some() && tx.data().is_empty() {
            return Ok(Interaction::NativeTransfer(
                NativeTransfer::try_from(tx).map_err(|_| ClassificateError::InvariantNarushen)?,
            ));
        };

        let _receipt = self
            .rpc_service
            .receipt(tx.tx_hash())
            .await
            .map_err(ClassificateError::Rpc)?;

        unimplemented!()
    }
}
