use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Sha256, Digest};
use ethers::types::H256;
use anyhow::Result;

/// Compute SHA256 hash
pub fn sha256(data: &[u8]) -> H256 {
    let mut hasher = Sha256::new();
    hasher.update(data);
    H256::from_slice(&hasher.finalize())
}

/// Ed25519 signing
pub fn sign_receipt(
    signing_key: &SigningKey,
    job_id: &H256,
    output_hash: &H256,
) -> [u8; 64] {
    let mut msg = Vec::with_capacity(64);
    msg.extend_from_slice(job_id.as_bytes());
    msg.extend_from_slice(output_hash.as_bytes());

    let signature = signing_key.sign(&msg);
    signature.to_bytes()
}

/// Verify Ed25519 signature
pub fn verify_signature(
    msg: &[u8],
    signature: &[u8; 64],
    public_key: &VerifyingKey,
) -> Result<()> {
    let sig = Signature::from_bytes(signature);
    public_key.verify_strict(msg, &sig)
        .map_err(|e| anyhow::anyhow!("Signature verification failed: {}", e))
}