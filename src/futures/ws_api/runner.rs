use crate::connector::BinanceConnector;
use crate::futures::ws_api::descriptor::BinanceFutWsApiStream;
use anyhow::{anyhow, Context};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::time::Instant;
use std::{
    net::{TcpStream, ToSocketAddrs},
    sync::Arc,
    time::Duration,
};
use titanrt::connector::errors::{StreamError, StreamResult};
use titanrt::connector::{Hook, HookArgs, IntoHook, RuntimeCtx, StreamRunner, StreamSpawner};
use titanrt::io::ringbuffer::RingSender;
use titanrt::prelude::{BaseRx, TxPairExt};
use titanrt::utils::backoff::Backoff;
use titanrt::utils::StateMarker;
use tungstenite::handshake::client::Response;
use tungstenite::protocol::{WebSocket, WebSocketConfig};
use tungstenite::{Message, Utf8Bytes};

pub const HOST: &str = "ws-fapi.binance.com";
pub const ADDR: &str = "ws-fapi.binance.com:443";
pub const CONNECT_URL: &str = "wss://ws-fapi.binance.com:443/ws-fapi/v1";
// TODO optional config for const
#[derive(Debug, Clone)]
pub struct BinanceFutWsApiAction {
    pub id: String,
    pub method: String,
    pub params: Vec<String>,
}

#[derive(Debug)]
pub enum BinanceFutWsApiEvent {
    WsTxt(Utf8Bytes),
    WsClose(Instant),
    WsUnknown(Message),
    WsErr(tungstenite::Error),
}

impl<E, S> StreamSpawner<BinanceFutWsApiStream, E, S> for BinanceConnector
where
    E: TxPairExt,
    S: StateMarker,
{
}

impl<E, S> StreamRunner<BinanceFutWsApiStream, E, S> for BinanceConnector
where
    E: TxPairExt,
    S: StateMarker,
{
    type Config = ();
    type ActionTx = RingSender<BinanceFutWsApiAction>;
    type RawEvent = BinanceFutWsApiEvent;
    type HookResult = ();

    fn build_config(&mut self, _desc: &BinanceFutWsApiStream) -> anyhow::Result<Self::Config> {
        Ok(())
    }

    fn run<H>(
        mut ctx: RuntimeCtx<Self::Config, BinanceFutWsApiStream, Self::ActionTx, E, S>,
        hook: H,
    ) -> StreamResult<()>
    where
        H: IntoHook<BinanceFutWsApiEvent, E, S, BinanceFutWsApiStream, Self::HookResult>,
    {
        let mut hook = hook.into_hook();

        let mut backoff = Backoff::new(ctx.desc.reconnect_cfg.clone());

        let sni = ServerName::try_from(HOST).context("SNI")?;

        let mut roots = RootCertStore::empty();

        for cert in rustls_native_certs::load_native_certs().certs {
            roots.add(cert).ok(); // пропускаем битые
        }

        let tls_cfg = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );

        'reconnect: loop {
            if ctx.cancel.is_cancelled() {
                break Err(StreamError::Cancelled);
            }

            let target = match ADDR.to_socket_addrs() {
                Ok(mut addrs) => addrs
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("no addr for {}", ADDR))?,
                Err(e) => {
                    tracing::error!("resolve {ADDR}: {e}");
                    return Err(StreamError::Unknown(anyhow!(
                        "resolve {ADDR}: {e}",
                        ADDR = ADDR
                    )));
                }
            };

            let tcp = TcpStream::connect_timeout(
                &target,
                Duration::from_millis(ctx.desc.connect_timeout_ms),
            )
            .with_context(|| format!("tcp connect {target}"))?;

            tcp.set_nodelay(true)
                .map_err(|e| anyhow::anyhow!("set_nodelay: {e}"))?;

            let conn = ClientConnection::new(tls_cfg.clone(), sni.clone())
                .map_err(|e| anyhow::anyhow!("tls conn: {e}"))?;

            let tls = StreamOwned::new(conn, tcp);

            let conf = WebSocketConfig::default()
                .read_buffer_size(256 * 1024)
                .write_buffer_size(256 * 1024);

            let (mut ws, _resp): (WebSocket<_>, Response) =
                tungstenite::client::client_with_config(CONNECT_URL, tls, Some(conf))
                    .map_err(|e| anyhow::anyhow!("websocket handshake {CONNECT_URL}: {e}"))?;

            {
                let s = ws.get_mut();

                s.sock
                    .set_nonblocking(true)
                    .map_err(|e| anyhow::anyhow!("set_nonblocking: {e}"))?;
            }

            ctx.health.up();

            'read: loop {
                if ctx.cancel.is_cancelled() {
                    break 'reconnect Err(StreamError::Cancelled);
                }

                match ws.read() {
                    Ok(Message::Text(txt)) => {
                        hook.call(HookArgs::new(
                            &BinanceFutWsApiEvent::WsTxt(txt),
                            &mut ctx.event_tx,
                            &ctx.state,
                            &ctx.desc,
                            &ctx.health,
                        ));
                    }
                    Ok(Message::Ping(p)) => {
                        ws.send(Message::Pong(p)).map_err(|e| {
                            ctx.health.down();
                            tracing::error!("websocket write ping: {e}");
                            anyhow::anyhow!("write: {e}")
                        })?;
                        tracing::debug!("websocket write ping");
                    }

                    Ok(Message::Close(_)) => {
                        hook.call(HookArgs::new(
                            &BinanceFutWsApiEvent::WsClose(Instant::now()),
                            &mut ctx.event_tx,
                            &ctx.state,
                            &ctx.desc,
                            &ctx.health,
                        ));

                        ctx.health.down();
                        break 'read;
                    }
                    Ok(unknown) => {
                        hook.call(HookArgs::new(
                            &BinanceFutWsApiEvent::WsUnknown(unknown),
                            &mut ctx.event_tx,
                            &ctx.state,
                            &ctx.desc,
                            &ctx.health,
                        ));
                    }
                    Err(tungstenite::Error::Io(e))
                        if e.kind() == std::io::ErrorKind::WouldBlock =>
                    {
                        while let Ok(action) = ctx.action_rx.try_recv() {
                            // TODO
                            std::hint::spin_loop();
                        }
                    }
                    Err(e) => {
                        hook.call(HookArgs::new(
                            &BinanceFutWsApiEvent::WsErr(e),
                            &mut ctx.event_tx,
                            &ctx.state,
                            &ctx.desc,
                            &ctx.health,
                        ));
                        ctx.health.down();
                        break 'read;
                    }
                }
            }

            if backoff.should_reset() {
                backoff.on_success();
            }

            if let Some(0) = backoff.attempts_left() {
                return Err(StreamError::WebSocket(anyhow!(
                    "reconnect attempts exhausted"
                )));
            }

            let delay = backoff.next_delay();

            if !ctx.cancel.sleep_cancellable(delay) {
                break Err(StreamError::Cancelled);
            }
        }
    }
}
