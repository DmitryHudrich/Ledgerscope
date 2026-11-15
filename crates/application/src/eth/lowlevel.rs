use domain::eth::{BlockRange, EthAddress, LowLevelActor, LowLevelInteraction};

#[derive(Clone, Debug)]
pub struct LowLevelNode {
    actor: LowLevelActor,
    depth: u32,
    root: bool,
    expanded: bool,
}

impl LowLevelNode {
    pub fn new(actor: LowLevelActor, depth: u32, root: bool, expanded: bool) -> Self {
        Self {
            actor,
            depth,
            root,
            expanded,
        }
    }

    pub fn actor(&self) -> &LowLevelActor {
        &self.actor
    }

    pub fn address(&self) -> &EthAddress {
        self.actor.address()
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    pub fn root(&self) -> bool {
        self.root
    }

    pub fn expanded(&self) -> bool {
        self.expanded
    }
}

#[derive(Debug, Default)]
pub struct LowLevelGraph {
    pub(crate) actors: Vec<LowLevelNode>,
    pub(crate) interactions: Vec<LowLevelInteraction>,
    pub(crate) filled: Vec<BlockRange>,
    pub(crate) truncated: bool,
}

impl LowLevelGraph {
    pub fn actors(&self) -> &[LowLevelNode] {
        &self.actors
    }

    pub fn interactions(&self) -> &[LowLevelInteraction] {
        &self.interactions
    }

    pub fn filled(&self) -> &[BlockRange] {
        &self.filled
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }
}
