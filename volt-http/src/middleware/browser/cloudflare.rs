use std::sync::Arc;
use std::time::Duration;

use wreq::cookie::Jar;

use super::BrowserBuilder;

/// A script that is `true` once the page has a title and shows no Cloudflare challenge.
const CHALLENGE_CLEARED: &str = "(function(){var t=(document.title||'').toLowerCase();if(!t)return false;return !(t.includes('just a moment')||t.includes('attention required')||t.includes('shields are up')||!!window._cf_chl_opt);})()";
/// How many times the Cloudflare action checks whether the challenge has cleared.
const CHALLENGE_POLLS: usize = 60;
/// The pause between two checks.
const CHALLENGE_POLL_INTERVAL: Duration = Duration::from_millis(500);

impl BrowserBuilder {
    /// Returns a builder for Cloudflare challenges that adds cookies to `jar`. Its action checks
    /// every 500 ms, for up to 30 s, whether the challenge has cleared, then lets the cookies be
    /// read either way.
    pub fn cloudflare(jar: Arc<Jar>) -> Self {
        Self::new(jar).action(|page| async move {
            for _ in 0..CHALLENGE_POLLS {
                let cleared = page
                    .evaluate(CHALLENGE_CLEARED)
                    .await
                    .ok()
                    .and_then(|value| value.into_value::<bool>().ok())
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
