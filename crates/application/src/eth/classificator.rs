use std::{fmt, io, sync::Arc};

use alloy_primitives::{B256, U256, b256};

use domain::eth::{
    EthAddress, EthLog, MinedTx, Swap,
    graph::{AssetTransfer, ContractAction, Interaction, InteractionKind, NativeTransfer},
};

use crate::eth::{ports::PoolRegistry, swaps::decode_swap};

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
pub struct FulliestEthTxClassificator {
    pools: Option<Arc<dyn PoolRegistry>>,
}

impl FulliestEthTxClassificator {
    pub fn new() -> Self {
        Self { pools: None }
    }

    pub fn with_pools(pools: Arc<dyn PoolRegistry>) -> Self {
        Self { pools: Some(pools) }
    }

    async fn swaps_of(&self, receipt: &domain::eth::EthReceipt) -> Vec<Swap> {
        let Some(pools) = self.pools.as_ref() else {
            return Vec::new();
        };

        let decoded: Vec<_> = receipt.logs().iter().filter_map(decode_swap).collect();
        if decoded.is_empty() {
            return Vec::new();
        }

        let candidates: Vec<_> = decoded.iter().map(|swap| *swap.candidate()).collect();
        let known = pools.resolve(&candidates).await;

        decoded
            .iter()
            .filter_map(|swap| {
                let pool = known.get(swap.candidate().key())?;
                let oriented = swap.flow().orient(pool.tokens())?;

                Some(Swap::new(
                    pool.clone(),
                    oriented.token_in,
                    oriented.amount_in,
                    oriented.token_out,
                    oriented.amount_out,
                ))
            })
            .collect()
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

        if let Some(action) = tx.action() {
            return Ok(vec![Interaction::new(
                receipt,
                InteractionKind::SystemAction {
                    action: action.to_owned(),
                    from: *tx.from(),
                    to: tx.to().copied(),
                    amount: tx.amount(),
                },
            )]);
        }

        if tx.asset().is_some() {
            let transfer =
                AssetTransfer::try_from(tx).map_err(|_| ClassificateError::InvariantNarushen)?;

            return Ok(vec![Interaction::new(
                receipt,
                InteractionKind::AssetTransfer(transfer),
            )]);
        }

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

        let swaps = self.swaps_of(&receipt).await;

        if transfers.is_empty() && swaps.is_empty() {
            return Ok(vec![Interaction::new(
                receipt,
                InteractionKind::ContractInteraction {
                    contract_address,
                    interactor,
                    contract_interaction_type: ContractAction::Other,
                },
            )]);
        }

        let transfer_actions =
            transfers
                .into_iter()
                .map(|transfer| ContractAction::Erc20Transfer {
                    token: transfer.token,
                    from: transfer.from,
                    to: transfer.to,
                    amount: transfer.amount,
                });
        let swap_actions = swaps.into_iter().map(ContractAction::Swap);

        let interactions = transfer_actions
            .chain(swap_actions)
            .map(|action| {
                Interaction::new(
                    receipt.clone(),
                    InteractionKind::ContractInteraction {
                        contract_address,
                        interactor,
                        contract_interaction_type: action,
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
    use domain::eth::{EthLog, EthReceipt, EthTx, IssuedAsset};

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

    fn plain_tx(asset: Option<IssuedAsset>, action: Option<&str>) -> EthTx {
        EthTx::builder()
            .tx_hash(TxHash::with_last_byte(2))
            .block_number(87_561_000)
            .timestamp(1_793_356_395)
            .amount(U256::from(4_444_u64))
            .from(address(1))
            .to(address(2))
            .data(Bytes::new())
            .maybe_asset(asset)
            .maybe_action(action.map(str::to_owned))
            .build()
    }

    #[tokio::test]
    async fn an_issued_asset_moves_as_its_own_transfer() {
        let asset = IssuedAsset::new("1005193".to_owned(), "USDD".to_owned(), 6);
        let interactions = FulliestEthTxClassificator::new()
            .classificate(MinedTx::new(
                plain_tx(Some(asset.clone()), None),
                EthReceipt::new(true, None, Vec::new()),
            ))
            .await
            .unwrap();

        match interactions[0].kind() {
            InteractionKind::AssetTransfer(transfer) => {
                assert_eq!(transfer.asset(), &asset);
                assert_eq!(transfer.amount(), U256::from(4_444_u64));
            }
            other => panic!("expected an asset transfer, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_system_action_is_never_mistaken_for_a_payment() {
        let interactions = FulliestEthTxClassificator::new()
            .classificate(MinedTx::new(
                plain_tx(None, Some("DelegateResourceContract")),
                EthReceipt::new(true, None, Vec::new()),
            ))
            .await
            .unwrap();

        assert!(matches!(
            interactions[0].kind(),
            InteractionKind::SystemAction { action, to: Some(_), .. } if action == "DelegateResourceContract"
        ));
    }

    struct FixedPools(Vec<domain::eth::PoolInfo>);

    #[async_trait::async_trait]
    impl PoolRegistry for FixedPools {
        async fn resolve(
            &self,
            wanted: &[domain::eth::PoolCandidate],
        ) -> std::collections::HashMap<domain::eth::PoolKey, domain::eth::PoolInfo> {
            self.0
                .iter()
                .filter(|pool| wanted.iter().any(|candidate| candidate.key() == pool.key()))
                .map(|pool| (*pool.key(), pool.clone()))
                .collect()
        }
    }

    fn v2_pool(pool: EthAddress) -> domain::eth::PoolInfo {
        domain::eth::PoolInfo::new(
            domain::eth::PoolKey::new(pool, None),
            domain::eth::SwapFamily::UniswapV2,
            "Uniswap V2".to_owned(),
            vec![address(10), address(11)],
            Some(3000),
            Some(address(99)),
            domain::eth::TagSource::heuristic("pool_registry.uniswap_v2", 1),
        )
    }

    fn v2_swap_log(pool: EthAddress, amounts: [u64; 4]) -> EthLog {
        let data: Vec<u8> = amounts
            .iter()
            .flat_map(|amount| U256::from(*amount).to_be_bytes::<32>())
            .collect();

        EthLog::new(
            pool,
            vec![
                *crate::eth::swaps::V2_SWAP,
                address(1).address().into_word(),
                address(2).address().into_word(),
            ],
            Bytes::from(data),
        )
    }

    #[tokio::test]
    async fn a_verified_swap_is_appended_after_the_transfers() {
        let pool = address(7);
        let receipt = EthReceipt::new(
            true,
            None,
            vec![
                transfer_log(address(10), address(1), pool, 100),
                transfer_log(address(11), pool, address(1), 40),
                v2_swap_log(pool, [100, 0, 0, 40]),
            ],
        );

        let classificator =
            FulliestEthTxClassificator::with_pools(Arc::new(FixedPools(vec![v2_pool(pool)])));
        let interactions = classificator
            .classificate(MinedTx::new(call_tx(address(9)), receipt))
            .await
            .unwrap();

        assert_eq!(interactions.len(), 3);
        assert_eq!(
            erc20_of(&interactions[0]),
            (address(1), pool, U256::from(100))
        );
        assert_eq!(
            erc20_of(&interactions[1]),
            (pool, address(1), U256::from(40))
        );
        match interactions[2].kind() {
            InteractionKind::ContractInteraction {
                interactor,
                contract_interaction_type: ContractAction::Swap(swap),
                ..
            } => {
                assert_eq!(*interactor, address(1));
                assert_eq!(swap.token_in(), &address(10));
                assert_eq!(swap.amount_in(), U256::from(100));
                assert_eq!(swap.token_out(), &address(11));
                assert_eq!(swap.amount_out(), U256::from(40));
            }
            other => panic!("expected a swap, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_swap_event_of_an_unverified_pool_makes_no_edge() {
        let pool = address(7);
        let receipt = EthReceipt::new(true, None, vec![v2_swap_log(pool, [100, 0, 0, 40])]);

        let classificator =
            FulliestEthTxClassificator::with_pools(Arc::new(FixedPools(Vec::new())));
        let interactions = classificator
            .classificate(MinedTx::new(call_tx(address(9)), receipt))
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

    #[tokio::test]
    async fn without_a_registry_swaps_are_left_alone() {
        let pool = address(7);
        let receipt = EthReceipt::new(
            true,
            None,
            vec![
                transfer_log(address(10), address(1), pool, 100),
                v2_swap_log(pool, [100, 0, 0, 40]),
            ],
        );

        let interactions = FulliestEthTxClassificator::new()
            .classificate(MinedTx::new(call_tx(address(9)), receipt))
            .await
            .unwrap();

        assert_eq!(interactions.len(), 1);
    }
}
