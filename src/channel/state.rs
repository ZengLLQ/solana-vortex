//! Bilateral State Channel State and cryptographic commitments.

use crate::channel::htlc::{ChannelParty, HTLCOrder};
use crate::crypto::sha256::sha256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelState {
    pub channel_id: [u8; 32],
    pub sequence_number: u64,
    pub balance_a: u64,
    pub balance_b: u64,
    pub total_deposit: u64,
    pub htlcs: Vec<HTLCOrder>,
}

impl ChannelState {
    pub fn new(
        pubkey_a: &[u8; 32],
        pubkey_b: &[u8; 32],
        deposit_a: u64,
        deposit_b: u64,
    ) -> Self {
        let mut id_data = Vec::with_capacity(64);
        id_data.extend_from_slice(pubkey_a);
        id_data.extend_from_slice(pubkey_b);
        let channel_id = sha256(&id_data);

        Self {
            channel_id,
            sequence_number: 0,
            balance_a: deposit_a,
            balance_b: deposit_b,
            total_deposit: deposit_a + deposit_b,
            htlcs: Vec::new(),
        }
    }

    /// Compute cryptographic state commitment hash for off-chain signatures.
    pub fn compute_state_hash(&self) -> [u8; 32] {
        let mut data = Vec::new();
        data.extend_from_slice(&self.channel_id);
        data.extend_from_slice(&self.sequence_number.to_be_bytes());
        data.extend_from_slice(&self.balance_a.to_be_bytes());
        data.extend_from_slice(&self.balance_b.to_be_bytes());

        // Include HTLC active hashes
        for htlc in &self.htlcs {
            if !htlc.claimed && !htlc.refunded {
                data.extend_from_slice(&htlc.htlc_id.to_be_bytes());
                data.extend_from_slice(&htlc.amount.to_be_bytes());
                data.extend_from_slice(&htlc.hash_lock);
                data.extend_from_slice(&htlc.expiry_slot.to_be_bytes());
            }
        }

        sha256(&data)
    }

    /// Execute off-chain payment transfer from party to counterparty.
    pub fn transfer(&mut self, sender: ChannelParty, amount: u64) -> Result<(), &'static str> {
        match sender {
            ChannelParty::PartyA => {
                if self.balance_a < amount {
                    return Err("Insufficient balance for Party A");
                }
                self.balance_a -= amount;
                self.balance_b += amount;
            }
            ChannelParty::PartyB => {
                if self.balance_b < amount {
                    return Err("Insufficient balance for Party B");
                }
                self.balance_b -= amount;
                self.balance_a += amount;
            }
        }

        self.sequence_number += 1;
        self.assert_balance_invariant();
        Ok(())
    }

    /// Lock funds into an HTLC conditional payment.
    pub fn open_htlc(
        &mut self,
        sender: ChannelParty,
        amount: u64,
        hash_lock: [u8; 32],
        expiry_slot: u64,
    ) -> Result<u64, &'static str> {
        match sender {
            ChannelParty::PartyA => {
                if self.balance_a < amount {
                    return Err("Insufficient balance for Party A to open HTLC");
                }
                self.balance_a -= amount;
            }
            ChannelParty::PartyB => {
                if self.balance_b < amount {
                    return Err("Insufficient balance for Party B to open HTLC");
                }
                self.balance_b -= amount;
            }
        }

        let htlc_id = self.htlcs.len() as u64 + 1;
        self.htlcs.push(HTLCOrder::new(
            htlc_id,
            sender,
            amount,
            hash_lock,
            expiry_slot,
        ));

        self.sequence_number += 1;
        self.assert_balance_invariant();
        Ok(htlc_id)
    }

    /// Claim HTLC with secret preimage, crediting the recipient.
    pub fn claim_htlc(&mut self, htlc_id: u64, preimage: &[u8]) -> Result<(), &'static str> {
        let htlc = self
            .htlcs
            .iter_mut()
            .find(|h| h.htlc_id == htlc_id)
            .ok_or("HTLC not found")?;

        let recipient = match htlc.sender {
            ChannelParty::PartyA => ChannelParty::PartyB,
            ChannelParty::PartyB => ChannelParty::PartyA,
        };

        let amount = htlc.claim(preimage)?;

        match recipient {
            ChannelParty::PartyA => self.balance_a += amount,
            ChannelParty::PartyB => self.balance_b += amount,
        }

        self.sequence_number += 1;
        self.assert_balance_invariant();
        Ok(())
    }

    /// Refund expired HTLC back to sender.
    pub fn refund_htlc(&mut self, htlc_id: u64, current_slot: u64) -> Result<(), &'static str> {
        let htlc = self
            .htlcs
            .iter_mut()
            .find(|h| h.htlc_id == htlc_id)
            .ok_or("HTLC not found")?;

        let sender = htlc.sender;
        let amount = htlc.refund(current_slot)?;

        match sender {
            ChannelParty::PartyA => self.balance_a += amount,
            ChannelParty::PartyB => self.balance_b += amount,
        }

        self.sequence_number += 1;
        self.assert_balance_invariant();
        Ok(())
    }

    /// Invariant: total balance + locked HTLC amounts must exactly equal total deposit.
    pub fn assert_balance_invariant(&self) {
        let active_htlc_sum: u64 = self
            .htlcs
            .iter()
            .filter(|h| !h.claimed && !h.refunded)
            .map(|h| h.amount)
            .sum();

        assert_eq!(
            self.balance_a + self.balance_b + active_htlc_sum,
            self.total_deposit,
            "State channel balance conservation invariant violated!"
        );
    }
}
