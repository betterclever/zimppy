//! Derive Orchard IVK from a BIP39 mnemonic phrase.
//!
//! Usage: cargo run --bin zimppy-derive-ivk --features keygen -- "word1 word2 ... word24"

#[cfg(feature = "keygen")]
fn main() {
    use bip0039::{English, Mnemonic};
    use zcash_keys::keys::{ReceiverRequirement, UnifiedAddressRequest, UnifiedSpendingKey};
    use zcash_protocol::consensus::Network;
    use zip32::{AccountId, Scope};

    let mnemonic_str = std::env::args()
        .nth(1)
        .expect("usage: zimppy-derive-ivk \"word1 word2 ... word24\"");

    let mnemonic =
        Mnemonic::<English>::from_phrase(&mnemonic_str).expect("invalid mnemonic phrase");

    let seed = mnemonic.to_seed("");
    let account = AccountId::try_from(0).expect("valid account id");

    let usk = UnifiedSpendingKey::from_seed(&Network::TestNetwork, &seed, account)
        .expect("failed to derive USK from seed");
    let ufvk = usk.to_unified_full_viewing_key();

    let orchard_fvk = ufvk.orchard().expect("should have orchard key");
    let orchard_ivk = orchard_fvk.to_ivk(Scope::External);
    let ivk_bytes = orchard_ivk.to_bytes();

    let ua_request = UnifiedAddressRequest::unsafe_custom(
        ReceiverRequirement::Require,
        ReceiverRequirement::Omit,
        ReceiverRequirement::Omit,
    );
    let (ua, _) = ufvk
        .default_address(ua_request)
        .expect("failed to derive address");

    println!("Address: {}", ua.encode(&Network::TestNetwork));
    println!("Orchard IVK: {}", hex::encode(ivk_bytes));
}

#[cfg(not(feature = "keygen"))]
fn main() {
    eprintln!("Requires --features keygen");
    std::process::exit(1);
}
