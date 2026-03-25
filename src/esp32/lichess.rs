use crate::lichess::{
    GameEvent, LichessClient, LichessGame, LichessStream, extract_json_string, parse_game_event,
};
use embedded_svc::http::Method;
use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::io::Write;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::sys::esp_crt_bundle_attach;

const LICHESS_BASE: &str = "https://lichess.org";

#[derive(Debug)]
pub enum Esp32LichessError {
    Http(String),
    Parse(String),
    Io(String),
}

impl std::fmt::Display for Esp32LichessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "HTTP error: {e}"),
            Self::Parse(e) => write!(f, "parse error: {e}"),
            Self::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

pub struct Esp32LichessClient {
    token: &'static str,
}

impl Esp32LichessClient {
    pub fn new(token: &'static str) -> Self {
        Self { token }
    }
}

pub struct Esp32LichessGame {
    game_id: String,
    token: &'static str,
}

impl LichessClient for Esp32LichessClient {
    type Error = Esp32LichessError;
    type Game = Esp32LichessGame;

    fn challenge_ai(
        self,
        level: u8,
        clock_limit: u32,
        clock_increment: u32,
    ) -> Result<Esp32LichessGame, Esp32LichessError> {
        let config = HttpConfig {
            timeout: Some(std::time::Duration::from_secs(10)),
            crt_bundle_attach: Some(esp_crt_bundle_attach),
            ..Default::default()
        };

        let mut client = HttpClient::wrap(
            EspHttpConnection::new(&config).map_err(|e| Esp32LichessError::Http(e.to_string()))?,
        );

        let url = format!("{LICHESS_BASE}/api/challenge/ai");
        let body = format!(
            "level={level}&color=white&variant=standard\
             &clock.limit={clock_limit}&clock.increment={clock_increment}"
        );
        let content_len = format!("{}", body.len());
        let auth = format!("Bearer {}", self.token);
        let headers = [
            ("Authorization", auth.as_str()),
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Content-Length", content_len.as_str()),
        ];

        let mut request = client
            .request(Method::Post, &url, &headers)
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        request
            .write_all(body.as_bytes())
            .map_err(|e| Esp32LichessError::Io(e.to_string()))?;

        request
            .flush()
            .map_err(|e| Esp32LichessError::Io(e.to_string()))?;

        let mut response = request
            .submit()
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        let status = response.status();
        if status != 201 {
            let mut buf = [0u8; 512];
            let n = response.read(&mut buf).unwrap_or(0);
            let body_str = core::str::from_utf8(&buf[..n]).unwrap_or("<non-utf8>");
            return Err(Esp32LichessError::Http(format!(
                "challenge_ai returned {status}: {body_str}"
            )));
        }

        // Read response body and parse game ID
        let mut buf = [0u8; 1024];
        let n = embedded_svc::utils::io::try_read_full(&mut response, &mut buf)
            .map_err(|e| Esp32LichessError::Io(e.0.to_string()))?;
        let body_str =
            core::str::from_utf8(&buf[..n]).map_err(|e| Esp32LichessError::Parse(e.to_string()))?;

        let game_id = extract_json_string(body_str, "id")
            .ok_or_else(|| Esp32LichessError::Parse(format!("no 'id' in response: {body_str}")))?;

        log::info!("Lichess challenge created: game_id={game_id}");

        Ok(Esp32LichessGame {
            game_id,
            token: self.token,
        })
    }
}

impl LichessGame for Esp32LichessGame {
    type Error = Esp32LichessError;

    fn game_id(&self) -> &str {
        &self.game_id
    }

    fn into_stream(
        self,
    ) -> Result<Box<dyn LichessStream<Error = Esp32LichessError>>, Esp32LichessError> {
        Esp32LichessStreamImpl::connect(self.token, &self.game_id)
            .map(|s| Box::new(s) as Box<dyn LichessStream<Error = Esp32LichessError>>)
    }
}

struct Esp32LichessStreamImpl {
    /// The NDJSON stream connection -- held in Response state.
    /// We read bytes directly from this via embedded_io::Read.
    stream_conn: EspHttpConnection,
    /// Token for POST Authorization header.
    post_token: &'static str,
    /// Game ID for constructing POST URLs.
    game_id: String,
    /// Line buffer for NDJSON parsing -- reused across reads.
    line_buf: Vec<u8>,
}

