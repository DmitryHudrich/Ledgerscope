use serde::Serialize;

use application::eth::GraphSnapshot;
use domain::eth::{ContractAction, EthAddress, Interaction, InteractionEdge, InteractionKind};

#[derive(Serialize)]
pub struct GraphResponse {
    nodes: Vec<String>,
    edges: Vec<EdgeResponse>,
}

impl GraphResponse {
    pub fn new(nodes: Vec<String>, edges: Vec<EdgeResponse>) -> Self {
        Self { nodes, edges }
    }
}

#[derive(Serialize)]
pub struct EdgeResponse {
    tx_hash: String,
    block_number: u64,
    timestamp: u64,
    succeeded: bool,

    #[serde(flatten)]
    interaction: InteractionResponse,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionResponse {
    Protocol,
    NativeTransfer {
        from: String,
        to: String,
        amount: String,
    },
    ContractDeployment {
        deployer: String,
        contract_address: String,
    },
    ContractInteraction {
        interactor: String,
        contract_address: String,
        action: ContractActionResponse,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContractActionResponse {
    Erc20Transfer {
        token: String,
        from: String,
        to: String,
        amount: String,
        token_name: String,
        decimals: u8,
    },
    Other,
}

impl From<GraphSnapshot> for GraphResponse {
    fn from(snapshot: GraphSnapshot) -> Self {
        Self::new(
            snapshot.nodes().iter().map(EthAddress::to_string).collect(),
            snapshot.edges().iter().map(EdgeResponse::from).collect(),
        )
    }
}

impl From<&InteractionEdge> for EdgeResponse {
    fn from(edge: &InteractionEdge) -> Self {
        let meta = edge.meta();
        Self {
            tx_hash: meta.tx_hash().to_string(),
            block_number: meta.block_number(),
            timestamp: meta.timestamp(),
            succeeded: edge.interaction().receipt().succeeded(),
            interaction: edge.interaction().into(),
        }
    }
}

impl From<&Interaction> for InteractionResponse {
    fn from(interaction: &Interaction) -> Self {
        interaction.kind().into()
    }
}

impl From<&InteractionKind> for InteractionResponse {
    fn from(kind: &InteractionKind) -> Self {
        match kind {
            InteractionKind::Protocol => Self::Protocol,
            InteractionKind::NativeTransfer(transfer) => Self::NativeTransfer {
                from: transfer.from().to_string(),
                to: transfer.to().to_string(),
                amount: transfer.amount().to_string(),
            },
            InteractionKind::ContractDeployment {
                contract_address,
                deployer,
            } => Self::ContractDeployment {
                deployer: deployer.to_string(),
                contract_address: contract_address.to_string(),
            },
            InteractionKind::ContractInteraction {
                contract_address,
                interactor,
                contract_interaction_type,
            } => Self::ContractInteraction {
                interactor: interactor.to_string(),
                contract_address: contract_address.to_string(),
                action: contract_interaction_type.into(),
            },
        }
    }
}

impl From<&ContractAction> for ContractActionResponse {
    fn from(action: &ContractAction) -> Self {
        match action {
            ContractAction::Erc20Transfer {
                token,
                from,
                to,
                amount,
                token_name,
                decimals,
            } => Self::Erc20Transfer {
                token: token.to_string(),
                from: from.to_string(),
                to: to.to_string(),
                amount: amount.to_string(),
                token_name: token_name.clone(),
                decimals: *decimals,
            },
            ContractAction::Other => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Bytes, U256, b256};
    use domain::eth::{EthReceipt, EthTx, TxMeta, graph::NativeTransfer};
    use serde_json::json;

    #[test]
    fn wire_shape() {
        let tx = EthTx::builder()
            .tx_hash(b256!(
                "0xabababababababababababababababababababababababababababababababab"
            ))
            .block_number(21_000_000)
            .timestamp(1_737_000_000)
            .amount(U256::from(1_500_000_000_000_000_000u128))
            .from(
                "0x1111111111111111111111111111111111111111"
                    .parse()
                    .unwrap(),
            )
            .to("0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap())
            .data(Bytes::new())
            .build();
        let meta = TxMeta::from(&tx);
        let receipt = EthReceipt::new(true, None, Vec::new());

        let native = Interaction::new(
            receipt.clone(),
            InteractionKind::NativeTransfer(NativeTransfer::try_from(tx).ok().unwrap()),
        );

        let erc20 = Interaction::new(
            receipt,
            InteractionKind::ContractInteraction {
                contract_address: "0x3333333333333333333333333333333333333333"
                    .parse()
                    .unwrap(),
                interactor: "0x1111111111111111111111111111111111111111"
                    .parse()
                    .unwrap(),
                contract_interaction_type: ContractAction::Erc20Transfer {
                    token: "0x3333333333333333333333333333333333333333"
                        .parse()
                        .unwrap(),
                    from: "0x1111111111111111111111111111111111111111"
                        .parse()
                        .unwrap(),
                    to: "0x2222222222222222222222222222222222222222"
                        .parse()
                        .unwrap(),
                    amount: U256::from(1_000_000),
                    token_name: "USDC".to_owned(),
                    decimals: 6,
                },
            },
        );

        let response = GraphResponse::new(
            vec!["0x1111111111111111111111111111111111111111".to_owned()],
            vec![
                EdgeResponse::from(&InteractionEdge::new(meta.clone(), native)),
                EdgeResponse::from(&InteractionEdge::new(meta, erc20)),
            ],
        );
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            json!({
                "nodes": ["0x1111111111111111111111111111111111111111"],
                "edges": [
                    {
                        "tx_hash": "0xabababababababababababababababababababababababababababababababab",
                        "block_number": 21_000_000,
                        "timestamp": 1_737_000_000,
                        "succeeded": true,
                        "kind": "native_transfer",
                        "from": "0x1111111111111111111111111111111111111111",
                        "to": "0x2222222222222222222222222222222222222222",
                        "amount": "1500000000000000000"
                    },
                    {
                        "tx_hash": "0xabababababababababababababababababababababababababababababababab",
                        "block_number": 21_000_000,
                        "timestamp": 1_737_000_000,
                        "succeeded": true,
                        "kind": "contract_interaction",
                        "interactor": "0x1111111111111111111111111111111111111111",
                        "contract_address": "0x3333333333333333333333333333333333333333",
                        "action": {
                            "kind": "erc20_transfer",
                            "token": "0x3333333333333333333333333333333333333333",
                            "from": "0x1111111111111111111111111111111111111111",
                            "to": "0x2222222222222222222222222222222222222222",
                            "amount": "1000000",
                            "token_name": "USDC",
                            "decimals": 6
                        }
                    }
                ]
            })
        );
    }
}
