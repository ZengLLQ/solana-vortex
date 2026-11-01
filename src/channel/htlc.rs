//! Hash Time-Locked Contract (HTLC) conditional payment structures.

use crate::crypto::sha256::sha256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelParty {
    PartyA,
    PartyB,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HTLCOrder {
    pub htlc_id: u64,
    pub sender: ChannelParty,
    pub amount: u64,
    pub hash_lock: [u8; 32],
    pub expiry_slot: u64,
    pub claimed: bool,
    pub refunded: bool,
}

impl HTLCOrder {
    pub fn new(
        htlc_id: u64,
        sender: ChannelParty,
        amount: u64,
        hash_lock: [u8; 32],
        expiry_slot: u64,
    ) -> Self {
        Self {
            htlc_id,
            sender,
            amount,
            hash_lock,
            expiry_slot,
            claimed: false,
            refunded: false,
        }
    }

    /// Claim HTLC by revealing the secret cryptographic preimage.
    pub fn claim(&mut self, preimage: &[u8]) -> Result<u64, &'static str> {
        if self.claimed {
            return Err("HTLC already claimed");
        }
        if self.refunded {
            return Err("HTLC already refunded");
        }

        let computed_hash = sha256(preimage);
        if computed_hash != self.hash_lock {
            return Err("Invalid secret preimage for hash lock");
        }

        self.claimed = true;
        Ok(self.amount)
    }

    /// Refund expired HTLC back to sender if time lock has passed.
    pub fn refund(&mut self, current_slot: u64) -> Result<u64, &'static str> {
        if self.claimed {
            return Err("HTLC already claimed");
        }
        if self.refunded {
            return Err("HTLC already refunded");
        }
        if current_slot < self.expiry_slot {
            return Err("HTLC time lock has not expired yet");
        }

        self.refunded = true;
        Ok(self.amount)
    }
}
