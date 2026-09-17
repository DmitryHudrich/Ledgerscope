use std::{io, str::FromStr};

use serde::{Deserialize, Serialize};

use domain::eth::{Actor, ActorKind, ContractKind, EthAddress};

const EOA: &str = "eoa";
const CONTRACT: &str = "contract";
const ERC20: &str = "erc20";

#[derive(Debug, Serialize, Deserialize)]
pub struct ActorRow {
    pub address: String,
    pub kind: String,
    pub symbol: String,
    pub decimals: u8,
}

impl ActorRow {
    pub fn from_actor(actor: &Actor) -> Option<Self> {
        let (kind, symbol, decimals) = match actor.kind() {
            ActorKind::Eoa => (EOA, String::new(), 0),
            ActorKind::Contract(ContractKind::Plain) => (CONTRACT, String::new(), 0),
            ActorKind::Contract(ContractKind::Erc20 { symbol, decimals }) => {
                (ERC20, symbol.clone(), *decimals)
            }
            ActorKind::Unknown => return None,
        };

        Some(Self {
            address: actor.address().to_string(),
            kind: kind.to_owned(),
            symbol,
            decimals,
        })
    }

    pub fn into_actor(self) -> Result<Actor, io::Error> {
        let address = EthAddress::from_str(&self.address).map_err(|error| {
            io::Error::other(format!("clickhouse holds a broken actor address: {error}"))
        })?;

        let kind = match self.kind.as_str() {
            EOA => ActorKind::Eoa,
            CONTRACT => ActorKind::Contract(ContractKind::Plain),
            ERC20 => ActorKind::Contract(ContractKind::Erc20 {
                symbol: self.symbol,
                decimals: self.decimals,
            }),
            other => {
                return Err(io::Error::other(format!(
                    "clickhouse calls {} a {other}, which is nothing we know",
                    self.address
                )));
            }
        };

        Ok(Actor::new(address, kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address() -> EthAddress {
        EthAddress::from([9u8; 20])
    }

    fn round_trip(kind: ActorKind) -> Actor {
        ActorRow::from_actor(&Actor::new(address(), kind))
            .expect("this actor is worth writing down")
            .into_actor()
            .expect("what we wrote must read back")
    }

    #[test]
    fn a_token_keeps_its_name_through_the_database() {
        let actor = round_trip(ActorKind::Contract(ContractKind::Erc20 {
            symbol: "USDC".to_owned(),
            decimals: 6,
        }));

        assert_eq!(actor.erc20(), Some(("USDC", 6)));
        assert_eq!(actor.address(), &address());
    }

    #[test]
    fn a_wallet_survives_the_round_trip() {
        assert_eq!(round_trip(ActorKind::Eoa).kind(), &ActorKind::Eoa);
    }

    #[test]
    fn a_plain_contract_survives_the_round_trip() {
        let actor = round_trip(ActorKind::Contract(ContractKind::Plain));

        assert!(actor.is_contract());
        assert_eq!(actor.erc20(), None);
    }

    #[test]
    fn an_unknown_actor_is_never_written_down() {
        assert!(ActorRow::from_actor(&Actor::unknown(address())).is_none());
    }

    #[test]
    fn a_kind_we_never_wrote_is_refused() {
        let row = ActorRow {
            address: address().to_string(),
            kind: "oracle".to_owned(),
            symbol: String::new(),
            decimals: 0,
        };

        assert!(row.into_actor().is_err());
    }
}
