use crate::entity::RiskScore;
use crate::primitives::Confidence;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TagId(uuid::Uuid);

impl TagId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn from_uuid(id: uuid::Uuid) -> Self {
        Self(id)
    }

    pub fn value(&self) -> uuid::Uuid {
        self.0
    }
}

impl Default for TagId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagCategory {
    Exchange,
    Mixer,
    Bridge,
    DefiProtocol,
    Sanctioned,
    Scam,
    Gambling,
    Darknet,
    Mining,
    KnownService,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TagSource {
    OfacSdn,
    EuSanctions,
    UnSanctions,
    InternalAnalyst,
    HeuristicCluster,
    ThirdParty(String),
    LegacyImport,
}

#[derive(Debug, Clone)]
pub struct LabelTag {
    tag_id: TagId,
    category: TagCategory,
    label_name: Option<String>,
    source: TagSource,
    confidence: Confidence,
    risk_score: RiskScore,
    sanction_list: Option<String>,
    active: bool,
    superseded_by: Option<TagId>,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    evidence_url: Option<String>,
}

#[allow(clippy::too_many_arguments)]
impl LabelTag {
    pub fn new(
        category: TagCategory,
        label_name: Option<String>,
        source: TagSource,
        confidence: Confidence,
        risk_score: RiskScore,
        sanction_list: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        evidence_url: Option<String>,
    ) -> Self {
        Self {
            tag_id: TagId::new(),
            category,
            label_name,
            source,
            confidence,
            risk_score,
            sanction_list,
            active: true,
            superseded_by: None,
            created_at: Utc::now(),
            expires_at,
            evidence_url,
        }
    }

    pub fn from_parts(
        tag_id: TagId,
        category: TagCategory,
        label_name: Option<String>,
        source: TagSource,
        confidence: Confidence,
        risk_score: RiskScore,
        sanction_list: Option<String>,
        active: bool,
        superseded_by: Option<TagId>,
        created_at: DateTime<Utc>,
        expires_at: Option<DateTime<Utc>>,
        evidence_url: Option<String>,
    ) -> Self {
        Self {
            tag_id,
            category,
            label_name,
            source,
            confidence,
            risk_score,
            sanction_list,
            active,
            superseded_by,
            created_at,
            expires_at,
            evidence_url,
        }
    }

    pub fn tag_id(&self) -> TagId {
        self.tag_id
    }

    pub fn category(&self) -> TagCategory {
        self.category
    }

    pub fn label_name(&self) -> Option<&str> {
        self.label_name.as_deref()
    }

    pub fn set_label_name(&mut self, name: Option<String>) {
        self.label_name = name;
    }

    pub fn source(&self) -> &TagSource {
        &self.source
    }

    pub fn confidence(&self) -> Confidence {
        self.confidence
    }

    pub fn set_confidence(&mut self, confidence: Confidence) {
        self.confidence = confidence;
    }

    pub fn risk_score(&self) -> RiskScore {
        self.risk_score
    }

    pub fn set_risk_score(&mut self, risk_score: RiskScore) {
        self.risk_score = risk_score;
    }

    pub fn sanction_list(&self) -> Option<&str> {
        self.sanction_list.as_deref()
    }

    pub fn set_sanction_list(&mut self, sanction_list: Option<String>) {
        self.sanction_list = sanction_list;
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Whether this tag counts as active *right now* — `active` flag AND
    /// (no `expires_at` or it hasn't passed yet). `expires_at` is filtered
    /// here rather than swept by a background job.
    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        self.active && self.expires_at.map(|exp| exp > now).unwrap_or(true)
    }

    pub fn superseded_by(&self) -> Option<TagId> {
        self.superseded_by
    }

    pub fn set_superseded_by(&mut self, tag_id: Option<TagId>) {
        self.superseded_by = tag_id;
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
    }

    pub fn set_expires_at(&mut self, expires_at: Option<DateTime<Utc>>) {
        self.expires_at = expires_at;
    }

    pub fn evidence_url(&self) -> Option<&str> {
        self.evidence_url.as_deref()
    }