impl Esp32LichessStreamImpl {
    fn connect(token: &'static str, game_id: &str) -> Result<Self, Esp32LichessError> {
        // Stream connection: 60s timeout for steady-state reads (AI thinking time).
        // esp-idf-svc sets timeout per-connection, not per-operation.
        let stream_config = HttpConfig {
            timeout: Some(std::time::Duration::from_secs(60)),
            crt_bundle_attach: Some(esp_crt_bundle_attach),
            ..Default::default()
        };

        let mut stream_conn = EspHttpConnection::new(&stream_config)
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        // Open the NDJSON stream using raw connection methods
        let url = format!("{LICHESS_BASE}/api/board/game/stream/{game_id}");
        let auth_header = format!("Bearer {token}");
        let headers = [("Authorization", auth_header.as_str())];

        stream_conn
            .initiate_request(Method::Get, &url, &headers)
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        stream_conn
            .initiate_response()
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        let status = stream_conn.status();
        if status != 200 {
            return Err(Esp32LichessError::Http(format!(
                "stream GET returned {status}"
            )));
        }

        Ok(Self {
            stream_conn,
            post_token: token,
            game_id: game_id.to_string(),
            line_buf: Vec::with_capacity(4096),
        })
    }

    fn read_line(&mut self) -> Result<Option<String>, Esp32LichessError> {
        self.line_buf.clear();
        let mut byte = [0u8; 1];
        loop {
            match self.stream_conn.read(&mut byte) {
                Ok(0) => {
                    // Stream closed
                    if self.line_buf.is_empty() {
                        return Ok(None);
                    }
                    let line = String::from_utf8_lossy(&self.line_buf).into_owned();
                    return Ok(Some(line));
                }
                Ok(_) => {
                    if byte[0] == b'\n' {
                        let line = String::from_utf8_lossy(&self.line_buf).into_owned();
                        return Ok(Some(line));
                    }
                    self.line_buf.push(byte[0]);
                }
                Err(e) => {
                    return Err(Esp32LichessError::Io(e.to_string()));
                }
            }
        }
    }
}

impl LichessStream for Esp32LichessStreamImpl {
    type Error = Esp32LichessError;

    fn next_event(&mut self) -> Option<Result<GameEvent, Esp32LichessError>> {
        loop {
            match self.read_line() {
                Ok(Some(line)) if line.trim().is_empty() => continue, // keep-alive ping
                Ok(Some(line)) => {
                    match parse_game_event(&line) {
                        Some(Ok(event)) => return Some(Ok(event)),
                        Some(Err(e)) => return Some(Err(Esp32LichessError::Parse(e))),
                        None => continue, // unknown event type, skip
                    }
                }
                Ok(None) => return None, // stream closed
                Err(e) => return Some(Err(e)),
            }
        }
    }

    fn make_move(&mut self, uci_move: &str) -> Result<(), Esp32LichessError> {
        // Fresh connection per move. ESP-IDF's HTTP client has known bugs
        // with connection reuse after POST requests (esp-idf #5117, #17605).
        // A fresh TLS handshake costs ~2s but is completely reliable.
        let config = HttpConfig {
            timeout: Some(std::time::Duration::from_secs(10)),
            crt_bundle_attach: Some(esp_crt_bundle_attach),
            ..Default::default()
        };

        let mut client = HttpClient::wrap(
            EspHttpConnection::new(&config).map_err(|e| Esp32LichessError::Http(e.to_string()))?,
        );

        let url = format!(
            "{LICHESS_BASE}/api/board/game/{}/move/{uci_move}",
            self.game_id
        );
        let auth_header = format!("Bearer {}", self.post_token);
        let headers = [
            ("Authorization", auth_header.as_str()),
            ("Content-Length", "0"),
        ];

        let request = client
            .request(Method::Post, &url, &headers)
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        let response = request
            .submit()
            .map_err(|e| Esp32LichessError::Http(e.to_string()))?;

        let status = response.status();
        if status != 200 {
            return Err(Esp32LichessError::Http(format!(
                "make_move returned {status}"
            )));
        }

        Ok(())
    }
}
