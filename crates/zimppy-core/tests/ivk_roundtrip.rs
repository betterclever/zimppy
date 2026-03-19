#![cfg(feature = "shielded")]

use orchard::keys::IncomingViewingKey;

#[test]
fn ivk_roundtrip() {
    let ivk_hex = "f803b8959eb4529227a0a3806db7302e8f7f77a0fa2898201f8c15862bafc71f84e798b8b3af27c6b6365f4b50d581396a7c8ca20ef188982cd4e28f47349c0a";
    let bytes = hex::decode(ivk_hex).expect("valid hex");
    assert_eq!(bytes.len(), 64, "IVK should be 64 bytes");

    let arr: [u8; 64] = bytes.try_into().expect("64 bytes");
    let ivk = IncomingViewingKey::from_bytes(&arr);
    let is_valid = ivk.is_some().unwrap_u8() == 1;
    assert!(is_valid, "IVK should be valid");

    let ivk: Option<IncomingViewingKey> = ivk.into();
    let ivk = ivk.expect("valid IVK");
    let roundtrip_hex = hex::encode(ivk.to_bytes());
    assert_eq!(ivk_hex, roundtrip_hex, "IVK should roundtrip");
}