    pub fn set_evidence_url(&mut self, url: Option<String>) {
        self.evidence_url = url;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagAction {
    Added,
    Updated,
    Deactivated,
    Reactivated,
    Expired,
    /// Old tag deactivated because a same-source, higher-confidence tag of
    /// the same category replaced it (see `TagAggregationStrategy` doc and
    /// the resolution rules in the labels usecase).
    Superseded,
}

#[derive(Debug, Clone)]
pub struct TagHistoryEvent {
    tag_id: TagId,
    action: TagAction,
    at: DateTime<Utc>,
    actor: TagSource,
    reason: Option<String>,
}

impl TagHistoryEvent {
    pub fn new(tag_id: TagId, action: TagAction, actor: TagSource, reason: Option<String>) -> Self {
        Self {
            tag_id,
            action,
            at: Utc::now(),
            actor,
            reason,
        }
    }

    pub fn from_parts(
        tag_id: TagId,
        action: TagAction,
        at: DateTime<Utc>,
        actor: TagSource,
        reason: Option<String>,
    ) -> Self {
        Self {
            tag_id,
            action,
            at,
            actor,
            reason,
        }
    }

    pub fn tag_id(&self) -> TagId {
        self.tag_id
    }

    pub fn action(&self) -> TagAction {
        self.action
    }

    pub fn at(&self) -> DateTime<Utc> {
        self.at
    }

    pub fn actor(&self) -> &TagSource {
        &self.actor
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

/// Strategy for collapsing an entity's active tags into one 0-100 score.
/// Computed lazily wherever needed (`Entity::aggregate_risk_score`) — never
/// persisted, so there's no risk of it going stale as tags are added or
/// deactivated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagAggregationStrategy {
    /// Maximum `risk_score` among active tags. Default.
    MaxActive,
    /// Combine active tags as independent evidence via a confidence-weighted
    /// noisy-OR (`1 − Π(1 − riskᵢ·confᵢ)`). Bounded in [0,100] without a hard
    /// clamp, monotonic, and — unlike a plain weighted sum — it does not
    /// saturate to 100 after a handful of medium-strength tags. A tag's
    /// confidence scales its contribution continuously (0-100), so a
    /// low-confidence tag can no longer swing the score as hard as a
    /// confirmed one.
    WeightedSum,
    /// Maximum `risk_score` among active tags at `Confidence::CERTAIN`
    /// ("Confirmed") confidence. `CLEAN` (0) if none exist.
    MaxConfirmedOnly,
}

impl Default for TagAggregationStrategy {
    fn default() -> Self {
        Self::MaxActive
    }
}

/// Aggregate `tags` (only entries active at `now` are considered) per
/// `strategy`. Pure function — no I/O, safe to call on every read.
pub fn aggregate_risk_score(
    tags: &[LabelTag],
    strategy: TagAggregationStrategy,
    now: DateTime<Utc>,
) -> RiskScore {
    let active: Vec<&LabelTag> = tags.iter().filter(|t| t.is_active_at(now)).collect();
    if active.is_empty() {
        return RiskScore::CLEAN;
    }

    match strategy {
        TagAggregationStrategy::MaxActive => {
            let max = active.iter().map(|t| t.risk_score().value()).max().unwrap_or(0);
            RiskScore::new(max)
        }
        TagAggregationStrategy::WeightedSum => {
            // Noisy-OR over per-tag evidence probabilities
            // pᵢ = (risk_score/100) · confidence_fraction, combined as
            // 1 − Π(1 − pᵢ). Naturally bounded, no arbitrary cap.
            let retained: f64 = active
                .iter()
                .map(|t| 1.0 - (t.risk_score().value() as f64 / 100.0) * t.confidence().fraction())
                .product();
            RiskScore::new(((1.0 - retained) * 100.0).round().clamp(0.0, 100.0) as u8)
        }
        TagAggregationStrategy::MaxConfirmedOnly => {
            let max = active
                .iter()
                .filter(|t| t.confidence().value() >= Confidence::CERTAIN.value())
                .map(|t| t.risk_score().value())
                .max()
                .unwrap_or(0);
            RiskScore::new(max)
        }
    }
}

#[cfg(test)]
mod aggregation_tests {
    use super::*;

    fn tag(risk: u8, confidence: Confidence) -> LabelTag {
        LabelTag::new(
            TagCategory::Scam,
            None,
            TagSource::InternalAnalyst,
            confidence,
            RiskScore::new(risk),
            None,
            None,
            None,
        )
    }

    #[test]
    fn empty_is_clean_for_every_strategy() {
        let now = Utc::now();
        for s in [
            TagAggregationStrategy::MaxActive,
            TagAggregationStrategy::WeightedSum,
            TagAggregationStrategy::MaxConfirmedOnly,
        ] {
            assert_eq!(aggregate_risk_score(&[], s, now), RiskScore::CLEAN);
        }
    }

    #[test]
    fn max_active_ignores_confidence() {
        let now = Utc::now();
        let tags = [tag(100, Confidence::LOW)];
        assert_eq!(
            aggregate_risk_score(&tags, TagAggregationStrategy::MaxActive, now).value(),
            100
        );
    }

    #[test]
    fn weighted_sum_scales_with_confidence() {
        let now = Utc::now();
        let low = aggregate_risk_score(
            &[tag(100, Confidence::LOW)],
            TagAggregationStrategy::WeightedSum,
            now,
        );
        let certain = aggregate_risk_score(
            &[tag(100, Confidence::CERTAIN)],
            TagAggregationStrategy::WeightedSum,
            now,
        );
        // risk 100 @ LOW(25) → p=0.25 → 25; @ CERTAIN → 100.
        assert_eq!(low.value(), 25);
        assert_eq!(certain.value(), 100);
        assert!(low.value() < certain.value());
    }

    #[test]
    fn weighted_sum_noisy_or_does_not_hard_saturate() {
        let now = Utc::now();
        // Two independent risk-50 @ CERTAIN tags: noisy-OR 1-(0.5·0.5)=0.75.
        // A plain capped sum would have hit 100 here.
        let combined = aggregate_risk_score(
            &[tag(50, Confidence::CERTAIN), tag(50, Confidence::CERTAIN)],
            TagAggregationStrategy::WeightedSum,
            now,
        );
        assert_eq!(combined.value(), 75);
    }

    #[test]
    fn max_confirmed_only_considers_certain_tags() {
        let now = Utc::now();
        let tags = [tag(100, Confidence::HIGH), tag(80, Confidence::CERTAIN)];
        // The risk-100 tag is only HIGH confidence, so it's excluded.
        assert_eq!(
            aggregate_risk_score(&tags, TagAggregationStrategy::MaxConfirmedOnly, now).value(),
            80
        );
    }
}
