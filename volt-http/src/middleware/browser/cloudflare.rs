use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use volt_browser::{Session, Settings};
use wreq::cookie::Jar;

use super::BrowserBuilder;

/// A script that is `true` once the page has a title and shows no Cloudflare challenge.
const CHALLENGE_CLEARED: &str = "(function(){var t=(document.title||'').toLowerCase();if(!t)return false;return !(t.includes('just a moment')||t.includes('attention required')||t.includes('shields are up')||!!window._cf_chl_opt);})()";
/// How many times the Cloudflare action checks whether the challenge has cleared.
const CHALLENGE_POLLS: usize = 60;
/// The pause between two checks.
const CHALLENGE_POLL_INTERVAL: Duration = Duration::from_millis(500);

impl BrowserBuilder {
    /// Returns a builder for Cloudflare challenges that adds cookies to `jar` and solves in the
    /// browser `launch` starts, as [`new`](Self::new) describes. Its action checks every 500 ms,
    /// for up to 30 s, whether the challenge has cleared, then lets the cookies be read either
    /// way.
    pub fn cloudflare<L, Fut>(jar: Arc<Jar>, launch: L) -> Self
    where
        L: Fn(Settings) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Session, volt_browser::Error>> + Send + 'static,
    {
        Self::new(jar, launch).action(|page| async move {
            for _ in 0..CHALLENGE_POLLS {
                let cleared = page
                    .evaluate::<bool>(CHALLENGE_CLEARED)
                    .await
                    .unwrap_or(false);
                if cleared {
                    break;
                }
                tokio::time::sleep(CHALLENGE_POLL_INTERVAL).await;
            }
            Ok(())
        })
    }
}
