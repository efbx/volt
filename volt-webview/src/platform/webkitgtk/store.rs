use webkit2gtk::{DownloadExt, WebContext, WebContextExt};

/// A session's webview store on Linux and the BSDs: one ephemeral context its pages share,
/// whose cookies and storage live in memory and go with the session. Downloads are refused.
pub(crate) struct Store {
    /// The context every page of the session is created in.
    pub(super) context: WebContext,
}

impl Store {
    /// Returns a new ephemeral store that refuses downloads.
    pub(crate) fn new() -> Self {
        let context = WebContext::new_ephemeral();
        context.connect_download_started(|_, download| download.cancel());
        Self { context }
    }
}
