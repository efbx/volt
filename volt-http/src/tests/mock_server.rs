use std::fmt::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

type Handler = dyn Fn(u32, &str) -> Reply + Send + Sync;

pub struct Reply {
    status: u16,
    headers: String,
    body: String,
    delay: Duration,
}

impl Reply {
    pub fn status(status: u16) -> Self {
        Self {
            status,
            headers: String::new(),
            body: String::new(),
            delay: Duration::ZERO,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        let _ = write!(self.headers, "{name}: {value}\r\n");
        self
    }

    pub fn body(mut self, body: &str) -> Self {
        self.body = body.to_owned();
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    fn render(&self) -> String {
        format!(
            "HTTP/1.1 {} X\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status,
            self.headers,
            self.body.len(),
            self.body
        )
    }
}

pub struct MockServer {
    url: String,
    hits: Arc<AtomicU32>,
}

impl MockServer {
    pub async fn start<F>(handler: F) -> Self
    where
        F: Fn(u32, &str) -> Reply + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicU32::new(0));
        tokio::spawn(Self::serve(listener, Arc::new(handler), hits.clone()));
        Self { url, hits }
    }

    pub async fn silent() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(Self::hold(listener));
        Self {
            url,
            hits: Arc::new(AtomicU32::new(0)),
        }
    }

    async fn hold(listener: TcpListener) {
        let mut open = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            open.push(stream);
        }
    }

    async fn serve(listener: TcpListener, handler: Arc<Handler>, hits: Arc<AtomicU32>) {
        while let Ok((stream, _)) = listener.accept().await {
            let hit = hits.fetch_add(1, Ordering::SeqCst) + 1;
            tokio::spawn(Self::answer(stream, handler.clone(), hit));
        }
    }

    async fn answer(mut stream: TcpStream, handler: Arc<Handler>, hit: u32) {
        let mut buffer = vec![0u8; 8192];
        let read = stream.read(&mut buffer).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buffer[..read]);
        let reply = handler(hit, &request);
        tokio::time::sleep(reply.delay).await;
        stream.write_all(reply.render().as_bytes()).await.ok();
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn hits(&self) -> u32 {
        self.hits.load(Ordering::SeqCst)
    }
}
