use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentModerationAssessment {
    pub status: String,
    pub moderation_score: i32,
    pub moderation_flags: Vec<String>,
    pub auto_flagged: bool,
    pub needs_review: bool,
    pub review_priority: i32,
    pub moderation_reason: Option<String>,
    pub moderated_by: Option<String>,
}

impl Default for CommentModerationAssessment {
    fn default() -> Self {
        Self {
            status: "VISIBLE".to_string(),
            moderation_score: 0,
            moderation_flags: vec![],
            auto_flagged: false,
            needs_review: false,
            review_priority: 0,
            moderation_reason: None,
            moderated_by: None,
        }
    }
}

const SEVERE_TERMS: &[&str] = &[
    "hate speech",
    "kill yourself",
    "go die",
    "lynch",
    "genocide",
    "rape",
    "terrorist",
];

const HARASSMENT_TERMS: &[&str] = &[
    "idiot", "stupid", "moron", "loser", "shut up", "dumb", "trash",
];

const PROFANITY_TERMS: &[&str] = &["f**k", "fuk", "fk", "shit", "bitch", "asshole", "bastard"];

const SEXUAL_EXPLICIT_TERMS: &[&str] = &["nude", "naked", "porn", "sex", "sext"];

const SELF_HARM_TERMS: &[&str] = &["suicide", "self harm", "hurt yourself", "end your life"];

const SPAM_TERMS: &[&str] = &[
    "buy now",
    "free money",
    "click here",
    "crypto giveaway",
    "telegram",
    "whatsapp me",
];

fn normalize_text(input: &str) -> String {
    let mut normalized = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch.is_whitespace() {
            normalized.push(ch.to_ascii_lowercase());
        } else {
            normalized.push(' ');
        }
    }
    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn contains_single_word(tokens: &[&str], term: &str) -> bool {
    tokens.iter().any(|token| *token == term)
}

fn contains_phrase(normalized: &str, term: &str) -> bool {
    normalized.contains(term)
}

fn score_category(
    normalized: &str,
    tokens: &[&str],
    terms: &[&str],
    score_each: i32,
    flag_prefix: &str,
    out_flags: &mut Vec<String>,
) -> i32 {
    let mut total = 0;

    for term in terms {
        let hit = if term.contains(' ') {
            contains_phrase(normalized, term)
        } else {
            contains_single_word(tokens, term)
        };

        if hit {
            total += score_each;
            out_flags.push(format!("{}:{}", flag_prefix, term));
        }
    }

    total
}

pub fn assess_comment_content(content: &str) -> CommentModerationAssessment {
    let normalized = normalize_text(content);
    let tokens: Vec<&str> = normalized.split_whitespace().collect();

    let mut flags = Vec::new();
    let mut score = 0;

    score += score_category(&normalized, &tokens, SEVERE_TERMS, 90, "SEVERE", &mut flags);

    score += score_category(
        &normalized,
        &tokens,
        SELF_HARM_TERMS,
        80,
        "SELF_HARM",
        &mut flags,
    );

    score += score_category(
        &normalized,
        &tokens,
        SEXUAL_EXPLICIT_TERMS,
        55,
        "SEXUAL",
        &mut flags,
    );

    score += score_category(
        &normalized,
        &tokens,
        HARASSMENT_TERMS,
        35,
        "HARASSMENT",
        &mut flags,
    );

    score += score_category(
        &normalized,
        &tokens,
        PROFANITY_TERMS,
        30,
        "PROFANITY",
        &mut flags,
    );

    score += score_category(&normalized, &tokens, SPAM_TERMS, 40, "SPAM", &mut flags);

    if content.contains("http://") || content.contains("https://") {
        score += 25;
        flags.push("SPAM:url".to_string());
    }

    let uppercase_chars = content
        .chars()
        .filter(|c| c.is_ascii_alphabetic() && c.is_ascii_uppercase())
        .count();
    let alpha_chars = content.chars().filter(|c| c.is_ascii_alphabetic()).count();

    if alpha_chars >= 10 && (uppercase_chars as f32 / alpha_chars as f32) > 0.8 {
        score += 15;
        flags.push("ABUSE:all_caps".to_string());
    }

    if content.matches('!').count() >= 5 {
        score += 10;
        flags.push("ABUSE:aggressive_punctuation".to_string());
    }

    score = score.clamp(0, 100);

    let auto_flagged = score >= 80 || flags.iter().any(|f| f.starts_with("SEVERE"));
    let needs_review = auto_flagged || score >= 40 || !flags.is_empty();

    let status = if auto_flagged { "HIDDEN" } else { "VISIBLE" }.to_string();
    let moderation_reason = if auto_flagged {
        Some("Auto-hidden by moderation for potentially harmful/offensive content".to_string())
    } else if needs_review {
        Some("Flagged for moderator review".to_string())
    } else {
        None
    };

    let review_priority = if needs_review {
        score + if auto_flagged { 20 } else { 0 }
    } else {
        0
    }
    .clamp(0, 100);

    CommentModerationAssessment {
        status,
        moderation_score: score,
        moderation_flags: flags,
        auto_flagged,
        needs_review,
        review_priority,
        moderation_reason,
        moderated_by: if auto_flagged {
            Some("AUTO_MODERATOR".to_string())
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::assess_comment_content;

    #[test]
    fn clean_comment_stays_visible() {
        let assessment =
            assess_comment_content("Beautiful signage, thanks for sharing this archive");
        assert_eq!(assessment.status, "VISIBLE");
        assert!(!assessment.auto_flagged);
    }

    #[test]
    fn severe_comment_is_auto_hidden() {
        let assessment = assess_comment_content("you should kill yourself");
        assert_eq!(assessment.status, "HIDDEN");
        assert!(assessment.auto_flagged);
        assert!(assessment.needs_review);
    }
}
