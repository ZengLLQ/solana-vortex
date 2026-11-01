//! SolanaVortex: High-throughput state channel settlement and dispute resolution engine on Solana.

pub mod channel {
    pub mod dispute;
    pub mod htlc;
    pub mod state;
}

pub mod crypto {
    pub mod sha256;
}

pub mod matching {
    pub mod order_book;
}

pub use channel::dispute::{ChannelStatus, StateChannelManager};
pub use channel::htlc::{ChannelParty, HTLCOrder};
pub use channel::state::ChannelState;
pub use crypto::sha256::sha256;
pub use matching::order_book::{Order, OrderBook, OrderSide, TradeExecution};
