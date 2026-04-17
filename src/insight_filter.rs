/// Insight filtering logic for inter-project memory aggregation
///
/// This module defines which insights are valuable enough to share across projects
/// based on their type and relevance score.

/// Insight types that are inherently high-value for cross-project sharing
pub const HIGH_VALUE_CATEGORIES: &[&str] = &[
    "architecture",
    "pattern",
    "design-decision",
    "design",
    "root-cause",
    "root-cause-fix",
    "framework-bug",
    "gotcha",
    "lesson-learned",
];

/// Insight types that are low-value and should not be aggregated
pub const LOW_VALUE_CATEGORIES: &[&str] = &[
    "debug",
    "debug-helper",
    "workaround",
    "temporary-fix",
    "typo",
    "typo-fix",
    "experiment",
    "one-off",
    "quick-fix",
];

/// Default relevance threshold for aggregation (0.0-1.0)
/// Insights with relevance_score < this threshold are excluded
pub const DEFAULT_RELEVANCE_THRESHOLD: f32 = 1.0;

/// Minimum threshold for high-value categories
/// High-value categories only need this much relevance to be included
pub const HIGH_VALUE_THRESHOLD: f32 = 0.6;

/// Minimum threshold for low-value categories
/// These are always excluded, regardless of relevance score
pub const LOW_VALUE_THRESHOLD: f32 = f32::INFINITY; // Never aggregate

/// Determine if an insight should be aggregated to the @interproject scope
///
/// An insight is aggregated if:
/// 1. It's NOT in the low-value category list
/// 2. If it's in the high-value category list, relevance_score >= 0.6
/// 3. Otherwise, relevance_score >= 1.0 (default threshold)
///
/// # Arguments
/// * `insight_type` - The type of the insight (bug, pattern, etc.)
/// * `relevance_score` - The relevance score assigned by the LLM (0.0-1.0)
/// * `min_threshold` - Optional custom minimum threshold (uses DEFAULT if None)
pub fn should_aggregate_insight(
    insight_type: &str,
    relevance_score: f32,
    min_threshold: Option<f32>,
) -> bool {
    let normalized_type = insight_type.to_lowercase();

    // Step 1: Exclude low-value categories outright
    if LOW_VALUE_CATEGORIES.contains(&normalized_type.as_str()) {
        return false;
    }

    // Step 2: Clamp relevance score to valid range
    let score = relevance_score.clamp(0.0, 1.0);

    // Step 3: Check relevance threshold based on category
    if HIGH_VALUE_CATEGORIES.contains(&normalized_type.as_str()) {
        // High-value categories have a lower bar
        score >= HIGH_VALUE_THRESHOLD
    } else {
        // Other categories use the default or custom threshold
        let threshold = min_threshold.unwrap_or(DEFAULT_RELEVANCE_THRESHOLD);
        score >= threshold
    }
}

/// Get a human-readable description of why an insight was filtered
pub fn filter_reason(insight_type: &str, relevance_score: f32) -> Option<String> {
    let normalized_type = insight_type.to_lowercase();

    if LOW_VALUE_CATEGORIES.contains(&normalized_type.as_str()) {
        return Some(format!(
            "Low-value category: '{}' is typically project-specific",
            insight_type
        ));
    }

    let score = relevance_score.clamp(0.0, 1.0);

    if HIGH_VALUE_CATEGORIES.contains(&normalized_type.as_str()) {
        if score < HIGH_VALUE_THRESHOLD {
            return Some(format!(
                "Insufficient relevance ({:.2}) for high-value category (min: {:.2})",
                score, HIGH_VALUE_THRESHOLD
            ));
        }
    } else {
        if score < DEFAULT_RELEVANCE_THRESHOLD {
            return Some(format!(
                "Insufficient relevance ({:.2}) for general category (min: {:.2})",
                score, DEFAULT_RELEVANCE_THRESHOLD
            ));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_high_value_categories() {
        // Architecture decisions should aggregate with good scores
        assert!(should_aggregate_insight("architecture", 0.7, None));
        assert!(should_aggregate_insight("pattern", 0.65, None));
        assert!(should_aggregate_insight("design-decision", 0.6, None));

        // But not with poor scores
        assert!(!should_aggregate_insight("architecture", 0.5, None));
    }

    #[test]
    fn test_low_value_categories() {
        // Low-value categories never aggregate
        assert!(!should_aggregate_insight("debug", 1.0, None));
        assert!(!should_aggregate_insight("workaround", 1.0, None));
        assert!(!should_aggregate_insight("typo", 1.0, None));
        assert!(!should_aggregate_insight("experiment", 0.99, None));
    }

    #[test]
    fn test_neutral_categories() {
        // Neutral categories use high bar by default
        assert!(should_aggregate_insight("bug", 1.0, None));
        assert!(!should_aggregate_insight("bug", 0.99, None));
        assert!(!should_aggregate_insight("fix", 0.5, None));

        // But can be customized
        assert!(should_aggregate_insight("bug", 0.7, Some(0.7)));
    }

    #[test]
    fn test_edge_cases() {
        // Clamp scores to 0-1 range
        assert!(should_aggregate_insight("architecture", 1.5, None)); // Clamped to 1.0
        assert!(!should_aggregate_insight("architecture", -0.5, None)); // Clamped to 0.0

        // Case insensitive
        assert!(should_aggregate_insight("ARCHITECTURE", 0.7, None));
        assert!(!should_aggregate_insight("DEBUG", 1.0, None));
    }

    #[test]
    fn test_filter_reason() {
        assert!(filter_reason("debug", 1.0)
            .unwrap()
            .contains("Low-value category"));

        assert!(filter_reason("architecture", 0.5)
            .unwrap()
            .contains("Insufficient relevance"));

        assert!(filter_reason("architecture", 0.7).is_none()); // Should pass
    }
}
