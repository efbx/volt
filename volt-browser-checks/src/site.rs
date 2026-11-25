use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time;

/// The page with the login form.
const FORM: &str = include_str!("html/form.html");
/// The page with the list of teas.
const LIST: &str = include_str!("html/list.html");
/// The page whose parts turn up late.
const LATE: &str = include_str!("html/late.html");
/// The page with a frame and a shadow root.
const FRAME: &str = include_str!("html/frame.html");
/// The page inside the frame.
const INNER: &str = include_str!("html/inner.html");
/// The page a link leads to.
const NEXT: &str = include_str!("html/next.html");
/// The page that sets cookies and stores values.
const STATE: &str = include_str!("html/state.html");
/// The page a submitted form lands on.
const SUBMITTED: &str = include_str!("html/submitted.html");
/// The cookies the state page sets: one a script cannot read, one it can.
const COOKIES: &str =
    "Set-Cookie: token=abc; HttpOnly; Path=/\r\nSet-Cookie: theme=dark; Path=/\r\n";

/// The small site on this machine that the checks' pages come from, served until dropped.
#[derive(Debug)]
pub(super) struct Site {
    /// Where the site listens.
    address: SocketAddr,
    /// The task that answers requests.
    serving: JoinHandle<()>,
}

impl Site {
    /// The host name of the site, which its cookies belong to.
    pub(super) const HOST: &str = "127.0.0.1";
    /// The path of the page that takes [`Site::SLOWNESS`] to arrive.
    pub(super) const SLOW: &str = "/slow";
    /// How long the slow page takes to arrive.
    const SLOWNESS: Duration = Duration::from_millis(300);

    /// Starts the site on a free port of this machine.
    pub(super) async fn serve() -> io::Result<Self> {
        let listener = TcpListener::bind((Self::HOST, 0)).await?;
        let address = listener.local_addr()?;
        let serving = tokio::spawn(Self::accept(listener));
        Ok(Self { address, serving })
    }

    /// The URL of `path` on the site.
    pub(super) fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    /// The URL of `path` on the site named `localhost`, which a browser takes for another site.
    pub(super) fn url_elsewhere(&self, path: &str) -> String {
        format!("http://localhost:{}{path}", self.address.port())
    }

    /// Answers every connection `listener` takes.
    async fn accept(listener: TcpListener) {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(Self::answer(stream));
        }
    }

    /// Reads one request from `stream` and writes the page it asks for.
    async fn answer(mut stream: TcpStream) {
        let mut request = Vec::with_capacity(1024);
        let mut chunk = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(read) => request.extend_from_slice(&chunk[..read]),
            }
        }
        let path = Self::path(&request);
        if path == Self::SLOW {
            time::sleep(Self::SLOWNESS).await;
        }
        let response = Self::response(path);
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    }

    /// The path `request` asks for, without its query.
    fn path(request: &[u8]) -> &str {
        let line = request
            .split(|byte| *byte == b'\r')
            .next()
            .unwrap_or_default();
        let target = line.split(|byte| *byte == b' ').nth(1).unwrap_or_default();
        let path = target
            .split(|byte| *byte == b'?')
            .next()
            .unwrap_or_default();
        str::from_utf8(path).unwrap_or_default()
    }

    /// The whole HTTP answer for `path`: its page, or nothing found.
    fn response(path: &str) -> String {
        let (status, headers, body) = match path {
            "/form" => ("200 OK", "", FORM),
            "/list" => ("200 OK", "", LIST),
            "/late" => ("200 OK", "", LATE),
            "/frame" => ("200 OK", "", FRAME),
            "/inner" => ("200 OK", "", INNER),
            "/next" | Self::SLOW => ("200 OK", "", NEXT),
            "/state" => ("200 OK", COOKIES, STATE),
            "/submitted" => ("200 OK", "", SUBMITTED),
            _ => ("404 Not Found", "", ""),
        };
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
            body.len()
        )
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::Site;

    /// A plain HTTP client for the site.
    struct Visitor;

    impl Visitor {
        /// Returns the whole answer to a GET of `target`.
        async fn get(site: &Site, target: &str) -> String {
            let mut stream = TcpStream::connect(site.address).await.unwrap();
            let request = format!("GET {target} HTTP/1.1\r\nHost: {}\r\n\r\n", site.address);
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut answer = String::new();
            stream.read_to_string(&mut answer).await.unwrap();
            answer
        }
    }

    #[tokio::test]
    async fn each_path_serves_its_page_whatever_the_query() {
        let site = Site::serve().await.unwrap();
        let form = Visitor::get(&site, "/form").await;
        assert!(form.starts_with("HTTP/1.1 200 OK\r\n"), "{form}");
        assert!(form.contains("<title>Form</title>"), "{form}");
        let submitted = Visitor::get(&site, "/submitted?email=a%40b.c").await;
        assert!(
            submitted.contains("<title>Submitted</title>"),
            "{submitted}"
        );
        assert!(site.url("/list").starts_with("http://127.0.0.1:"));
    }

    #[tokio::test]
    async fn the_state_page_sets_its_two_cookies() {
        let site = Site::serve().await.unwrap();
        let state = Visitor::get(&site, "/state").await;
        assert!(
            state.contains("Set-Cookie: token=abc; HttpOnly; Path=/\r\n"),
            "{state}"
        );
        assert!(
            state.contains("Set-Cookie: theme=dark; Path=/\r\n"),
            "{state}"
        );
    }

    #[tokio::test]
    async fn the_slow_page_takes_its_time() {
        let site = Site::serve().await.unwrap();
        let asked = Instant::now();
        let slow = Visitor::get(&site, Site::SLOW).await;
        assert!(asked.elapsed() >= Site::SLOWNESS, "{:?}", asked.elapsed());
        assert!(slow.contains("<title>Next</title>"), "{slow}");
    }

    #[tokio::test]
    async fn an_unknown_path_is_not_found() {
        let site = Site::serve().await.unwrap();
        let missing = Visitor::get(&site, "/favicon.ico").await;
        assert!(
            missing.starts_with("HTTP/1.1 404 Not Found\r\n"),
            "{missing}"
        );
        assert!(
            missing.ends_with("Content-Length: 0\r\nConnection: close\r\n\r\n"),
            "{missing}"
        );
    }
}
