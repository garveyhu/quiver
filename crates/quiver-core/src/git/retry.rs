//! git 操作的瞬时锁错误重试(§6):并发 worktree 操作偶发 index.lock / shallow.lock 抢占,
//! 分类为瞬时错误后按 RetryConfig 退避重试,避免假失败。`pub(super)` 让父模块的 run_meta 复用。

use std::future::Future;

use super::RetryConfig;

/// Does this git error message look like a transient lock contention we should
/// retry (DESIGN §6.3)? Matches the strings git emits when a `.git` lock file is
/// held: `index.lock`, `config.lock`, or "could not lock"/"Unable to create ...
/// lock". A non-lock error (real failure) returns false → surfaced immediately.
pub(super) fn is_transient_lock_error(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("index.lock")
        || m.contains("config.lock")
        || m.contains("could not lock")
        || m.contains("unable to create")
        || m.contains("file exists") && m.contains(".lock")
}

/// Run `op` with bounded retry + exponential backoff, retrying ONLY on errors
/// classified transient by `is_transient`. Surfaces the last error once attempts
/// are exhausted, or any non-transient error immediately.
pub(super) async fn retry_on_lock<T, F, Fut>(
    cfg: RetryConfig,
    is_transient: fn(&str) -> bool,
    mut op: F,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let mut delay = cfg.base_delay;
    let mut attempt = 1;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let transient = is_transient(&e.to_string());
                if !transient || attempt >= cfg.max_attempts {
                    return Err(e);
                }
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                delay = delay.saturating_mul(2);
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_transient_lock_errors() {
        assert!(is_transient_lock_error(
            "fatal: Unable to create '/r/.git/index.lock': File exists."
        ));
        assert!(is_transient_lock_error("could not lock config file"));
        assert!(is_transient_lock_error("error: config.lock held"));
        // A real failure is NOT transient — must surface immediately.
        assert!(!is_transient_lock_error("fatal: not a git repository"));
        assert!(!is_transient_lock_error("merge conflict in foo.rs"));
    }

    #[tokio::test]
    async fn retry_succeeds_after_n_transient_lock_errors() {
        use std::cell::Cell;
        let calls = Cell::new(0u32);
        // Fail with a lock error the first 2 times, succeed on the 3rd.
        let result = retry_on_lock(RetryConfig::instant(5), is_transient_lock_error, || {
            let n = calls.get() + 1;
            calls.set(n);
            async move {
                if n < 3 {
                    Err(anyhow::anyhow!("fatal: Unable to create '.git/index.lock': File exists."))
                } else {
                    Ok(n)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 3);
        assert_eq!(calls.get(), 3, "should retry exactly until success");
    }

    #[tokio::test]
    async fn retry_surfaces_after_exhausting_attempts() {
        use std::cell::Cell;
        let calls = Cell::new(0u32);
        let result: anyhow::Result<()> =
            retry_on_lock(RetryConfig::instant(3), is_transient_lock_error, || {
                calls.set(calls.get() + 1);
                async { Err(anyhow::anyhow!("could not lock index.lock")) }
            })
            .await;
        assert!(result.is_err(), "exhausted retries must surface the error");
        assert_eq!(calls.get(), 3, "should attempt exactly max_attempts times");
    }

    #[tokio::test]
    async fn retry_does_not_retry_non_transient_error() {
        use std::cell::Cell;
        let calls = Cell::new(0u32);
        let result: anyhow::Result<()> =
            retry_on_lock(RetryConfig::instant(5), is_transient_lock_error, || {
                calls.set(calls.get() + 1);
                async { Err(anyhow::anyhow!("fatal: not a git repository")) }
            })
            .await;
        assert!(result.is_err());
        assert_eq!(calls.get(), 1, "a non-transient error surfaces on the first try");
    }
}
