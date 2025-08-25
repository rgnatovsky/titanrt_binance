use titanrt::connector::{Kind, StreamDescriptor, Venue};
use titanrt::utils::backoff::ReconnectCfg;
use titanrt::utils::CorePickPolicy;

pub const STREAM_VENUE: &str = "futures";
pub const STREAM_KIND: &str = "ws_api";

#[derive(Clone, Debug)]
pub struct BinanceFutWsApiStream {
    pub connect_timeout_ms: u64,
    pub reconnect_cfg: ReconnectCfg,
    pub max_pending_actions: Option<usize>,
    pub max_pending_events: Option<usize>,
    pub core_pick_policy: Option<CorePickPolicy>,
}

impl StreamDescriptor for BinanceFutWsApiStream {
    fn venue(&self) -> impl Venue {
        STREAM_VENUE
    }

    fn kind(&self) -> impl Kind {
        STREAM_KIND
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
        false
    }
}
