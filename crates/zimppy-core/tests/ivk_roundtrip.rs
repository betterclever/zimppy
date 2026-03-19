#![cfg(feature = "shielded")]

use orchard::keys::IncomingViewingKey;

#[test]
fn ivk_roundtrip() {
    let ivk_hex = "f803b8959eb4529227a0a3806db7302e8f7f77a0fa2898201f8c15862bafc71f84e798b8b3af27c6b6365f4b50d581396a7c8ca20ef188982cd4e28f47349c0a";
    let bytes = hex::decode(ivk_hex).unwrap();
    println!("IVK bytes length: {}", bytes.len());
    assert_eq!(bytes.len(), 64, "IVK should be 64 bytes");
    
    let arr: [u8; 64] = bytes.try_into().unwrap();
    let ivk = IncomingViewingKey::from_bytes(&arr);
    let is_valid = ivk.is_some().unwrap_u8() == 1;
    println!("IVK valid: {}", is_valid);
    assert!(is_valid, "IVK should be valid");
    
    // Check roundtrip
    let ivk = ivk.unwrap();
    let roundtrip = ivk.to_bytes();
    let roundtrip_hex = hex::encode(roundtrip);
    println!("Original:  {}", ivk_hex);
    println!("Roundtrip: {}", roundtrip_hex);
    assert_eq!(ivk_hex, roundtrip_hex, "IVK should roundtrip");
}
