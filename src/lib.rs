use std::fmt::{Display, Formatter};
use titanrt::connector::Kind;

pub mod bbo;
pub mod connector;
mod de;

pub const BINANCE_VENUE: &str = "binance";

pub enum StreamKind {
    BBO,
}

impl AsRef<str> for StreamKind {
    fn as_ref(&self) -> &str {
        match self {
            StreamKind::BBO => "bbo",
        }
    }
}

impl Display for StreamKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamKind::BBO => write!(f, "bbo"),
        }
    }
}
