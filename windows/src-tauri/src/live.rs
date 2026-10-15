// The Gemini Live voice: a spoken conversation over a websocket, opened only when the person has chosen it and
// saved a Gemini key. The key stays here (Credential Manager / Secret Service), never in the page. The page
// sends microphone audio while the mic button is held and plays what comes back.
//
// This is the one place the app sends the person's voice to a server, so it is off unless chosen in Settings,
// and Settings says so next to the choice. It has no tools: what the model says is sound and words only.
// The wire format (and its tests) is in coucou-assistant's live.rs.

use std::sync::Mutex;
use std::time::Duration;

use coucou_assistant::live::{self, LiveEvent, Speaker};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

/// A session with nobody talking to it closes itself after this long.
const IDLE: Duration = Duration::from_secs(120);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// What the page is told. Short codes only; the page turns them into lines in the assistant's language.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    Ready,
    /// 16-bit PCM, 24 kHz, mono, base64.
    Audio { data: String },
    Text { role: &'static str, text: String },
    Interrupted,
    TurnComplete,
    /// "no_key", "auth", "connect" or "closed".
    Error { code: &'static str },
    Closed,
}

pub enum Cmd {
    Audio(Vec<u8>),
    Start,
    End,
    Close,
}

#[derive(Default)]
pub struct Live {
    tx: Mutex<Option<UnboundedSender<Cmd>>>,
}

impl Live {
    pub fn replace(&self, tx: Option<UnboundedSender<Cmd>>) {
        if let Some(old) = std::mem::replace(&mut *self.tx.lock().unwrap(), tx) {
            let _ = old.send(Cmd::Close);
        }
    }

    pub fn send(&self, cmd: Cmd) {
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            let _ = tx.send(cmd);
        }
    }
}

fn auth_failure(status: u16) -> bool {
    matches!(status, 400 | 401 | 403)
}

fn mentions_key(reason: &str) -> bool {
    let r = reason.to_ascii_lowercase();
    r.contains("api key") || r.contains("api_key") || r.contains("permission") || r.contains("unauthenticated")
}

fn emit_all(events: Vec<LiveEvent>, emit: &impl Fn(Event)) {
    for event in events {
        match event {
            LiveEvent::Ready | LiveEvent::GoAway => {}
            LiveEvent::Audio(bytes) => {
                use base64::{engine::general_purpose::STANDARD, Engine as _};
                emit(Event::Audio { data: STANDARD.encode(bytes) });
            }
            LiveEvent::Text { speaker, text } => emit(Event::Text {
                role: if speaker == Speaker::User { "user" } else { "model" },
                text,
            }),
            LiveEvent::Interrupted => emit(Event::Interrupted),
            LiveEvent::TurnComplete => emit(Event::TurnComplete),
        }
    }
}

fn parse_frame(message: &Message) -> Vec<LiveEvent> {
    // The service sends its JSON in text frames, and sometimes in binary ones.
    match message {
        Message::Text(t) => live::parse_server_message(t.as_str()),
        Message::Binary(b) => std::str::from_utf8(b).map(live::parse_server_message).unwrap_or_default(),
        _ => vec![],
    }
}

trait Io: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> Io for T {}

