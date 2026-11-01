use solana_vortex::*;

#[test]
fn test_sha256_standard_vectors() {
    // "abc" NIST SHA-256 test vector
    // ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad
    let hash = sha256(b"abc");
    let hex_str: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
    assert_eq!(
        hex_str,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn test_state_channel_transfers_and_invariants() {
    let pk_a = [1u8; 32];
    let pk_b = [2u8; 32];
    let deposit_a = 5_000_000_000; // 5 SOL
    let deposit_b = 3_000_000_000; // 3 SOL

    let mut state = ChannelState::new(&pk_a, &pk_b, deposit_a, deposit_b);
    assert_eq!(state.total_deposit, 8_000_000_000);
    assert_eq!(state.sequence_number, 0);

    // Transfer 1 SOL from Party A to Party B
    state.transfer(ChannelParty::PartyA, 1_000_000_000).unwrap();
    assert_eq!(state.balance_a, 4_000_000_000);
    assert_eq!(state.balance_b, 4_000_000_000);
    assert_eq!(state.sequence_number, 1);

    // Transfer 2 SOL from Party B to Party A
    state.transfer(ChannelParty::PartyB, 2_000_000_000).unwrap();
    assert_eq!(state.balance_a, 6_000_000_000);
    assert_eq!(state.balance_b, 2_000_000_000);
    assert_eq!(state.sequence_number, 2);

    // Overdraft should fail
    assert!(state.transfer(ChannelParty::PartyB, 5_000_000_000).is_err());
}

#[test]
fn test_htlc_conditional_payment_flow() {
    let pk_a = [1u8; 32];
    let pk_b = [2u8; 32];
    let mut state = ChannelState::new(&pk_a, &pk_b, 10_000_000_000, 10_000_000_000);

    let secret = b"super_secret_quantum_key_999";
    let hash_lock = sha256(secret);
    let expiry_slot = 1000;

    // Party A opens HTLC for 2 SOL locked to Party B
    let htlc_id = state
        .open_htlc(ChannelParty::PartyA, 2_000_000_000, hash_lock, expiry_slot)
        .unwrap();

    assert_eq!(state.balance_a, 8_000_000_000);
    assert_eq!(state.balance_b, 10_000_000_000);

    // Invalid secret preimage fails
    assert!(state.claim_htlc(htlc_id, b"wrong_secret").is_err());

    // Valid preimage unlocks funds to Party B
    state.claim_htlc(htlc_id, secret).unwrap();
    assert_eq!(state.balance_a, 8_000_000_000);
    assert_eq!(state.balance_b, 12_000_000_000);

    // Test refund flow
    let refund_secret = b"refund_me_please";
    let refund_lock = sha256(refund_secret);
    let refund_id = state
        .open_htlc(
            ChannelParty::PartyB,
            1_000_000_000,
            refund_lock,
            expiry_slot,
        )
        .unwrap();

    assert_eq!(state.balance_b, 11_000_000_000);

    // Cannot refund before expiry slot
    assert!(state.refund_htlc(refund_id, 999).is_err());

    // Refund succeeds at expiry slot
    state.refund_htlc(refund_id, 1000).unwrap();
    assert_eq!(state.balance_b, 12_000_000_000);
}

#[test]
fn test_fraud_proof_and_slashing() {
    let pk_a = [1u8; 32];
    let pk_b = [2u8; 32];
    let mut state = ChannelState::new(&pk_a, &pk_b, 5_000_000_000, 5_000_000_000);

    let mut manager = StateChannelManager::new(state.channel_id, state.total_deposit);

    // Party A performs honest state transitions
    state.transfer(ChannelParty::PartyA, 1_000_000_000).unwrap(); // Nonce 1
    let stale_state_nonce_1 = state.clone();

    state.transfer(ChannelParty::PartyA, 2_000_000_000).unwrap(); // Nonce 2
    state.transfer(ChannelParty::PartyA, 1_000_000_000).unwrap(); // Nonce 3
    let honest_latest_state = state.clone();

    // Dishonest Party A tries to close channel with old state (Nonce 1) to steal 3 SOL
    manager
        .initiate_unilateral_close(
            ChannelParty::PartyA,
            stale_state_nonce_1,
            500, // current slot
            100, // 100 slots challenge window
        )
        .unwrap();

    // Honest Party B submits fraud proof with Nonce 3
    manager
        .challenge_fraud_proof(honest_latest_state)
        .unwrap();

    // Cheater Party A should be slashed 100%, total deposit awarded to victim Party B
    match manager.status {
        ChannelStatus::Slashed {
            cheater,
            victim,
            awarded_deposit,
        } => {
            assert_eq!(cheater, ChannelParty::PartyA);
            assert_eq!(victim, ChannelParty::PartyB);
            assert_eq!(awarded_deposit, 10_000_000_000);
        }
        _ => panic!("Expected channel to be Slashed"),
    }
}

#[test]
fn test_limit_order_book_execution() {
    let mut ob = OrderBook::new();

    // Party A places Buy limit order: 10 units at 50 lamports each
    let (bid_id, trades) =
        ob.place_limit_order(OrderSide::Buy, 50, 10, ChannelParty::PartyA);
    assert_eq!(bid_id, 1);
    assert!(trades.is_empty());
    assert_eq!(ob.bids.len(), 1);

    // Party B places Sell limit order: 4 units at 48 lamports each (crosses spread)
    let (_, sell_trades) =
        ob.place_limit_order(OrderSide::Sell, 48, 4, ChannelParty::PartyB);
    assert_eq!(sell_trades.len(), 1);
    assert_eq!(sell_trades[0].quantity, 4);
    assert_eq!(sell_trades[0].price, 50); // Passive price matching
    assert_eq!(sell_trades[0].quote_amount, 200);

    // Remaining bid quantity should be 6 units
    assert_eq!(ob.bids[0].quantity, 6);
}
