use crate::BINANCE_VENUE;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::sync::Arc;
use titanrt::connector::BaseConnector;
use titanrt::utils::{CancelToken, CoreStats};

#[derive(Clone, Deserialize, Serialize)]
pub struct BinanceConnectorConfig {
    pub default_max_cores: Option<usize>,
    pub specific_core_ids: Vec<usize>,
}

pub struct BinanceConnector {
    config: BinanceConnectorConfig,
    cancel_token: CancelToken,
    core_stats: Option<Arc<CoreStats>>,
}

impl BaseConnector for BinanceConnector {
    type Config = BinanceConnectorConfig;

    fn init(
        config: Self::Config,
        cancel_token: CancelToken,
        reserved_core_ids: Option<Vec<usize>>,
    ) -> anyhow::Result<Self> {
        let core_stats = if let Some(core_ids) = reserved_core_ids {
            Some(CoreStats::new(
                config.default_max_cores,
                config.specific_core_ids.clone(),
                core_ids,
            )?)
        } else {
            None
        };

        Ok(Self {
            config,
            cancel_token,
            core_stats,
        })
    }

    fn name(&self) -> impl AsRef<str> + Display {
        BINANCE_VENUE
    }

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn cancel_token(&self) -> &CancelToken {
        &self.cancel_token
    }

    fn cores_stats(&self) -> Option<Arc<CoreStats>> {
        self.core_stats.clone()
    }
}

impl Display for BinanceConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BinanceConnector")
    }
}