/// Opens the websocket: TCP, then TLS for `wss` with the same rustls (ring) and web roots as the phone relay,
/// so no second crypto library is linked. A plain `ws` address is only used by the tests' local server.
async fn connect(url: &str) -> Result<tokio_tungstenite::WebSocketStream<Box<dyn Io>>, WsError> {
    use std::sync::Arc;
    use tokio_rustls::rustls::pki_types::ServerName;
    use tokio_rustls::rustls::{crypto::ring as ring_provider, ClientConfig, RootCertStore};
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let request = url.into_client_request()?;
    let uri = request.uri().clone();
    let tls = uri.scheme_str() == Some("wss");
    let host = uri.host().ok_or(WsError::Url(tokio_tungstenite::tungstenite::error::UrlError::NoHostName))?.to_string();
    let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
    let tcp = tokio::net::TcpStream::connect((host.as_str(), port)).await.map_err(WsError::Io)?;
    let _ = tcp.set_nodelay(true);
    let stream: Box<dyn Io> = if tls {
        let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = ClientConfig::builder_with_provider(Arc::new(ring_provider::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("the ring provider supports the default TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = ServerName::try_from(host).map_err(|_| WsError::Io(std::io::ErrorKind::InvalidInput.into()))?;
        Box::new(tokio_rustls::TlsConnector::from(Arc::new(config)).connect(name, tcp).await.map_err(WsError::Io)?)
    } else {
        Box::new(tcp)
    };
    tokio_tungstenite::client_async(request, stream).await.map(|(ws, _)| ws)
}

/// Runs one conversation from connect to close. `emit` carries events to the page.
pub async fn run_session(
    url: String,
    setup: String,
    mut rx: UnboundedReceiver<Cmd>,
    emit: impl Fn(Event) + Send + 'static,
) {
    let connected = tokio::time::timeout(CONNECT_TIMEOUT, connect(&url)).await;
    let ws = match connected {
        Ok(Ok(ws)) => ws,
        Ok(Err(WsError::Http(response))) => {
            emit(Event::Error { code: if auth_failure(response.status().as_u16()) { "auth" } else { "connect" } });
            emit(Event::Closed);
            return;
        }
        _ => {
            emit(Event::Error { code: "connect" });
            emit(Event::Closed);
            return;
        }
    };
    let (mut sink, mut stream) = ws.split();

    if sink.send(Message::text(setup)).await.is_err() {
        emit(Event::Error { code: "connect" });
        emit(Event::Closed);
        return;
    }

    // Wait for the service to accept the setup.
    let ready = tokio::time::timeout(CONNECT_TIMEOUT, async {
        while let Some(frame) = stream.next().await {
            match frame {
                Ok(Message::Close(frame)) => {
                    let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                    return Err(if mentions_key(&reason) { "auth" } else { "closed" });
                }
                Ok(message) => {
                    if parse_frame(&message).contains(&LiveEvent::Ready) {
                        return Ok(());
                    }
                }
                Err(_) => return Err("closed"),
            }
        }
        Err("closed")
    })
    .await;
    match ready {
        Ok(Ok(())) => emit(Event::Ready),
        Ok(Err(code)) => {
            emit(Event::Error { code });
            emit(Event::Closed);
            return;
        }
        Err(_) => {
            emit(Event::Error { code: "connect" });
            emit(Event::Closed);
            return;
        }
    }

    let idle = tokio::time::sleep(IDLE);
    tokio::pin!(idle);
    loop {
        tokio::select! {
            cmd = rx.recv() => {
                idle.as_mut().reset(tokio::time::Instant::now() + IDLE);
                let message = match cmd {
                    Some(Cmd::Audio(pcm)) => live::audio_message(&pcm),
                    Some(Cmd::Start) => Some(live::activity_start()),
                    Some(Cmd::End) => Some(live::activity_end()),
                    Some(Cmd::Close) | None => break,
                };
                if let Some(text) = message {
                    if sink.send(Message::text(text)).await.is_err() {
                        emit(Event::Error { code: "closed" });
                        break;
                    }
                }
            }
            frame = stream.next() => {
                match frame {
                    Some(Ok(Message::Close(frame))) => {
                        let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                        emit(Event::Error { code: if mentions_key(&reason) { "auth" } else { "closed" } });
                        break;
                    }
                    Some(Ok(message)) => emit_all(parse_frame(&message), &emit),
                    Some(Err(_)) | None => {
                        emit(Event::Error { code: "closed" });
                        break;
                    }
                }
            }
            _ = &mut idle => break,
        }
    }
    let _ = sink.send(Message::Close(None)).await;
    emit(Event::Closed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use coucou_assistant::VoiceLanguage;
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::net::TcpListener;
    use tokio::sync::mpsc::unbounded_channel;

    /// A stand-in for the service on localhost: checks what it is sent and answers the way the reference says.
    async fn mock_server(listener: TcpListener, seen: Arc<StdMutex<Vec<String>>>) {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        // 1. setup
        let setup = ws.next().await.unwrap().unwrap();
        seen.lock().unwrap().push(setup.into_text().unwrap().to_string());
        // setupComplete in a binary frame, as the service sometimes does
        ws.send(Message::binary(r#"{"setupComplete":{}}"#)).await.unwrap();
        // 2. activityStart, audio, activityEnd
        for _ in 0..3 {
            let m = ws.next().await.unwrap().unwrap();
            seen.lock().unwrap().push(m.into_text().unwrap().to_string());
        }
        ws.send(Message::text(
            r#"{"serverContent":{"inputTranscription":{"text":"how far"},"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"AQACAA=="}}]},"outputTranscription":{"text":"I dey"}}}"#,
        ))
        .await
        .unwrap();
        ws.send(Message::text(r#"{"serverContent":{"interrupted":true}}"#)).await.unwrap();
        ws.send(Message::text(r#"{"serverContent":{"turnComplete":true}}"#)).await.unwrap();
        // wait for the client to close
        while let Some(Ok(m)) = ws.next().await {
            if m.is_close() {
                break;
            }
        }
    }

    #[tokio::test]
    async fn a_held_turn_goes_out_and_the_reply_comes_back() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let seen = Arc::new(StdMutex::new(vec![]));
        let server = tokio::spawn(mock_server(listener, seen.clone()));

        let (tx, rx) = unbounded_channel();
        let events = Arc::new(StdMutex::new(vec![]));
        let sink = events.clone();
        let setup = live::setup_message("gemini-3.8-live", "Puck", VoiceLanguage::Pidgin);
        let session = tokio::spawn(run_session(url, setup, rx, move |e| sink.lock().unwrap().push(e)));

        tx.send(Cmd::Start).unwrap();
        tx.send(Cmd::Audio(vec![1, 0, 2, 0])).unwrap();
        tx.send(Cmd::End).unwrap();
        // wait until the whole reply has arrived, then hang up
        for _ in 0..200 {
            if events.lock().unwrap().contains(&Event::TurnComplete) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tx.send(Cmd::Close).unwrap();
        tokio::time::timeout(Duration::from_secs(5), session).await.unwrap().unwrap();
        tokio::time::timeout(Duration::from_secs(5), server).await.unwrap().unwrap();

        let got = events.lock().unwrap().clone();
        assert_eq!(
            got,
            vec![
                Event::Ready,
                // one message carried all three: sound first, then the words
                Event::Audio { data: "AQACAA==".into() },
                Event::Text { role: "user", text: "how far".into() },
                Event::Text { role: "model", text: "I dey".into() },
                Event::Interrupted,
                Event::TurnComplete,
                Event::Closed,
            ]
        );
        let sent = seen.lock().unwrap().clone();
        assert!(sent[0].contains("\"setup\"") && sent[0].contains("models/gemini-3.8-live"));
        assert_eq!(sent[1], live::activity_start());
        assert!(sent[2].contains("audio/pcm;rate=16000"));
        assert_eq!(sent[3], live::activity_end());
    }

    #[tokio::test]
    async fn an_unreachable_service_is_one_error_then_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        drop(listener); // nothing listening
        let (_tx, rx) = unbounded_channel();
        let events = Arc::new(StdMutex::new(vec![]));
        let sink = events.clone();
        run_session(url, "{}".into(), rx, move |e| sink.lock().unwrap().push(e)).await;
        assert_eq!(*events.lock().unwrap(), vec![Event::Error { code: "connect" }, Event::Closed]);
    }

    #[tokio::test]
    async fn a_close_that_names_the_key_is_an_auth_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let _ = ws.next().await;
            let frame = tokio_tungstenite::tungstenite::protocol::CloseFrame {
                code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Policy,
                reason: "API key not valid. Please pass a valid API key.".into(),
            };
            let _ = ws.send(Message::Close(Some(frame))).await;
        });
        let (_tx, rx) = unbounded_channel();
        let events = Arc::new(StdMutex::new(vec![]));
        let sink = events.clone();
        run_session(url, "{}".into(), rx, move |e| sink.lock().unwrap().push(e)).await;
        assert_eq!(*events.lock().unwrap(), vec![Event::Error { code: "auth" }, Event::Closed]);
    }
}
