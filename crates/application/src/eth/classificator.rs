use std::{collections::HashMap, fmt, io, sync::Arc};

use alloy_primitives::{B256, Bytes, U256, b256};
use tokio::sync::{Mutex, OnceCell};

use domain::eth::{
    BlockRef, EthAddress, EthLog, MinedTx,
    graph::{ContractAction, Interaction, InteractionKind, NativeTransfer},
};

use crate::eth::EthRpcSource;

const TRANSFER_TOPIC_SIGNATURE: B256 =
    b256!("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");

const SYMBOL_SELECTOR: [u8; 4] = [0x95, 0xd8, 0x9b, 0x41];

const DECIMALS_SELECTOR: [u8; 4] = [0x31, 0x3c, 0xe5, 0x67];

const DEFAULT_DECIMALS: u8 = 18;

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

struct AskAgainLater;

#[derive(Clone)]
struct TokenMeta {
    symbol: String,
    decimals: u8,
}

pub struct FulliestEthTxClassificator {
    rpc_service: Arc<dyn EthRpcSource>,
    token_meta: Mutex<HashMap<EthAddress, Arc<OnceCell<TokenMeta>>>>,
}

impl FulliestEthTxClassificator {
    pub fn new(rpc_service: Arc<dyn EthRpcSource>) -> Self {
        Self {
            rpc_service,
            token_meta: Mutex::new(HashMap::new()),
        }
    }

    async fn token_meta(&self, token: &EthAddress) -> TokenMeta {
        let meta = {
            let mut token_meta = self.token_meta.lock().await;
            token_meta.entry(*token).or_default().clone()
        };

        match meta.get_or_try_init(|| self.ask_token_meta(token)).await {
            Ok(meta) => meta.clone(),
            Err(AskAgainLater) => TokenMeta {
                symbol: token.to_string(),
                decimals: DEFAULT_DECIMALS,
            },
        }
    }

    async fn ask_token_meta(&self, token: &EthAddress) -> Result<TokenMeta, AskAgainLater> {
        let (symbol, decimals) = tokio::join!(
            self.ask_token_symbol(token),
            self.ask_token_decimals(token)
        );

        Ok(TokenMeta {
            symbol: symbol?,
            decimals: decimals?,
        })
    }

    async fn ask_token_symbol(&self, token: &EthAddress) -> Result<String, AskAgainLater> {
        let returned = self
            .rpc_service
            .call(token, &SYMBOL_SELECTOR, BlockRef::Latest)
            .await;

        match returned {
            Ok(returned) => Ok(decode_symbol(&returned).unwrap_or_else(|| token.to_string())),
            Err(error) if answered_with_revert(&error) => {
                tracing::warn!("{token} has no symbol(), naming it by address from now on");
                Ok(token.to_string())
            }
            Err(error) => {
                tracing::warn!(
                    "symbol() on {token} failed, naming it by address this time: {error}"
                );
                Err(AskAgainLater)
            }
        }
    }

    async fn ask_token_decimals(&self, token: &EthAddress) -> Result<u8, AskAgainLater> {
        let returned = self
            .rpc_service
            .call(token, &DECIMALS_SELECTOR, BlockRef::Latest)
            .await;

        match returned {
            Ok(returned) => Ok(decode_decimals(&returned).unwrap_or(DEFAULT_DECIMALS)),
            Err(error) if answered_with_revert(&error) => {
                tracing::warn!("{token} has no decimals(), assuming {DEFAULT_DECIMALS} from now on");
                Ok(DEFAULT_DECIMALS)
            }
            Err(error) => {
                tracing::warn!(
                    "decimals() on {token} failed, assuming {DEFAULT_DECIMALS} this time: {error}"
                );
                Err(AskAgainLater)
            }
        }
    }
}

fn answered_with_revert(error: &io::Error) -> bool {
    error.to_string().contains("execution reverted")
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

fn decode_decimals(returned: &Bytes) -> Option<u8> {
    let word = returned.get(..32)?;
    u8::try_from(U256::from_be_slice(word)).ok().filter(|d| *d <= 36)
}

#[async_trait::async_trait]
pub trait EthTxClassificator: Send + Sync {
    async fn classificate(&self, mined: MinedTx) -> Result<Interaction, ClassificateError>;
}

#[async_trait::async_trait]
impl EthTxClassificator for FulliestEthTxClassificator {
    async fn classificate(&self, mined: MinedTx) -> Result<Interaction, ClassificateError> {
        let (tx, receipt) = mined.into_parts();

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
            Some(transfer) => {
                let meta = self.token_meta(&transfer.token).await;
                ContractAction::Erc20Transfer {
                    token: transfer.token,
                    from: transfer.from,
                    to: transfer.to,
                    amount: transfer.amount,
                    token_name: meta.symbol,
                    decimals: meta.decimals,
                }
            }
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
