use alloy_primitives::TxHash;

use crate::eth::EthAddress;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TagSubject {
    Address(EthAddress),
    Tx(TxHash),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SourceKind {
    Heuristic,
    Provider,
    User,
    Agent,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Heuristic => "heuristic",
            SourceKind::Provider => "provider",
            SourceKind::User => "user",
            SourceKind::Agent => "agent",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "heuristic" => Some(SourceKind::Heuristic),
            "provider" => Some(SourceKind::Provider),
            "user" => Some(SourceKind::User),
            "agent" => Some(SourceKind::Agent),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TagSource {
    kind: SourceKind,
    name: String,
    version: u32,
}

impl TagSource {
    pub fn new(kind: SourceKind, name: impl Into<String>, version: u32) -> Self {
        Self {
            kind,
            name: name.into(),
            version,
        }
    }

    pub fn heuristic(name: impl Into<String>, version: u32) -> Self {
        Self::new(SourceKind::Heuristic, name, version)
    }

    pub fn kind(&self) -> SourceKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn version(&self) -> u32 {
        self.version
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    subject: TagSubject,
    key: String,
    confidence: f32,
    source: TagSource,
    evidence: Vec<TxHash>,
    note: String,
}

impl Tag {
    pub fn new(subject: TagSubject, key: impl Into<String>, source: TagSource) -> Self {
        Self {
            subject,
            key: key.into(),
            confidence: 1.0,
            source,
            evidence: Vec::new(),
            note: String::new(),
        }
    }

    pub fn with_confidence(self, confidence: f32) -> Self {
        Self {
            confidence: confidence.clamp(0.0, 1.0),
            ..self
        }
    }

    pub fn with_evidence(self, evidence: Vec<TxHash>) -> Self {
        Self { evidence, ..self }
    }

    pub fn with_note(self, note: impl Into<String>) -> Self {
        Self {
            note: note.into(),
            ..self
        }
    }

    pub fn subject(&self) -> &TagSubject {
        &self.subject
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn category(&self) -> &str {
        self.key.split('.').next().unwrap_or(&self.key)
    }

    pub fn confidence(&self) -> f32 {
        self.confidence
    }

    pub fn source(&self) -> &TagSource {
        &self.source
    }

    pub fn evidence(&self) -> &[TxHash] {
        &self.evidence
    }

    pub fn note(&self) -> &str {
        &self.note
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_category_is_the_first_part_of_the_key() {
        let tag = Tag::new(
            TagSubject::Tx(TxHash::ZERO),
            "gas.top_up",
            TagSource::heuristic("gas_top_up", 1),
        );

        assert_eq!(tag.category(), "gas");
    }

    #[test]
    fn confidence_stays_within_one() {
        let tag = Tag::new(
            TagSubject::Tx(TxHash::ZERO),
            "gas.top_up",
            TagSource::heuristic("gas_top_up", 1),
        )
        .with_confidence(3.0);

        assert_eq!(tag.confidence(), 1.0);
    }
}
