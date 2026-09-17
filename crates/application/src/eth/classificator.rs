use std::{fmt, io};

use alloy_primitives::{B256, U256, b256};

use domain::eth::{
    EthAddress, EthLog, MinedTx,
    graph::{ContractAction, Interaction, InteractionKind, NativeTransfer},
};

const TRANSFER_TOPIC_SIGNATURE: B256 =
    b256!("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");

#[derive(Debug)]
pub enum ClassificateError {
    InvariantNarushen,
    ReceiptMissing,
    Rpc(io::Error),
}

impl fmt::Display for ClassificateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClassificateError::InvariantNarushen => write!(f, "transaction broke its own shape"),
            ClassificateError::ReceiptMissing => write!(f, "node knows no receipt for it"),
            ClassificateError::Rpc(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ClassificateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ClassificateError::Rpc(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct FulliestEthTxClassificator;

impl FulliestEthTxClassificator {
    pub fn new() -> Self {
        Self
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

#[async_trait::async_trait]
pub trait EthTxClassificator: Send + Sync {
    async fn classificate(&self, mined: MinedTx) -> Result<Vec<Interaction>, ClassificateError>;
}

#[async_trait::async_trait]
impl EthTxClassificator for FulliestEthTxClassificator {
    async fn classificate(&self, mined: MinedTx) -> Result<Vec<Interaction>, ClassificateError> {
        let (tx, receipt) = mined.into_parts();

        if tx.to() == Some(&EthAddress::ZERO) {
            return Ok(vec![Interaction::new(receipt, InteractionKind::Protocol)]);
        }

        let Some(contract_address) = tx.to().copied() else {
            let deployed = receipt
                .contract_address()
                .copied()
                .ok_or(ClassificateError::InvariantNarushen)?;

            return Ok(vec![Interaction::new(
                receipt,
                InteractionKind::ContractDeployment {
                    contract_address: deployed,
                    deployer: *tx.from(),
                },
            )]);
        };

        if tx.data().is_empty() {
            let transfer =
                NativeTransfer::try_from(tx).map_err(|_| ClassificateError::InvariantNarushen)?;

            return Ok(vec![Interaction::new(
                receipt,
                InteractionKind::NativeTransfer(transfer),
            )]);
        }

        let interactor = *tx.from();
        let transfers: Vec<Erc20Transfer> = receipt
            .logs()
            .iter()
            .filter_map(decode_erc20_transfer)
            .collect();

        if transfers.is_empty() {
            return Ok(vec![Interaction::new(
                receipt,
                InteractionKind::ContractInteraction {
                    contract_address,
                    interactor,
                    contract_interaction_type: ContractAction::Other,
                },
            )]);
        }

        let interactions = transfers
            .into_iter()
            .map(|transfer| {
                Interaction::new(
                    receipt.clone(),
                    InteractionKind::ContractInteraction {
                        contract_address,
                        interactor,
                        contract_interaction_type: ContractAction::Erc20Transfer {
                            token: transfer.token,
                            from: transfer.from,
                            to: transfer.to,
                            amount: transfer.amount,
                        },
                    },
                )
            })
            .collect();

        Ok(interactions)
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Bytes, TxHash};
    use domain::eth::{EthLog, EthReceipt, EthTx};

    use super::*;

    const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn transfer_log(token: EthAddress, from: EthAddress, to: EthAddress, amount: u64) -> EthLog {
        EthLog::new(
            token,
            vec![
                TRANSFER_TOPIC_SIGNATURE,
                from.address().into_word(),
                to.address().into_word(),
            ],
            Bytes::from(U256::from(amount).to_be_bytes::<32>().to_vec()),
        )
    }

    fn call_tx(to: EthAddress) -> EthTx {
        EthTx::builder()
            .tx_hash(TxHash::with_last_byte(1))
            .block_number(21_000_000)
            .timestamp(1_737_000_000)
            .amount(U256::ZERO)
            .from(address(1))
            .to(to)
            .data(Bytes::from_static(&TRANSFER_SELECTOR))
            .build()
    }

    fn erc20_of(interaction: &Interaction) -> (EthAddress, EthAddress, U256) {
        match interaction.kind() {
            InteractionKind::ContractInteraction {
                contract_interaction_type:
                    ContractAction::Erc20Transfer {
                        from, to, amount, ..
                    },
                ..
            } => (*from, *to, *amount),
            other => panic!("expected an erc20 transfer, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn every_transfer_log_becomes_its_own_interaction() {
        let (token, alice, bob) = (address(9), address(1), address(2));
        let receipt = EthReceipt::new(
            true,
            None,
            vec![
                transfer_log(token, alice, bob, 10),
                transfer_log(token, bob, alice, 20),
            ],
        );

        let classificator = FulliestEthTxClassificator::new();
        let interactions = classificator
            .classificate(MinedTx::new(call_tx(token), receipt))
            .await
            .unwrap();

        assert_eq!(interactions.len(), 2);
        assert_eq!(erc20_of(&interactions[0]), (alice, bob, U256::from(10)));
        assert_eq!(erc20_of(&interactions[1]), (bob, alice, U256::from(20)));
    }

    #[tokio::test]
    async fn a_log_that_is_not_a_transfer_never_makes_an_edge() {
        let (token, alice, bob) = (address(9), address(1), address(2));
        let noise = EthLog::new(token, vec![B256::with_last_byte(7)], Bytes::new());
        let receipt = EthReceipt::new(true, None, vec![noise, transfer_log(token, alice, bob, 10)]);

        let classificator = FulliestEthTxClassificator::new();
        let interactions = classificator
            .classificate(MinedTx::new(call_tx(token), receipt))
            .await
            .unwrap();

        assert_eq!(interactions.len(), 1);
        assert_eq!(erc20_of(&interactions[0]), (alice, bob, U256::from(10)));
    }

    #[tokio::test]
    async fn a_logless_call_stays_a_single_other_interaction() {
        let classificator = FulliestEthTxClassificator::new();
        let interactions = classificator
            .classificate(MinedTx::new(
                call_tx(address(9)),
                EthReceipt::new(true, None, Vec::new()),
            ))
            .await
            .unwrap();

        assert_eq!(interactions.len(), 1);
        assert!(matches!(
            interactions[0].kind(),
            InteractionKind::ContractInteraction {
                contract_interaction_type: ContractAction::Other,
                ..
            }
        ));
    }
}
