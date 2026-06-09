//! Embedding producer for §6.7 vector recall.
//!
//! [`Embedder`] is the injectable seam — per the build plan, LLM/embedding calls go
//! through a trait so tests use a free, deterministic [`FakeEmbedder`] while real
//! runs use 千问. The real [`QwenEmbedder`] (behind the `qwen` cargo feature) reads
//! its key at runtime from `~/.agents/resources.json` — never hardcoded, never
//! logged, never committed.

use anyhow::Context;

/// Produces dense embeddings for fact text. One call may embed a batch (order-preserving).
pub trait Embedder {
    /// Embed each input string; returns one vector per input, in the same order.
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;
    /// Dimensionality this embedder produces.
    fn dim(&self) -> usize;
    /// Model id stamped onto facts (so a model swap knows what to recompute).
    fn model(&self) -> &str;
}

/// Deterministic, offline, free embedder for tests + `simulate` mode. NOT semantic —
/// same text always maps to the same vector and different text to a different one,
/// so retrieval logic can be exercised without a network call or API key.
pub struct FakeEmbedder {
    dim: usize,
}

impl FakeEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

impl Default for FakeEmbedder {
    fn default() -> Self {
        Self { dim: 8 }
    }
}

impl Embedder for FakeEmbedder {
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| fake_vec(t, self.dim)).collect())
    }
    fn dim(&self) -> usize {
        self.dim
    }
    fn model(&self) -> &str {
        "fake-deterministic"
    }
}

/// FNV-1a seed over the bytes, then a splitmix-style deterministic fill → values in [-1,1).
fn fake_vec(text: &str, dim: usize) -> Vec<f32> {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (0..dim)
        .map(|i| {
            let mut x = h.wrapping_add((i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
            x ^= x >> 33;
            x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
            x ^= x >> 33;
            (x % 2000) as f32 / 1000.0 - 1.0
        })
        .collect()
}

/// 千问 (DashScope) credentials, read from `~/.agents/resources.json` at
/// `llm.qwen.<profile>.{api_key,base_url}`.
#[derive(Debug, Clone)]
pub struct QwenCreds {
    pub api_key: String,
    pub base_url: String,
}

/// Parse 千问 creds out of a resources.json string for `profile` (e.g. `"personal"`
/// / `"company"`). Kept separate from file IO so it is unit-testable.
pub fn parse_qwen_creds(json: &str, profile: &str) -> anyhow::Result<QwenCreds> {
    let v: serde_json::Value =
        serde_json::from_str(json).context("resources.json 不是合法 JSON")?;
    let node = v
        .get("llm")
        .and_then(|l| l.get("qwen"))
        .and_then(|q| q.get(profile))
        .with_context(|| format!("resources.json 缺 llm.qwen.{profile}"))?;
    let api_key = node
        .get("api_key")
        .and_then(|x| x.as_str())
        .with_context(|| format!("缺 llm.qwen.{profile}.api_key"))?
        .to_string();
    let base_url = node
        .get("base_url")
        .and_then(|x| x.as_str())
        .with_context(|| format!("缺 llm.qwen.{profile}.base_url"))?
        .to_string();
    Ok(QwenCreds { api_key, base_url })
}

/// Read `~/.agents/resources.json` and parse 千问 creds for `profile`. Runtime-only;
/// the key value is never logged or persisted.
pub fn load_qwen_creds(profile: &str) -> anyhow::Result<QwenCreds> {
    let home = std::env::var("HOME").context("HOME 未设置")?;
    let path = std::path::Path::new(&home).join(".agents/resources.json");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("读不到 {}", path.display()))?;
    parse_qwen_creds(&raw, profile)
}

/// Real 千问 v2 embedder over the OpenAI-compatible DashScope endpoint
/// (`{base_url}/embeddings`). Behind the `qwen` feature so default builds stay offline.
#[cfg(feature = "qwen")]
pub struct QwenEmbedder {
    creds: QwenCreds,
    model: String,
    dim: usize,
    client: reqwest::blocking::Client,
}

#[cfg(feature = "qwen")]
impl QwenEmbedder {
    /// `profile` = `"personal"`/`"company"`; `model` e.g. `"text-embedding-v2"` (1536-dim).
    pub fn new(profile: &str, model: impl Into<String>, dim: usize) -> anyhow::Result<Self> {
        Ok(Self {
            creds: load_qwen_creds(profile)?,
            model: model.into(),
            dim,
            client: reqwest::blocking::Client::new(),
        })
    }
}

#[cfg(feature = "qwen")]
impl Embedder for QwenEmbedder {
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.creds.base_url.trim_end_matches('/'));
        let body = serde_json::json!({ "model": self.model, "input": texts });
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.creds.api_key)
            .json(&body)
            .send()
            .context("千问 embedding 请求失败")?
            .error_for_status()
            .context("千问 embedding 返回错误状态")?;
        let parsed: serde_json::Value = resp.json().context("千问 embedding 响应非 JSON")?;
        let data = parsed
            .get("data")
            .and_then(|d| d.as_array())
            .context("千问响应缺 data[]")?;
        let mut out = Vec::with_capacity(data.len());
        for item in data {
            let emb = item
                .get("embedding")
                .and_then(|e| e.as_array())
                .context("千问响应 data[].embedding 缺失")?;
            out.push(emb.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect());
        }
        Ok(out)
    }
    fn dim(&self) -> usize {
        self.dim
    }
    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_embedder_is_deterministic_and_distinct() {
        let e = FakeEmbedder::new(8);
        let a1 = e.embed(&["hello".into()]).unwrap();
        let a2 = e.embed(&["hello".into()]).unwrap();
        let b = e.embed(&["world".into()]).unwrap();
        assert_eq!(a1[0].len(), 8, "honors dim");
        assert_eq!(a1, a2, "same text → same vector");
        assert_ne!(a1[0], b[0], "different text → different vector");
        assert_eq!(e.dim(), 8);
    }

    #[test]
    fn parse_qwen_creds_reads_profile() {
        let json = r#"{"llm":{"qwen":{
            "personal":{"api_key":"sk-x","base_url":"https://h/v1"},
            "company":{"api_key":"sk-y","base_url":"https://h2/v1"}}}}"#;
        let p = parse_qwen_creds(json, "personal").unwrap();
        assert_eq!(p.api_key, "sk-x");
        assert_eq!(p.base_url, "https://h/v1");
        assert_eq!(parse_qwen_creds(json, "company").unwrap().api_key, "sk-y");
        assert!(parse_qwen_creds(json, "missing").is_err(), "unknown profile errors");
        assert!(parse_qwen_creds("not json", "personal").is_err(), "bad json errors");
    }
}
