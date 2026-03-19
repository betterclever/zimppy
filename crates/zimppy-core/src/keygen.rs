//! Generate a Zcash testnet wallet with Unified Address (Orchard).
//!
//! Usage: cargo run --bin zimppy-keygen --features keygen

#[cfg(feature = "keygen")]
fn main() {
    use zcash_keys::keys::{UnifiedSpendingKey, UnifiedAddressRequest, ReceiverRequirement};
    use zcash_protocol::consensus::Network;
    use zip32::{AccountId, Scope};

    // Generate random seed (32 bytes)
    let mut seed = [0u8; 32];
    rand::Rng::fill(&mut rand::thread_rng(), &mut seed);

    let account = AccountId::try_from(0).expect("valid account id");

    // Derive unified spending key from seed
    let usk = UnifiedSpendingKey::from_seed(&Network::TestNetwork, &seed, account)
        .expect("failed to derive USK from seed");

    // Get the unified full viewing key
    let ufvk = usk.to_unified_full_viewing_key();

    // Request a UA with Orchard receiver (the current recommended pool)
    let ua_request = UnifiedAddressRequest::unsafe_custom(
        ReceiverRequirement::Require, // orchard
        ReceiverRequirement::Omit,    // sapling
        ReceiverRequirement::Omit,    // p2pkh
    );

    let (ua, _diversifier_index) = ufvk
        .default_address(ua_request)
        .expect("failed to derive default address");

    let ua_string = ua.encode(&Network::TestNetwork);

    // Get the Orchard IVK for shielded verification
    let orchard_fvk = ufvk.orchard().expect("should have orchard key");
    let orchard_ivk = orchard_fvk.to_ivk(Scope::External);
    let ivk_bytes = orchard_ivk.to_bytes();

    println!("=== Zcash Testnet Wallet (Orchard) ===");
    println!();
    println!("Seed (hex): {}", hex::encode(seed));
    println!();
    println!("Unified Address: {ua_string}");
    println!();
    println!("Orchard IVK (hex): {}", hex::encode(ivk_bytes));
    println!();
    println!("IMPORTANT: Save the seed securely.");
    println!("Use this Unified Address with the faucet at https://testnet.zecfaucet.com/");
}

#[cfg(not(feature = "keygen"))]
fn main() {
    eprintln!("This binary requires the 'keygen' feature.");
    eprintln!("Run: cargo run --bin zimppy-keygen --features keygen");
    std::process::exit(1);
}
