//! §6.7 向量召回 + 混合检索:embedding 存取、暴力余弦向量腿、RRF 融合三腿(FTS/向量/内在分)。
//! 内在分腿复用 `super::recall::score`。

use std::collections::HashMap;

use rusqlite::params;

use super::recall::score;
use super::FactRecord;
use crate::{Embedder, MemoryStore};

impl MemoryStore {
    /// Attach a dense embedding to a fact (§6.7 向量召回). Stores the f32 vector as a
    /// little-endian BLOB and stamps which model/dim produced it (so a model swap
    /// knows what to recompute). Idempotent overwrite.
    pub fn set_fact_embedding(
        &self,
        fact_id: i64,
        embedding: &[f32],
        embed_model: &str,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("memory store lock");
        conn.execute(
            "UPDATE memory_fact SET embedding = ?2, embed_model = ?3, embed_dim = ?4 WHERE id = ?1",
            params![fact_id, vec_to_blob(embedding), embed_model, embedding.len() as i64],
        )?;
        Ok(())
    }

    /// §6.7 向量腿:在 `project` 的当前真相事实里按 cosine 相似度排序返回 top-k(只看已
    /// 向量化的事实)。小库直接 Rust 暴力余弦;sqlite-vec(vec0)ANN 索引是后续刀(规模化时)。
    /// 与 `search_facts`(FTS 关键词腿)互补,上层融合成混合检索。
    pub fn vector_search(
        &self,
        project: &str,
        query: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<FactRecord>> {
        let conn = self.conn.lock().expect("memory store lock");
        let mut stmt = conn.prepare(
            "SELECT id, project, scope, kind, text, entities, importance,
                    valid_at, invalid_at, recorded_at, trust, entity, embedding
             FROM memory_fact
             WHERE project = ?1 AND invalid_at IS NULL AND embedding IS NOT NULL",
        )?;
        let mut scored: Vec<(f32, FactRecord)> = stmt
            .query_map(params![project], |row| {
                let blob: Vec<u8> = row.get(12)?;
                let rec = FactRecord {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    scope: row.get(2)?,
                    kind: row.get(3)?,
                    text: row.get(4)?,
                    entities: row.get(5)?,
                    importance: row.get(6)?,
                    valid_at: row.get(7)?,
                    invalid_at: row.get(8)?,
                    recorded_at: row.get(9)?,
                    trust: row.get(10)?,
                    entity: row.get(11)?,
                };
                Ok((cosine(query, &blob_to_vec(&blob)), rec))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored.into_iter().map(|(_, r)| r).collect())
    }

    /// §6.7 混合检索:把三条腿用 Reciprocal Rank Fusion(RRF)融成一个排名 ——
    /// (1) FTS 关键词腿 `search_facts`、(2) 向量腿 `vector_search`、(3) §6.7 内在分腿
    /// (importance/trust/recency 排序的当前真相)。RRF 按**名次**融合,不受各腿分值
    /// 量纲差异影响(经典 hybrid 做法)。空 `query_text`/`query_vec` 跳过对应腿;内在腿
    /// 恒在(冷启动也有结果)。返回前 `limit`,best first。
    ///
    /// `query_vec` 由调用方用注入的 [`crate::Embedder`] 把查询文本向量化得到。
    pub fn hybrid_recall(
        &self,
        project: &str,
        query_text: &str,
        query_vec: &[f32],
        now_ms: i64,
        limit: usize,
    ) -> anyhow::Result<Vec<FactRecord>> {
        const K: f64 = 60.0; // RRF 抑制常数(标准取 60)
        const LEG_CAP: usize = 50; // 每条腿候选上限

        let mut legs: Vec<Vec<FactRecord>> = Vec::new();
        if !query_text.trim().is_empty() {
            legs.push(self.search_facts(project, query_text)?);
        }
        if !query_vec.is_empty() {
            legs.push(self.vector_search(project, query_vec, LEG_CAP)?);
        }
        // §6.7 内在分腿(恒在):当前真相按 importance/trust/recency 降序。
        let mut intrinsic = self.current_facts(project)?;
        intrinsic.sort_by(|a, b| {
            score(b, now_ms)
                .partial_cmp(&score(a, now_ms))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        legs.push(intrinsic);

        let mut rrf: HashMap<i64, f64> = HashMap::new();
        let mut by_id: HashMap<i64, FactRecord> = HashMap::new();
        for leg in legs {
            for (rank, f) in leg.into_iter().enumerate() {
                *rrf.entry(f.id).or_insert(0.0) += 1.0 / (K + rank as f64 + 1.0);
                by_id.entry(f.id).or_insert(f);
            }
        }

        let mut ranked: Vec<FactRecord> = by_id.into_values().collect();
        ranked.sort_by(|a, b| {
            rrf[&b.id]
                .partial_cmp(&rrf[&a.id])
                .unwrap_or(std::cmp::Ordering::Equal)
                // deterministic tie-break: intrinsic score, then id
                .then_with(|| {
                    score(b, now_ms)
                        .partial_cmp(&score(a, now_ms))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then(a.id.cmp(&b.id))
        });
        ranked.truncate(limit);
        Ok(ranked)
    }

    /// Embed one fact's `text` with an injected `embedder` and store the vector
    /// (§6.7). The store stays embedder-agnostic — callers pass the Embedder (Fake
    /// in tests/simulate, 千问 in real runs). Use at insert/promote time.
    pub fn embed_fact(
        &self,
        fact_id: i64,
        text: &str,
        embedder: &dyn Embedder,
    ) -> anyhow::Result<()> {
        let vecs = embedder.embed(std::slice::from_ref(&text.to_string()))?;
        let vec = vecs
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("embedder 返回空结果"))?;
        self.set_fact_embedding(fact_id, &vec, embedder.model())
    }

    /// Backfill embeddings for up to `limit` current-truth facts in `project` that
    /// have none yet (batch-embed in one `embedder` call). Returns how many were
    /// stored. Idempotent: a second call once everything is embedded returns 0.
    pub fn embed_unembedded(
        &self,
        project: &str,
        embedder: &dyn Embedder,
        limit: usize,
    ) -> anyhow::Result<usize> {
        let pending: Vec<(i64, String)> = {
            let conn = self.conn.lock().expect("memory store lock");
            let mut stmt = conn.prepare(
                "SELECT id, text FROM memory_fact
                 WHERE project = ?1 AND invalid_at IS NULL AND embedding IS NULL
                 ORDER BY id LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(params![project, limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        if pending.is_empty() {
            return Ok(0);
        }
        let texts: Vec<String> = pending.iter().map(|(_, t)| t.clone()).collect();
        let vecs = embedder.embed(&texts)?;
        let mut stored = 0usize;
        for ((id, _), v) in pending.iter().zip(vecs.iter()) {
            self.set_fact_embedding(*id, v, embedder.model())?;
            stored += 1;
        }
        Ok(stored)
    }
}

/// Pack an f32 embedding into a little-endian BLOB.
fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Unpack a little-endian BLOB back into f32s (trailing partial bytes ignored).
fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Cosine similarity in [-1, 1]; 0 when either vector is empty/zero or dims differ.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}
