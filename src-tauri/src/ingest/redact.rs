//! Masks likely secrets in transcript text before it is shown or indexed.
//!
//! This is a best-effort net, not a guarantee: it errs towards hiding
//! things that look like credentials, at the cost of occasionally hiding
//! something harmless.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

pub const MASK: &str = "[redacted]";

/// Patterns whose whole match is a secret.
static WHOLE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // PEM private keys, including multi-line bodies.
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        // Anthropic, OpenAI and similar "sk-" keys.
        r"\bsk-[A-Za-z0-9_\-]{20,}",
        // Stripe.
        r"\b[rs]k_(?:live|test)_[A-Za-z0-9]{16,}",
        // GitHub tokens.
        r"\bgh[pousr]_[A-Za-z0-9]{30,}",
        r"\bgithub_pat_[A-Za-z0-9_]{30,}",
        // AWS access key ids.
        r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
        // Slack.
        r"\bxox[abposr]-[A-Za-z0-9\-]{10,}",
        // Google API keys.
        r"\bAIza[0-9A-Za-z_\-]{35}",
        // npm tokens.
        r"\bnpm_[A-Za-z0-9]{36}\b",
        // JSON Web Tokens.
        r"\beyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
    ]
    .iter()
    .filter_map(|p| Regex::new(p).ok())
    .collect()
});

/// `NAME=value` or `"name": "value"` where the name suggests a secret. The
/// name is kept so the reader can still follow what happened.
static ASSIGNMENT: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(?P<name>[A-Za-z0-9_\-]*(?:secret|token|passw(?:or)?d|api[_\-]?key|private[_\-]?key|access[_\-]?key|credential)[A-Za-z0-9_\-]*)(?P<sep>"?\s*[=:]\s*"?)(?P<value>[^\s"',;]{6,})"#,
    )
    .ok()
});

/// `Authorization: Bearer <token>`.
static BEARER: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)\b(bearer|basic)\s+[A-Za-z0-9._~+/=\-]{16,}").ok());

/// Credentials embedded in URLs: `scheme://user:password@host`.
static URL_CREDENTIALS: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?P<scheme>[A-Za-z][A-Za-z0-9+.\-]*://)[^/\s:@]+:[^/\s@]+@").ok()
});

pub fn redact(text: &str) -> Cow<'_, str> {
    let mut out = Cow::Borrowed(text);
    for re in WHOLE.iter() {
        if re.is_match(&out) {
            out = Cow::Owned(re.replace_all(&out, MASK).into_owned());
        }
    }
    if let Some(re) = URL_CREDENTIALS.as_ref()
        && re.is_match(&out)
    {
        out = Cow::Owned(
            re.replace_all(&out, format!("${{scheme}}{MASK}@"))
                .into_owned(),
        );
    }
    if let Some(re) = BEARER.as_ref()
        && re.is_match(&out)
    {
        out = Cow::Owned(re.replace_all(&out, format!("$1 {MASK}")).into_owned());
    }
    if let Some(re) = ASSIGNMENT.as_ref()
        && re.is_match(&out)
    {
        out = Cow::Owned(
            re.replace_all(&out, |c: &regex::Captures| {
                let value = &c["value"];
                // Already masked, or a placeholder reference rather than a value.
                if value.contains(MASK)
                    || value.starts_with('$')
                    || value.starts_with("process.env")
                {
                    c[0].to_string()
                } else {
                    format!("{}{}{MASK}", &c["name"], &c["sep"])
                }
            })
            .into_owned(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Secrets are assembled at runtime so the repository never contains a
    // string that secret scanners would flag.
    fn fake(prefix: &str, body: &str, n: usize) -> String {
        format!("{prefix}{}", body.repeat(n))
    }

    fn assert_masked(input: &str, secret: &str) {
        let out = redact(input);
        assert!(!out.contains(secret), "secret survived in: {out}");
        assert!(out.contains(MASK), "no mask in: {out}");
    }

    #[test]
    fn masks_provider_keys() {
        for secret in [
            fake("sk-ant-api03-", "Ab1", 12),
            fake("sk-proj-", "Zz9", 10),
            fake("ghp_", "a1B2", 10),
            fake("github_pat_", "X1_y", 10),
            fake("AKIA", "ABCD", 4),
            fake("xoxb-", "12345-", 4),
            fake("AIza", "Sy", 18)[..39].to_string(),
            fake("sk_live_", "q1W2", 6),
            fake("npm_", "a1b2c3", 6),
        ] {
            assert_masked(&format!("key is {secret} ok"), &secret);
        }
    }

    #[test]
    fn masks_jwt() {
        let jwt = format!(
            "eyJ{}.{}.{}",
            "a".repeat(20),
            "b".repeat(20),
            "c".repeat(20)
        );
        assert_masked(&format!("token {jwt}"), &jwt);
    }

    #[test]
    fn masks_private_key_blocks() {
        let begin = format!("-----BEGIN RSA {} KEY-----", "PRIVATE");
        let end = format!("-----END RSA {} KEY-----", "PRIVATE");
        let input = format!("{begin}\nMIIEow\nIBAAK\n{end}\nafter");
        let out = redact(&input);
        assert!(!out.contains("MIIEow"));
        assert!(out.ends_with("after"));
    }

    #[test]
    fn masks_env_style_assignments_but_keeps_the_name() {
        let out = redact("DATABASE_PASSWORD=hunter2hunter2\nAPI_KEY: abcdef123456\nPORT=3000");
        assert!(out.contains("DATABASE_PASSWORD=[redacted]"));
        assert!(out.contains("API_KEY: [redacted]"));
        assert!(out.contains("PORT=3000"));
    }

    #[test]
    fn masks_json_style_assignments() {
        let out = redact(r#"{"client_secret": "s3cr3tvalue", "name": "demo"}"#);
        assert!(!out.contains("s3cr3tvalue"));
        assert!(out.contains(r#""name": "demo""#));
    }

    #[test]
    fn leaves_references_to_secrets_alone() {
        let input = "API_KEY=$API_KEY and token: process.env.TOKEN";
        assert_eq!(redact(input), input);
    }

    #[test]
    fn masks_bearer_tokens_and_url_credentials() {
        let token = "abcDEF123456ghiJKL789";
        assert_masked(&format!("Authorization: Bearer {token}"), token);
        let out = redact("postgres://admin:pa55word@db.local:5432/app");
        assert_eq!(out, "postgres://[redacted]@db.local:5432/app");
    }

    #[test]
    fn ordinary_text_is_untouched_and_not_copied() {
        let input = "Ran the tests: 42 passed. Edited src/settings/theme.ts.";
        assert!(matches!(redact(input), Cow::Borrowed(_)));
    }
}
