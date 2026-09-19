pub mod actor;
pub mod classificator;
pub mod explorer;
pub mod index;
pub mod lowlevel;
pub mod ports;
pub mod store;
pub mod etherscan_json_labels {
    use std::{collections::HashMap, io, path::Path, str::FromStr};

    use domain::eth::{AddressLabel, EthAddress};
    use serde::Deserialize;

    use crate::eth::LabelProvider;

    const SOURCE: &str = "etherscan-json";

    #[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DeserializedLabel {
        address: String,
        chain_id: u64,
        label: Option<String>,
        name_tag: Option<String>,
    }

    pub struct JsonLabelProvider {
        data: HashMap<EthAddress, Vec<AddressLabel>>,
    }

    impl JsonLabelProvider {
        pub fn read_all(path: &Path, chain_id: u64) -> Result<Self, io::Error> {
            tracing::info!("begin read labels from '{}'", path.display());
            let all_labels = std::fs::read_to_string(path)?;
            let deserialized: Vec<DeserializedLabel> =
                serde_json::from_str(&all_labels).map_err(|error| {
                    io::Error::other(format!("{} is not a label file: {error}", path.display()))
                })?;

            let mut data: HashMap<EthAddress, Vec<AddressLabel>> = HashMap::new();
            let mut skipped = 0usize;
            for entry in deserialized {
                if entry.chain_id != chain_id {
                    continue;
                }

                let Ok(address) = EthAddress::from_str(&entry.address) else {
                    skipped += 1;
                    continue;
                };

                let labels = data.entry(address).or_default();
                for value in [entry.name_tag, entry.label].into_iter().flatten() {
                    if value.is_empty() {
                        continue;
                    }

                    let label = AddressLabel::new(value, SOURCE.to_owned());
                    if !labels.contains(&label) {
                        labels.push(label);
                    }
                }
            }

            if skipped > 0 {
                tracing::warn!("{skipped} labels have an unreadable address and were dropped");
            }

            tracing::info!(
                "read labels for {} addresses of chain {chain_id}",
                data.len()
            );

            Ok(Self { data })
        }
    }

    #[async_trait::async_trait]
    impl LabelProvider for JsonLabelProvider {
        fn source(&self) -> &str {
            SOURCE
        }

        async fn fetch_labels_for(
            &self,
            addresses: &[EthAddress],
        ) -> Result<HashMap<EthAddress, Vec<AddressLabel>>, io::Error> {
            Ok(addresses
                .iter()
                .map(|address| {
                    (
                        *address,
                        self.data.get(address).cloned().unwrap_or_default(),
                    )
                })
                .collect())
        }
    }
}

pub use actor::CachingActorResolver;
pub use etherscan_json_labels::JsonLabelProvider;
pub use explorer::{
    AddressGraph, EthExplorer, Exploration, ExploreError, ExploreLimits, ExploreRequest, GraphNode,
    GraphRoot, RpcPlan,
};
pub use index::FetchingTxIndex;
pub use lowlevel::{LowLevelGraph, LowLevelNode};
pub use ports::{
    ActorRepository, ActorResolver, EthRpcSource, EthTxCache, EthTxIndex, EthTxRepository,
    EthTxSource, LabelProvider,
};
pub use store::StoringEthTxSource;
