use std::{io, sync::Arc};

use alloy_primitives::{B256, Bytes, U256, b256};

use domain::eth::{
    BlockRef, EthAddress, EthLog, EthTx,
    graph::{ContractAction, Interaction, InteractionKind, NativeTransfer},
};

use crate::eth::EthRpcSource;

const TRANSFER_TOPIC_SIGNATURE: B256 =
    b256!("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");

const SYMBOL_SELECTOR: [u8; 4] = [0x95, 0xd8, 0x9b, 0x41];

#[derive(Debug)]
pub enum ClassificateError {
    InvariantNarushen,
    ReceiptMissing,
    Rpc(io::Error),
}

pub struct FulliestEthTxClassificator {
    rpc_service: Arc<dyn EthRpcSource>,
}

impl FulliestEthTxClassificator {
    pub fn new(rpc_service: Arc<dyn EthRpcSource>) -> Self {
        Self { rpc_service }
    }

    async fn token_name(&self, token: &EthAddress) -> Result<String, ClassificateError> {
        let returned = self
            .rpc_service
            .call(token, &SYMBOL_SELECTOR, BlockRef::Latest)
            .await
            .map_err(ClassificateError::Rpc)?;

        Ok(decode_symbol(&returned).unwrap_or_else(|| token.to_string()))
    }
}

struct Erc20Transfer {
    token: EthAddress,
    from: EthAddress,
    to: EthAddress,
    amount: U256,
}

fn decode_erc20_transfer(log: &EthLog) -> Option<Erc20Transfer> {
    let topics = log.topics();

    if topics.first() != Some(&TRANSFER_TOPIC_SIGNATURE)
        || topics.len() != 3
        || log.data().len() != 32
    {
        return None;
    }

    Some(Erc20Transfer {
        token: *log.address(),
        from: EthAddress::from_word(topics[1]),
        to: EthAddress::from_word(topics[2]),
        amount: U256::from_be_slice(log.data()),
    })
}

fn decode_symbol(returned: &Bytes) -> Option<String> {
    if returned.len() == 32 {
        let text: Vec<u8> = returned
            .iter()
            .take_while(|byte| **byte != 0)
            .copied()
            .collect();
        return String::from_utf8(text).ok().filter(|name| !name.is_empty());
    }

    let word_at = |at: usize| -> Option<usize> {
        let word = returned.get(at..at.checked_add(32)?)?;
        usize::try_from(U256::from_be_slice(word)).ok()
    };

    let offset = word_at(0)?;
    let length = word_at(offset)?;
    let start = offset.checked_add(32)?;
    let text = returned.get(start..start.checked_add(length)?)?;

    String::from_utf8(text.to_vec())
        .ok()
        .filter(|name| !name.is_empty())
}

#[async_trait::async_trait]
pub trait EthTxClassificator: Send + Sync {
    async fn classificate(&self, tx: EthTx) -> Result<Interaction, ClassificateError>;
}

#[async_trait::async_trait]
impl EthTxClassificator for FulliestEthTxClassificator {
    async fn classificate(&self, tx: EthTx) -> Result<Interaction, ClassificateError> {
        let receipt = self
            .rpc_service
            .receipt(tx.tx_hash())
            .await
            .map_err(ClassificateError::Rpc)?
            .ok_or(ClassificateError::ReceiptMissing)?;

        if tx.to() == Some(&EthAddress::ZERO) {
            return Ok(Interaction::new(receipt, InteractionKind::Protocol));
        }

        let Some(contract_address) = tx.to().copied() else {
            let deployed = receipt
                .contract_address()
                .copied()
                .ok_or(ClassificateError::InvariantNarushen)?;

            return Ok(Interaction::new(
                receipt,
                InteractionKind::ContractDeployment {
                    contract_address: deployed,
                    deployer: *tx.from(),
                },
            ));
        };

        if tx.data().is_empty() {
            let transfer =
                NativeTransfer::try_from(tx).map_err(|_| ClassificateError::InvariantNarushen)?;

            return Ok(Interaction::new(
                receipt,
                InteractionKind::NativeTransfer(transfer),
            ));
        }

        let interactor = *tx.from();
        let erc20 = receipt.logs().iter().find_map(decode_erc20_transfer);

        let contract_interaction_type = match erc20 {
            Some(transfer) => ContractAction::Erc20Transfer {
                from: transfer.from,
                to: transfer.to,
                amount: transfer.amount,
                token_name: self.token_name(&transfer.token).await?,
            },
            None => ContractAction::Other,
        };

        Ok(Interaction::new(
            receipt,
            InteractionKind::ContractInteraction {
                contract_address,
                interactor,
                contract_interaction_type,
            },
        ))
    }
}
