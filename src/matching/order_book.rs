//! Fast-path limit order matching engine for high-throughput state channel trading.

use crate::channel::htlc::ChannelParty;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    pub order_id: u64,
    pub side: OrderSide,
    pub price: u64,    // Price in quote lamports per unit
    pub quantity: u64, // Remaining quantity of units
    pub owner: ChannelParty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeExecution {
    pub buyer: ChannelParty,
    pub seller: ChannelParty,
    pub price: u64,
    pub quantity: u64,
    pub quote_amount: u64, // Total lamports transferred (price * quantity)
}

pub struct OrderBook {
    pub next_order_id: u64,
    pub bids: Vec<Order>, // Sorted descending by price
    pub asks: Vec<Order>, // Sorted ascending by price
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            next_order_id: 1,
            bids: Vec::new(),
            asks: Vec::new(),
        }
    }

    /// Place a new order and match against existing book with price-time priority.
    pub fn place_limit_order(
        &mut self,
        side: OrderSide,
        price: u64,
        mut quantity: u64,
        owner: ChannelParty,
    ) -> (u64, Vec<TradeExecution>) {
        let order_id = self.next_order_id;
        self.next_order_id += 1;

        let mut trades = Vec::new();

        match side {
            OrderSide::Buy => {
                // Match against asks while best_ask <= buy_price
                let mut ask_idx = 0;
                while ask_idx < self.asks.len() && quantity > 0 {
                    let best_ask = &mut self.asks[ask_idx];
                    if best_ask.price > price {
                        break;
                    }

                    let match_qty = quantity.min(best_ask.quantity);
                    let execution_price = best_ask.price; // Maker gets passive price

                    trades.push(TradeExecution {
                        buyer: owner,
                        seller: best_ask.owner,
                        price: execution_price,
                        quantity: match_qty,
                        quote_amount: match_qty * execution_price,
                    });

                    quantity -= match_qty;
                    best_ask.quantity -= match_qty;

                    if best_ask.quantity == 0 {
                        self.asks.remove(ask_idx);
                    } else {
                        ask_idx += 1;
                    }
                }

                // If resting quantity remains, insert into bids (descending price order)
                if quantity > 0 {
                    let insert_pos = self
                        .bids
                        .iter()
                        .position(|b| b.price < price)
                        .unwrap_or(self.bids.len());

                    self.bids.insert(
                        insert_pos,
                        Order {
                            order_id,
                            side: OrderSide::Buy,
                            price,
                            quantity,
                            owner,
                        },
                    );
                }
            }
            OrderSide::Sell => {
                // Match against bids while best_bid >= sell_price
                let mut bid_idx = 0;
                while bid_idx < self.bids.len() && quantity > 0 {
                    let best_bid = &mut self.bids[bid_idx];
                    if best_bid.price < price {
                        break;
                    }

                    let match_qty = quantity.min(best_bid.quantity);
                    let execution_price = best_bid.price; // Maker gets passive price

                    trades.push(TradeExecution {
                        buyer: best_bid.owner,
                        seller: owner,
                        price: execution_price,
                        quantity: match_qty,
                        quote_amount: match_qty * execution_price,
                    });

                    quantity -= match_qty;
                    best_bid.quantity -= match_qty;

                    if best_bid.quantity == 0 {
                        self.bids.remove(bid_idx);
                    } else {
                        bid_idx += 1;
                    }
                }

                // If resting quantity remains, insert into asks (ascending price order)
                if quantity > 0 {
                    let insert_pos = self
                        .asks
                        .iter()
                        .position(|a| a.price > price)
                        .unwrap_or(self.asks.len());

                    self.asks.insert(
                        insert_pos,
                        Order {
                            order_id,
                            side: OrderSide::Sell,
                            price,
                            quantity,
                            owner,
                        },
                    );
                }
            }
        }

        (order_id, trades)
    }
}
