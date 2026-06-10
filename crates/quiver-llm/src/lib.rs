//! 共享 LLM 适配层:千问(DashScope OpenAI 兼容)凭证 + chat HTTP。
//!
//! 经理大脑(quiver-orchestrator)和记忆官/嵌入(quiver-memory)都调千问,凭证读取
//! 和 HTTP 调用集中在这里,避免各处重复。**key 只在运行时从 `~/.agents/resources.json` 的
//! `llm.qwen.<profile>` 读,绝不硬编码 / 打印 / 提交。**

use anyhow::Context;

/// 千问(DashScope)凭证,读自 `~/.agents/resources.json` 的 `llm.qwen.<profile>`。
#[derive(Debug, Clone)]
pub struct QwenCreds {
    pub api_key: String,
    pub base_url: String,
}

/// 从一段 resources.json 文本里解析 `profile`(如 `"personal"`/`"company"`)的千问凭证。
/// 与文件 IO 分离,便于单测。
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

/// 读 `~/.agents/resources.json` 并解析 `profile` 的千问凭证。运行时专用;key 不打印/落盘。
pub fn load_qwen_creds(profile: &str) -> anyhow::Result<QwenCreds> {
    let home = std::env::var("HOME").context("HOME 未设置")?;
    let path = std::path::Path::new(&home).join(".agents/resources.json");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("读不到 {}", path.display()))?;
    parse_qwen_creds(&raw, profile)
}

/// 一次千问 chat completion:给 system + user,温度 0,返回助手回复正文(§21 决策/判断都用它)。
pub fn qwen_chat(creds: &QwenCreds, model: &str, system: &str, user: &str) -> anyhow::Result<String> {
    let client = reqwest::blocking::Client::new();
    let url = format!("{}/chat/completions", creds.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0
    });
    let resp = client
        .post(&url)
        .bearer_auth(&creds.api_key)
        .json(&body)
        .send()
        .context("千问 chat 请求失败")?
        .error_for_status()
        .context("千问 chat 返回错误状态")?;
    let parsed: serde_json::Value = resp.json().context("千问 chat 响应非 JSON")?;
    let content = parsed["choices"][0]["message"]["content"]
        .as_str()
        .context("千问响应缺 choices[0].message.content")?;
    Ok(content.to_string())
}

/// 一次千问 embedding(OpenAI 兼容 `{base_url}/embeddings`):返回每条输入的稠密向量。
pub fn qwen_embed(creds: &QwenCreds, model: &str, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
    let client = reqwest::blocking::Client::new();
    let url = format!("{}/embeddings", creds.base_url.trim_end_matches('/'));
    let body = serde_json::json!({ "model": model, "input": texts });
    let resp = client
        .post(&url)
        .bearer_auth(&creds.api_key)
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

/// 从 LLM 回复里抠出第一个 `{…}` JSON 对象(容忍代码围栏 / 散文)。
pub fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    (end > start).then(|| &s[start..=end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_qwen_creds_reads_profile() {
        let json = r#"{"llm":{"qwen":{
            "personal":{"api_key":"sk-x","base_url":"https://h/v1"},
            "company":{"api_key":"sk-y","base_url":"https://h2/v1"}}}}"#;
        let p = parse_qwen_creds(json, "personal").unwrap();
        assert_eq!(p.api_key, "sk-x");
        assert_eq!(p.base_url, "https://h/v1");
        assert_eq!(parse_qwen_creds(json, "company").unwrap().api_key, "sk-y");
        assert!(parse_qwen_creds(json, "missing").is_err());
        assert!(parse_qwen_creds("nope", "personal").is_err());
    }

    #[test]
    fn extract_json_object_tolerates_fences() {
        assert_eq!(extract_json_object("```json\n{\"a\":1}\n```"), Some("{\"a\":1}"));
        assert_eq!(extract_json_object("无对象"), None);
    }
}
