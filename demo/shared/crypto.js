import * as ed25519 from '@noble/ed25519';
import crypto from 'crypto';

/**
 * Generate Ed25519 keypair for signing
 */
export async function generateKeypair() {
  const privateKey = ed25519.utils.randomPrivateKey();
  const publicKey = await ed25519.getPublicKeyAsync(privateKey);
  return { privateKey, publicKey };
}

/**
 * Sign message with Ed25519 private key
 */
export async function sign(message, privateKey) {
  const msgHash = sha256(message);
  const signature = await ed25519.signAsync(msgHash, privateKey);
  return Buffer.from(signature).toString('hex');
}

/**
 * Verify Ed25519 signature
 */
export async function verify(message, signature, publicKey) {
  const msgHash = sha256(message);
  const sigBytes = Buffer.from(signature, 'hex');
  return ed25519.verifyAsync(sigBytes, msgHash, publicKey);
}

/**
 * SHA-256 hash (returns hex string)
 */
export function sha256(data) {
  const buffer = typeof data === 'string' ? Buffer.from(data) : data;
  return crypto.createHash('sha256').update(buffer).digest();
}

/**
 * SHA-256 hash (returns Buffer)
 */
export function sha256Hex(data) {
  const buffer = typeof data === 'string' ? Buffer.from(data) : data;
  return crypto.createHash('sha256').update(buffer).digest('hex');
}

/**
 * Generate deterministic job ID from inputs
 */
export function generateJobId(wasmHash, inputHash, nonce) {
  const data = `${wasmHash}${inputHash}${nonce}`;
  return sha256Hex(data);
}
