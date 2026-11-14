use reqwest::Url;

pub const SEPOLIA: u64 = 11_155_111;
pub const HOODI: u64 = 560_048;
pub const ETHEREUM_TESTNETS: [u64; 2] = [SEPOLIA, HOODI];

pub const ANVIL: u64 = 31_337;
pub const GETH_DEV: u64 = 1_337;
pub const KNOWN_DEVNETS: [u64; 2] = [ANVIL, GETH_DEV];

const SEPOLIA_NODES: &[&str] = &[
    "https://ethereum-sepolia-rpc.publicnode.com",
    "https://sepolia.drpc.org",
    "https://rpc.sepolia.org",
];
const HOODI_NODES: &[&str] = &[
    "https://ethereum-hoodi-rpc.publicnode.com",
    "https://hoodi.drpc.org",
];

pub fn public_nodes(chain_id: u64) -> Vec<Url> {
    let nodes = match chain_id {
        SEPOLIA => SEPOLIA_NODES,
        HOODI => HOODI_NODES,
        _ => &[],
    };
    nodes
        .iter()
        .filter_map(|url| Url::parse(url).ok())
        .collect()
}

pub fn known_name(chain_id: u64) -> Option<&'static str> {
    match chain_id {
        SEPOLIA => Some("Sepolia"),
        HOODI => Some("Hoodi"),
        ANVIL => Some("Anvil devnet"),
        GETH_DEV => Some("Geth devnet"),
        crate::eth::tron::TRON_NILE => Some("Tron Nile"),
        crate::eth::tron::TRON_SHASTA => Some("Tron Shasta"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ethereum_testnet_has_public_nodes_and_a_name() {
        for chain_id in ETHEREUM_TESTNETS {
            assert!(!public_nodes(chain_id).is_empty());
            assert!(known_name(chain_id).is_some());
        }
        assert!(public_nodes(ANVIL).is_empty());
    }
}
