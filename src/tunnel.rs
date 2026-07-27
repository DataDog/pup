use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use russh::keys::{decode_secret_key, PublicKey};
use russh::server::{Auth, ChannelOpenHandle, Handler, Msg, Session};
use russh::ChannelId;
use serde::{Deserialize, Serialize};
use ssh_key::{rand_core::OsRng, Algorithm, EcdsaCurve, LineEnding, PrivateKey as SshKey};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelInfo {
    pub host: String,
    pub id: String,
    #[serde(rename = "privateKey")]
    pub private_key: String,
}

pub struct Tunnel {
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Tunnel {
    pub async fn start(url: &str, test_ids: Vec<String>) -> Result<(TunnelInfo, Tunnel)> {
        let algo = Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        };
        let mut rng = OsRng;

        let host_ssh_key = SshKey::random(&mut rng, algo.clone())
            .map_err(|e| anyhow!("failed to generate SSH host key: {e}"))?;
        let host_pem = host_ssh_key
            .to_openssh(LineEnding::LF)
            .map_err(|e| anyhow!("failed to encode host key: {e}"))?;
        let host_key = decode_secret_key(&host_pem, None)
            .map_err(|e| anyhow!("failed to decode host key: {e}"))?;

        let client_ssh_key = SshKey::random(&mut rng, algo)
            .map_err(|e| anyhow!("failed to generate SSH client key: {e}"))?;
        let private_key_str = client_ssh_key
            .to_openssh(LineEnding::LF)
            .map_err(|e| anyhow!("failed to encode client key: {e}"))?
            .to_string();
        let client_key = decode_secret_key(&private_key_str, None)
            .map_err(|e| anyhow!("failed to decode client key: {e}"))?;
        let client_public = client_key.public_key().clone();

        // Normalize: wss://host?... → wss://host/?... (tungstenite needs a path)
        let normalized_url = if let Some(idx) = url.find('?') {
            let before = &url[..idx];
            if !before.ends_with('/') && before.contains("://") {
                format!("{}/?{}", before, &url[idx + 1..])
            } else {
                url.to_string()
            }
        } else {
            url.to_string()
        };

        let (ws, _) = tokio_tungstenite::connect_async(normalized_url.as_str())
            .await
            .map_err(|e| {
                use tokio_tungstenite::tungstenite::Error::Http;
                if let Http(resp) = &e {
                    if let Some(body) = resp.body() {
                        return anyhow!(
                            "WebSocket HTTP {}: {}",
                            resp.status(),
                            String::from_utf8_lossy(body)
                        );
                    }
                }
                anyhow!("WebSocket connection failed: {e}")
            })?;

        let (mut ws_write, mut ws_read) = ws.split();

        let first_msg = ws_read
            .next()
            .await
            .ok_or_else(|| anyhow!("WebSocket closed before receiving connection info"))??;

        let first_bytes = match first_msg {
            Message::Binary(b) => b.to_vec(),
            Message::Text(t) => t.as_bytes().to_vec(),
            other => anyhow::bail!("unexpected first WebSocket message: {other:?}"),
        };

        #[derive(Deserialize)]
        struct ServerInfo {
            host: String,
            id: String,
        }

        let server_info: ServerInfo = serde_json::from_slice(&first_bytes)
            .map_err(|e| anyhow!("invalid connection info from tunnel service: {e}"))?;

        let tunnel_info = TunnelInfo {
            host: server_info.host,
            id: server_info.id,
            private_key: private_key_str,
        };

        // Bridge WebSocket ↔ yamux via a tokio duplex channel pair
        let (our_side, yamux_side) = tokio::io::duplex(256 * 1024);
        let (mut our_read, mut our_write) = tokio::io::split(our_side);

        // WS → yamux
        tokio::spawn(async move {
            while let Some(msg) = ws_read.next().await {
                match msg {
                    Ok(Message::Binary(data)) => {
                        // Bind result first so write_all is not a side-effecting
                        // match guard expression; avoids clippy::collapsible_match.
                        let result = our_write.write_all(&data).await;
                        if result.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        });

        // yamux → WS
        tokio::spawn(async move {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                match our_read.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if ws_write
                            .send(Message::Binary(buf[..n].to_vec().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        let yamux_compat = yamux_side.compat();
        let mut yamux_conn =
            yamux::Connection::new(yamux_compat, yamux::Config::default(), yamux::Mode::Server);

        let ssh_config = Arc::new(russh::server::Config {
            keys: vec![host_key],
            inactivity_timeout: None,
            auth_rejection_time: std::time::Duration::from_millis(100),
            auth_rejection_time_initial: None,
            ..Default::default()
        });

        let test_ids = Arc::new(test_ids);
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            tokio::pin!(stop_rx);
            let mut stream_source =
                futures::stream::poll_fn(move |cx| yamux_conn.poll_next_inbound(cx));

            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    maybe_stream = stream_source.next() => {
                        match maybe_stream {
                            Some(Ok(yamux_stream)) => {
                                let tokio_stream = Box::pin(yamux_stream.compat());
                                let handler = SshHandler {
                                    test_ids: Arc::clone(&test_ids),
                                    client_public_key: client_public.clone(),
                                    tcp_writers: HashMap::new(),
                                };
                                let cfg = Arc::clone(&ssh_config);
                                tokio::spawn(async move {
                                    let _ = russh::server::run_stream(cfg, tokio_stream, handler).await;
                                });
                            }
                            Some(Err(e)) => {
                                eprintln!("tunnel: yamux error: {e}");
                                break;
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        Ok((
            tunnel_info,
            Tunnel {
                stop_tx: Some(stop_tx),
            },
        ))
    }

    pub fn stop(mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

struct SshHandler {
    test_ids: Arc<Vec<String>>,
    client_public_key: PublicKey,
    tcp_writers: HashMap<ChannelId, tokio::net::tcp::OwnedWriteHalf>,
}

impl Handler for SshHandler {
    type Error = anyhow::Error;

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        if !self.test_ids.contains(&user.to_string()) {
            return Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            });
        }
        if public_key == &self.client_public_key {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            })
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: russh::Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: russh::Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel_id = channel.id();
        let handle = session.handle();

        // On error the `?` drops `reply`, which sends an automatic channel-open
        // rejection to the client before the error tears down the session.
        let tcp = TcpStream::connect((host_to_connect, port_to_connect as u16))
            .await
            .map_err(|e| {
                anyhow!("TCP connect to {host_to_connect}:{port_to_connect} failed: {e}")
            })?;
        let (mut tcp_read, tcp_write) = tcp.into_split();
        self.tcp_writers.insert(channel_id, tcp_write);

        // Confirm the channel before starting the pump below, so no data is
        // sent to a channel the client has not seen accepted yet.
        reply.accept().await;

        tokio::spawn(async move {
            let mut buf = vec![0u8; 16 * 1024];
            let handle = handle;
            loop {
                match tcp_read.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if handle.data(channel_id, buf[..n].to_vec()).await.is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = handle.eof(channel_id).await;
            let _ = handle.close(channel_id).await;
        });

        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(writer) = self.tcp_writers.get_mut(&channel) {
            writer.write_all(data).await?;
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.tcp_writers.remove(&channel);
        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.tcp_writers.remove(&channel);
        Ok(())
    }
}
