use crate::futures::ws_api::runner::BinanceFutWsApiEvent;

#[test]
fn spawn_stream_smoke() {
    use crate::connector::{BinanceConnector, BinanceConnectorConfig};
    use crate::futures::ws_api::BinanceFutWsApiStream;
    use std::time::Duration;
    use titanrt::connector::{BaseConnector, HookArgs};
    use titanrt::io::ringbuffer::RingSender;
    use titanrt::utils::backoff::ReconnectCfg;
    use titanrt::utils::{CancelToken, CorePickPolicy, NullState};
    use tracing::Level;
    fn setup_simple_tracing(level: &str) {
        let level = match level.to_lowercase().as_str() {
            "trace" => Level::TRACE,
            "debug" => Level::DEBUG,
            "info" => Level::INFO,
            "warn" => Level::WARN,
            "error" => Level::ERROR,
            _ => Level::INFO,
        };

        tracing_subscriber::fmt().with_max_level(level).init();
    }
    fn hook(
        args: HookArgs<BinanceFutWsApiEvent, RingSender<String>, NullState, BinanceFutWsApiStream>,
    ) {
        println!("{:?}", args.raw);
    }

    setup_simple_tracing("debug");

    let cancel = CancelToken::new_root();

    let mut conn = BinanceConnector::init(
        BinanceConnectorConfig {
            default_max_cores: None,
            specific_core_ids: vec![],
        },
        cancel,
        Some(vec![1]),
    )
    .unwrap();

    let bbo_desc = BinanceFutWsApiStream {
        connect_timeout_ms: 5000,
        reconnect_cfg: ReconnectCfg {
            fast_attempts: 3,
            fast_delay_ms: 100,
            base_delay_ms: 300,
            max_delay_ms: 5000,
            factor: 0.7,
            reset_after_ms: 5000,
            max_retries: Some(5),
        },
        max_pending_actions: None,
        max_pending_events: None,
        core_pick_policy: Some(CorePickPolicy::Specific(5)),
    };

    let mut stream = conn.spawn_stream(bbo_desc, hook).unwrap();

    stream.recv(Some(Duration::from_millis(200000))).ok();

    if !stream.is_healthy() {
        println!("stream is not healthy");
        let res = stream.wait_to_end();
        println!("stream wait_to_end: {:?}", res);
        return;
    } else {
        println!("stream is healthy");
        stream.cancel();
    }

    drop(stream);
}
