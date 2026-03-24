use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::http::Method;
use embedded_svc::io::Write;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::sys::esp_crt_bundle_attach;
use shakmaty::Color;

use crate::lichess::{
    GameEvent, GameStateData, GameStatus, LichessClient, LichessGame, LichessStream,
};

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
    /// Reusable POST client for make_move requests.
    post_client: HttpClient<EspHttpConnection>,
    /// Token for POST Authorization header.
    post_token: &'static str,
    /// Game ID for constructing POST URLs.
    game_id: String,
    /// Line buffer for NDJSON parsing -- reused across reads.
    line_buf: Vec<u8>,
}

impl Esp32LichessStreamImpl {
    fn connect(token: &'static str, game_id: &str) -> Result<Self, Esp32LichessError> {
        // Stream connection: The spec calls for 10s on initial connection and
        // 60s for steady-state reads, but esp-idf-svc sets timeout per-connection,
        // not per-operation. We use 60s because the steady-state read timeout is
        // the critical one (AI thinking time). The initial HTTP handshake completes
        // in <1s on a working network; a 60s timeout just means slower failure
        // detection on a bad connection at startup, which is acceptable.
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

        // POST client: 10s timeout, reused across moves
        let post_config = HttpConfig {
            timeout: Some(std::time::Duration::from_secs(10)),
            crt_bundle_attach: Some(esp_crt_bundle_attach),
            ..Default::default()
        };

        let post_client = HttpClient::wrap(
            EspHttpConnection::new(&post_config)
                .map_err(|e| Esp32LichessError::Http(e.to_string()))?,
        );

        Ok(Self {
            stream_conn,
            post_client,
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
                        Some(result) => return Some(result),
                        None => continue, // unknown event type, skip
                    }
                }
                Ok(None) => return None, // stream closed
                Err(e) => return Some(Err(e)),
            }
        }
    }

    fn make_move(&mut self, uci_move: &str) -> Result<(), Esp32LichessError> {
        let url = format!(
            "{LICHESS_BASE}/api/board/game/{}/move/{uci_move}",
            self.game_id
        );
        let auth_header = format!("Bearer {}", self.post_token);
        let headers = [("Authorization", auth_header.as_str())];

        let request = self
            .post_client
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

        // Response is dropped here, freeing the borrow on post_client
        // so it can be reused for the next move.
        Ok(())
    }
}

// --- JSON parsing helpers ---

/// Minimal JSON string extractor -- avoids pulling in a JSON crate.
/// Finds `"key":"value"` or `"key": "value"` and returns value.
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let start = json.find(&pattern)? + pattern.len();
    let rest = &json[start..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Extract a nested JSON object as a raw string by brace-matching.
/// Given `"state":{"moves":"e2e4","status":"started"}`, returns
/// `{"moves":"e2e4","status":"started"}`.
fn extract_json_object(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let key_start = json.find(&pattern)?;
    let after_key = &json[key_start + pattern.len()..];
    let after_key = after_key.trim_start();
    let after_key = after_key.strip_prefix(':')?;
    let after_key = after_key.trim_start();

    if !after_key.starts_with('{') {
        return None;
    }

    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;
    for (i, ch) in after_key.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(after_key[..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_game_event(line: &str) -> Option<Result<GameEvent, Esp32LichessError>> {
    let event_type = match extract_json_string(line, "type") {
        Some(t) => t,
        None => {
            return Some(Err(Esp32LichessError::Parse(format!(
                "no 'type' in event: {line}"
            ))));
        }
    };

    match event_type.as_str() {
        "gameFull" => {
            let id = extract_json_string(line, "id").unwrap_or_default();
            let initial_fen =
                extract_json_string(line, "initialFen").unwrap_or_else(|| "startpos".to_string());
            let state_json = match extract_json_object(line, "state") {
                Some(s) => s,
                None => {
                    return Some(Err(Esp32LichessError::Parse(
                        "no 'state' object in gameFull".into(),
                    )));
                }
            };
            let state = match parse_state_data(&state_json) {
                Ok(s) => s,
                Err(e) => return Some(Err(e)),
            };
            Some(Ok(GameEvent::GameFull {
                id,
                initial_fen,
                state,
            }))
        }
        "gameState" => Some(parse_state_data(line).map(GameEvent::GameState)),
        // chatLine, opponentGone, etc. -- silently skip
        _ => {
            log::debug!("Ignoring Lichess event type: {event_type}");
            None
        }
    }
}

fn parse_state_data(json: &str) -> Result<GameStateData, Esp32LichessError> {
    let moves = extract_json_string(json, "moves").unwrap_or_default();
    let status_str = extract_json_string(json, "status")
        .ok_or_else(|| Esp32LichessError::Parse("no 'status' field".into()))?;
    let winner = extract_json_string(json, "winner").and_then(|w| match w.as_str() {
        "white" => Some(Color::White),
        "black" => Some(Color::Black),
        _ => None,
    });

    Ok(GameStateData {
        moves,
        status: GameStatus::from_str(&status_str),
        winner,
    })
}
