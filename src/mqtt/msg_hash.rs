use sha2::{Digest, Sha256};
use ulid::Ulid;

pub fn generate_dedup_ulid(timestamp_ms: u64, payload: &[u8]) -> Ulid {
    // 1. Hash the payload
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let result = hasher.finalize();

    // 2. We need 80 bits (10 bytes) for entropy
    // We create a 16-byte buffer for u128 conversion
    let mut entropy_bytes = [0u8; 16];
    // Copy 10 bytes from hash into the end of the buffer
    entropy_bytes[6..16].copy_from_slice(&result[0..10]);

    // 3. Convert bytes to u128
    let entropy_u128 = u128::from_be_bytes(entropy_bytes);

    // 4. Construct the ULID
    // from_parts(timestamp_ms: u64, entropy: u128)
    Ulid::from_parts(timestamp_ms, entropy_u128)
}
