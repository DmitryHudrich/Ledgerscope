use serde::Serialize;

use application::eth::GraphSnapshot;
use domain::eth::{ContractAction, EthAddress, Interaction, InteractionEdge};

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
        from: String,
        to: String,
        amount: String,
        token_name: String,
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
            interaction: edge.interaction().into(),
        }
    }
}

impl From<&Interaction> for InteractionResponse {
    fn from(interaction: &Interaction) -> Self {
        match interaction {
            Interaction::Protocol => Self::Protocol,
            Interaction::NativeTransfer(transfer) => Self::NativeTransfer {
                from: transfer.from().to_string(),
                to: transfer.to().to_string(),
                amount: transfer.amount().to_string(),
            },
            Interaction::ContractDeployment {
                contract_address,
                deployer,
            } => Self::ContractDeployment {
                deployer: deployer.to_string(),
                contract_address: contract_address.to_string(),
            },
            Interaction::ContractInteraction {
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
                from,
                to,
                amount,
                token_name,
            } => Self::Erc20Transfer {
                from: from.to_string(),
                to: to.to_string(),
                amount: amount.to_string(),
                token_name: token_name.clone(),
            },
            ContractAction::Other => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::eth::{EthTx, TxMeta, graph::NativeTransfer};
    use serde_json::json;

    #[test]
    fn wire_shape() {
        let tx = EthTx::builder()
            .tx_hash("0xabc".to_owned())
            .block_number(21_000_000)
            .timestamp(1_737_000_000)
            .amount(1_500_000_000_000_000_000u128)
            .from(
                "0x1111111111111111111111111111111111111111"
                    .parse()
                    .unwrap(),
            )
            .to("0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap())
            .data(vec![])
            .build();
        let meta = TxMeta::from(&tx);
        let native = Interaction::NativeTransfer(NativeTransfer::try_from(tx).ok().unwrap());

        let erc20 = Interaction::ContractInteraction {
            contract_address: "0x3333333333333333333333333333333333333333"
                .parse()
                .unwrap(),
            interactor: "0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            contract_interaction_type: ContractAction::Erc20Transfer {
                from: "0x1111111111111111111111111111111111111111"
                    .parse()
                    .unwrap(),
                to: "0x2222222222222222222222222222222222222222"
                    .parse()
                    .unwrap(),
                amount: 1_000_000,
                token_name: "USDC".to_owned(),
            },
        };

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
                        "tx_hash": "0xabc",
                        "block_number": 21_000_000,
                        "timestamp": 1_737_000_000,
                        "kind": "native_transfer",
                        "from": "0x1111111111111111111111111111111111111111",
                        "to": "0x2222222222222222222222222222222222222222",
                        "amount": "1500000000000000000"
                    },
                    {
                        "tx_hash": "0xabc",
                        "block_number": 21_000_000,
                        "timestamp": 1_737_000_000,
                        "kind": "contract_interaction",
                        "interactor": "0x1111111111111111111111111111111111111111",
                        "contract_address": "0x3333333333333333333333333333333333333333",
                        "action": {
                            "kind": "erc20_transfer",
                            "from": "0x1111111111111111111111111111111111111111",
                            "to": "0x2222222222222222222222222222222222222222",
                            "amount": "1000000",
                            "token_name": "USDC"
                        }
                    }
                ]
            })
        );
    }
}
