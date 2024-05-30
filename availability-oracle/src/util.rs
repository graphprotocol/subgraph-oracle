use tiny_cid::Cid;

pub fn bytes32_to_cid_v0(bytes32: [u8; 32]) -> Cid {
    let mut cidv0: [u8; 34] = [0; 34];

    // The start of any CIDv0.
    cidv0[0] = 0x12;
    cidv0[1] = 0x20;

    cidv0[2..].copy_from_slice(&bytes32);

    // Unwrap: We've constructed a valid CIDv0.
    Cid::read_bytes(cidv0.as_ref()).unwrap()
}

// Panics if `cid` version is not v0.
#[allow(dead_code)]
pub fn cid_v0_to_bytes32(cid: &Cid) -> [u8; 32] {
    assert!(cid.version() == tiny_cid::Version::V0);
    let cid_bytes = cid.to_bytes();

    // A CIDv0 in byte form is 34 bytes long, starting with 0x1220.
    assert_eq!(cid_bytes.len(), 34);

    let mut bytes: [u8; 32] = [0; 32];
    bytes.copy_from_slice(&cid_bytes[2..]);
    bytes
}
