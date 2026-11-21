use rand::RngCore;
use sha2::{Digest, Sha256};

pub const PLAN_TIERS: [&str; 8] = ["go", "lite", "essential", "standard", "plus", "pro", "max", "ultra"];

pub fn random_token(bytes: usize) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn random_key() -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    let mut buf = [0u8; 30];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    format!("sk-pm-{}", URL_SAFE_NO_PAD.encode(buf))
}

pub fn sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

pub fn is_valid_tier(tier: &str) -> bool {
    PLAN_TIERS.contains(&tier)
}

pub fn sanitize_next(next: &str) -> String {
    let value = next.trim();
    if value.starts_with('/')
        && !value.starts_with("//")
        && !value.contains("://")
        && !value.contains('\\')
    {
        value.to_string()
    } else {
        "/".to_string()
    }
}

pub fn client_ip(headers: &axum::http::HeaderMap) -> String {
    if let Some(value) = headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok()) {
        return value.trim().to_string();
    }
    if let Some(value) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        return value.trim().to_string();
    }
    if let Some(value) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(first) = value.split(',').next() {
            return first.trim().to_string();
        }
    }
    String::new()
}

pub fn estimate_tokens(text: &str) -> i64 {
    let chars = text.chars().count() as i64;
    if chars == 0 {
        0
    } else {
        (chars / 3).max(1)
    }
}

pub fn micro_to_usd(micro: i64) -> String {
    format!("{:.6}", micro as f64 / 1_000_000.0)
}

pub fn truncate_chars(value: &str, max: usize) -> String {
    let mut out: String = value.chars().take(max).collect();
    if value.chars().count() > max {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_path_is_restricted_to_local_paths() {
        assert_eq!(sanitize_next("/"), "/");
        assert_eq!(sanitize_next("/#/keys"), "/#/keys");
        assert_eq!(sanitize_next("//evil.example"), "/");
        assert_eq!(sanitize_next("https://evil.example"), "/");
        assert_eq!(sanitize_next("javascript:alert(1)"), "/");
    }

    #[test]
    fn keys_are_prefixed_and_long() {
        let key = random_key();
        assert!(key.starts_with("sk-pm-"));
        assert!(key.len() > 30);
        assert_ne!(key, random_key());
    }

    #[test]
    fn money_format_is_stable() {
        assert_eq!(micro_to_usd(1_000_000), "1.000000");
        assert_eq!(micro_to_usd(3_600), "0.003600");
    }

    #[test]
    fn tier_normalization_rejects_unknown_values() {
        assert!(is_valid_tier("max"));
        assert!(!is_valid_tier("vip"));
    }
}
