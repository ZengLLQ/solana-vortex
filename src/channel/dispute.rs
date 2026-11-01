//! Dispute resolution, fraud proof verification, and slashing mechanics.

use crate::channel::htlc::ChannelParty;
use crate::channel::state::ChannelState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelStatus {
    Open,
    DisputePending {
        submitted_state: ChannelState,
        submitted_by: ChannelParty,
        challenge_deadline_slot: u64,
    },
    Settled {
        payout_a: u64,
        payout_b: u64,
    },
    Slashed {
        cheater: ChannelParty,
        victim: ChannelParty,
        awarded_deposit: u64,
    },
}

pub struct StateChannelManager {
    pub channel_id: [u8; 32],
    pub total_deposit: u64,
    pub status: ChannelStatus,
}

impl StateChannelManager {
    pub fn new(channel_id: [u8; 32], total_deposit: u64) -> Self {
        Self {
            channel_id,
            total_deposit,
            status: ChannelStatus::Open,
        }
    }

    /// Cooperative mutual close: settles immediately without challenge delay.
    pub fn cooperative_close(&mut self, state: &ChannelState) -> Result<(), &'static str> {
        if self.status != ChannelStatus::Open {
            return Err("Channel is not in Open status");
        }
        if state.channel_id != self.channel_id {
            return Err("Mismatched channel ID");
        }
        state.assert_balance_invariant();

        self.status = ChannelStatus::Settled {
            payout_a: state.balance_a,
            payout_b: state.balance_b,
        };
        Ok(())
    }

    /// Initiate unilateral channel close: starts the dispute challenge window.
    pub fn initiate_unilateral_close(
        &mut self,
        submitted_by: ChannelParty,
        state: ChannelState,
        current_slot: u64,
        challenge_duration_slots: u64,
    ) -> Result<(), &'static str> {
        if self.status != ChannelStatus::Open {
            return Err("Channel is already in dispute or settled");
        }
        if state.channel_id != self.channel_id {
            return Err("Mismatched channel ID");
        }
        state.assert_balance_invariant();

        self.status = ChannelStatus::DisputePending {
            submitted_state: state,
            submitted_by,
            challenge_deadline_slot: current_slot + challenge_duration_slots,
        };
        Ok(())
    }

    /// Challenge stale state using a newer signed state (Fraud Proof).
    /// If newer sequence number is proven, the dishonest party is slashed 100%.
    pub fn challenge_fraud_proof(&mut self, newer_state: ChannelState) -> Result<(), &'static str> {
        match &self.status {
            ChannelStatus::DisputePending {
                submitted_state,
                submitted_by,
                ..
            } => {
                if newer_state.channel_id != self.channel_id {
                    return Err("Mismatched channel ID on challenge");
                }
                newer_state.assert_balance_invariant();

                if newer_state.sequence_number <= submitted_state.sequence_number {
                    return Err("Challenging state does not have a strictly higher sequence number");
                }

                let cheater = *submitted_by;
                let victim = match cheater {
                    ChannelParty::PartyA => ChannelParty::PartyB,
                    ChannelParty::PartyB => ChannelParty::PartyA,
                };

                // Immediate slashing: cheater loses entire channel deposit
                self.status = ChannelStatus::Slashed {
                    cheater,
                    victim,
                    awarded_deposit: self.total_deposit,
                };
                Ok(())
            }
            _ => Err("Cannot challenge: no dispute is currently pending"),
        }
    }

    /// Finalize settlement if challenge window expired without challenge.
    pub fn finalize_settlement(&mut self, current_slot: u64) -> Result<(), &'static str> {
        match &self.status {
            ChannelStatus::DisputePending {
                submitted_state,
                challenge_deadline_slot,
                ..
            } => {
                if current_slot < *challenge_deadline_slot {
                    return Err("Challenge window has not yet elapsed");
                }

                self.status = ChannelStatus::Settled {
                    payout_a: submitted_state.balance_a,
                    payout_b: submitted_state.balance_b,
                };
                Ok(())
            }
            _ => Err("Channel is not in DisputePending status"),
        }
    }
}
