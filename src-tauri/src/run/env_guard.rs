//! 烧钱红线守门(§9.3):`real`(订阅/OAuth)模式起 claude 前,断言子环境里没有任何会把请求
//! 改道到 API-key / Bedrock / Vertex / Foundry 计费路径的变量 —— 有就拒绝启动,而不是悄悄花钱。

/// Env vars that, if present, mean the `claude` CLI would NOT be on the
/// subscription/OAuth route (§9.3). In `real` (subscription) mode we assert NONE
/// of these are present in the scrubbed child env before spawning — if any is,
/// we refuse to launch rather than silently spend on a key/Bedrock/Vertex route.
const FORBIDDEN_SUBSCRIPTION_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

/// The API-key/Bedrock/Vertex env vars that would route a "real" run around the
/// subscription/OAuth path (§9.3). Exposed so the pre-flight check can name the
/// offending var without re-spelling the list.
pub(crate) const SUBSCRIPTION_ENV_KEYS: &[&str] = FORBIDDEN_SUBSCRIPTION_ENV;

/// Assert the live process env carries none of the [`FORBIDDEN_SUBSCRIPTION_ENV`]
/// vars before launching a `real` run.
pub(crate) fn assert_subscription_env() -> anyhow::Result<()> {
    assert_no_forbidden_env(|key| std::env::var_os(key).is_some())
}

/// Pure core of [`assert_subscription_env`]: error if `is_present` reports any of
/// the [`FORBIDDEN_SUBSCRIPTION_ENV`] vars set.
fn assert_no_forbidden_env(is_present: impl Fn(&str) -> bool) -> anyhow::Result<()> {
    for key in FORBIDDEN_SUBSCRIPTION_ENV {
        if is_present(key) {
            anyhow::bail!(
                "refusing to launch real mode: {key} is set, which would route around \
                 the subscription/OAuth path (§9.3). Unset it and retry."
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_env_assertion_rejects_api_key() {
        let result = assert_no_forbidden_env(|key| key == "ANTHROPIC_API_KEY");
        assert!(result.is_err(), "an API key present must refuse real mode");
    }

    #[test]
    fn subscription_env_assertion_rejects_bedrock_and_vertex() {
        assert!(assert_no_forbidden_env(|k| k == "CLAUDE_CODE_USE_BEDROCK").is_err());
        assert!(assert_no_forbidden_env(|k| k == "CLAUDE_CODE_USE_VERTEX").is_err());
        assert!(assert_no_forbidden_env(|k| k == "ANTHROPIC_AUTH_TOKEN").is_err());
    }

    #[test]
    fn subscription_env_assertion_passes_when_clean() {
        assert!(assert_no_forbidden_env(|_| false).is_ok());
    }
}
