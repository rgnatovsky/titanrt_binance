use crate::de::de_f64;
use crate::connector::BinanceConnector;
use crate::{StreamKind, BINANCE_VENUE};
use anyhow::Context;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use serde::Deserialize;
use std::{
    net::{TcpStream, ToSocketAddrs},
    sync::Arc,
    time::{Duration, Instant},
};

use titanrt::connector::errors::StreamResult;
use titanrt::connector::{
    AnyConnector, BaseConnector, Kind, RuntimeCtx, StreamDescriptor, StreamRunner, StreamSpawner,
    Venue,
};
use titanrt::prelude::{NullTx, TxPairExt};
use titanrt::utils::{CorePickPolicy, StateCell, StateMarker};
use tungstenite::handshake::client::Response;
use tungstenite::{
    protocol::{WebSocket, WebSocketConfig},
    Message,
};
use url::Url;

#[derive(Clone, Debug)]
pub struct Bbo {
    pub symbol: String,
    pub bid_px: f64,
    pub bid_sz: f64,
    pub ask_px: f64,
    pub ask_sz: f64,
    pub u: u64,
    pub ts_recv: Instant,
}

#[derive(Deserialize)]
pub struct BookTicker {
    u: u64,

    #[serde(deserialize_with = "de_f64")]
    b: f64,
    #[serde(deserialize_with = "de_f64")]
    B: f64,
    #[serde(deserialize_with = "de_f64")]
    a: f64,
    #[serde(deserialize_with = "de_f64")]
    A: f64,
}
#[derive(Clone, Debug)]
pub struct BinanceBboCfg {
    pub symbol_raw: String,
    /// spot: wss://stream.binance.com:9443; futures: wss://fstream.binance.com
    pub host: String,
    pub port: u16, // 9443 для spot, 443 для futures
    pub connect_timeout_ms: u64,
    pub write_timeout_ms: u64,
    pub stale_kill_ms: u64,    // напр. 1000
    pub reconnect_min_ms: u64, // напр. 200
    pub reconnect_max_ms: u64, // напр. 5_000
    pub max_pending_actions: Option<usize>,
    pub max_pending_events: Option<usize>,
    pub core_pick_policy: Option<CorePickPolicy>,
    pub health_at_start: bool,
}

impl StreamDescriptor for BinanceBboCfg {
    fn venue(&self) -> impl Venue {
        BINANCE_VENUE
    }

    fn kind(&self) -> impl Kind {
        StreamKind::BBO
    }

    fn max_pending_actions(&self) -> Option<usize> {
        self.max_pending_actions
    }

    fn max_pending_events(&self) -> Option<usize> {
        self.max_pending_events
    }

    fn core_pick_policy(&self) -> Option<CorePickPolicy> {
        self.core_pick_policy
    }

    fn health_at_start(&self) -> bool {
        self.health_at_start
    }
}

impl<C, E, S> StreamSpawner<BinanceBboCfg, E, S> for AnyConnector<C>
where
    C: BaseConnector,
    E: TxPairExt,
    S: StateMarker,
{
}

impl<C, E, S> StreamRunner<BinanceBboCfg, E, S> for AnyConnector<C>
where
    C: BaseConnector,
    E: TxPairExt,
    S: StateMarker,
{
    type Config = ();
    type ActionTx = NullTx;
    type RawEvent = BookTicker;
    type Hook = fn(&Self::RawEvent, &mut E, &StateCell<S>);

    fn build_config(&mut self, _desc: &BinanceBboCfg) -> anyhow::Result<Self::Config> {
        Ok(())
    }

    fn run(mut ctx: RuntimeCtx<BinanceBboCfg, Self, E, S>, hook: Self::Hook) -> StreamResult<()> {
        let addr = format!("{}:{}", ctx.desc.host, ctx.desc.port);

        let mut addrs = addr
            .to_socket_addrs()
            .with_context(|| format!("resolve {addr}"))?;
        let target = addrs
            .next()
            .ok_or_else(|| anyhow::anyhow!("no addr for {}", addr))?;

        let path = format!("/ws/{}@bookTicker", ctx.desc.symbol_raw);
        let url = Url::parse(&format!(
            "wss://{}:{}{}",
            ctx.desc.host, ctx.desc.port, path
        ))
        .map_err(|e| anyhow::anyhow!("bad url: {e}"))?;

        let mut roots = RootCertStore::empty();

        for cert in rustls_native_certs::load_native_certs().certs {
            roots.add(cert).ok(); // пропускаем битые
        }

        let tls_cfg = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );

        let sni = ServerName::try_from(ctx.desc.host.clone()).context("SNI")?;

        'reconnect: loop {
            let tcp = TcpStream::connect_timeout(
                &target,
                Duration::from_millis(ctx.desc.connect_timeout_ms),
            )
            .with_context(|| format!("connect {target}"))?;

            tcp.set_nodelay(true)
                .map_err(|e| anyhow::anyhow!("set_nodelay: {e}"))?;
            tcp.set_nonblocking(true)
                .map_err(|e| anyhow::anyhow!("set_nonblocking: {e}"))?;
            tcp.set_write_timeout(Some(Duration::from_millis(ctx.desc.write_timeout_ms)))
                .map_err(|e| anyhow::anyhow!("set_write_timeout: {e}"))?;

            let conn = ClientConnection::new(tls_cfg.clone(), sni.clone())
                .map_err(|e| anyhow::anyhow!("tls conn: {e}"))?;
            let tls = StreamOwned::new(conn, tcp);

            let conf = WebSocketConfig::default()
                .read_buffer_size(256 * 1024)
                .write_buffer_size(256 * 1024);

            let (mut ws, _resp): (WebSocket<_>, Response) =
                tungstenite::client::client_with_config(url.as_str(), tls, Some(conf))
                    .map_err(|e| anyhow::anyhow!("websocket handshake: {e}"))?;

            let mut last = Instant::now();

            ctx.health.up();

            'read: loop {
                match ws.read() {
                    Ok(Message::Text(txt)) => {
                        last = Instant::now();
                        // желательно: парсить borrowed из &mut [u8] (simd-json),
                        // здесь для эскиза оставлю serde_json:
                        if let Ok(bt) = serde_json::from_str::<BookTicker>(&txt) {
                            hook(&bt, &mut ctx.event_tx, &ctx.state);
                        }
                    }
                    Ok(Message::Ping(p)) => {
                        ws.write(Message::Pong(p)).map_err(|e| {
                            ctx.health.down();
                            anyhow::anyhow!("write: {e}")
                        })?;
                        last = Instant::now();
                    }
                    Ok(Message::Pong(_)) | Ok(Message::Binary(_)) => {}
                    Ok(Message::Close(_)) => {
                        ctx.health.down();
                        break;
                    }
                    Ok(_) => continue,
                    Err(tungstenite::Error::Io(e))
                        if e.kind() == std::io::ErrorKind::WouldBlock =>
                    {
                        // no log; tight spin + редкий yield
                        std::hint::spin_loop();
                        if last.elapsed().as_millis() as u64 > ctx.desc.stale_kill_ms {
                            ctx.health.down();
                            break 'read;
                        }
                    }
                    Err(e) => {
                        tracing::error!("websocket read: {e}");
                        ctx.health.down();
                        break 'read;
                    }
                }
            }
        }
    }
}
