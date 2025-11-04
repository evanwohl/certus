package net.certus.crypto;

import org.bouncycastle.crypto.params.Ed25519PrivateKeyParameters;
import org.bouncycastle.crypto.params.Ed25519PublicKeyParameters;
import org.bouncycastle.crypto.signers.Ed25519Signer;

import java.security.SecureRandom;

/**
 * Ed25519 key management and signature operations for Certus.
 * All signatures are raw 64-byte Ed25519 signatures.
 */
public final class CertusKeys {

    private CertusKeys() {}

    /**
     * Key pair for Ed25519 signing.
     */
    public static class KeyPair {
        private final Ed25519PrivateKeyParameters privateKey;
        private final Ed25519PublicKeyParameters publicKey;

        public KeyPair(Ed25519PrivateKeyParameters privateKey, Ed25519PublicKeyParameters publicKey) {
            this.privateKey = privateKey;
            this.publicKey = publicKey;
        }

        public Ed25519PrivateKeyParameters getPrivateKey() {
            return privateKey;
        }

        public Ed25519PublicKeyParameters getPublicKey() {
            return publicKey;
        }

        /**
         * Get raw 32-byte public key.
         */
        public byte[] getPublicKeyBytes() {
            return publicKey.getEncoded();
        }

        /**
         * Get raw 32-byte private key seed.
         */
        public byte[] getPrivateKeyBytes() {
            return privateKey.getEncoded();
        }
    }

    /**
     * Generate a new Ed25519 key pair.
     * @return New KeyPair
     */
    public static KeyPair generateKeyPair() {
        SecureRandom random = new SecureRandom();
        byte[] seed = new byte[32];
        random.nextBytes(seed);
        return fromPrivateKey(seed);
    }

    /**
     * Reconstruct key pair from 32-byte private key seed.
     * @param privateKeySeed 32-byte Ed25519 private key seed
     * @return KeyPair
     */
    public static KeyPair fromPrivateKey(byte[] privateKeySeed) {
        if (privateKeySeed.length != 32) {
            throw new IllegalArgumentException("Private key seed must be 32 bytes");
        }

        Ed25519PrivateKeyParameters privateKey = new Ed25519PrivateKeyParameters(privateKeySeed, 0);
        Ed25519PublicKeyParameters publicKey = privateKey.generatePublicKey();

        return new KeyPair(privateKey, publicKey);
    }

    /**
     * Reconstruct public key from 32-byte encoded public key.
     * @param publicKeyBytes 32-byte Ed25519 public key
     * @return Ed25519PublicKeyParameters
     */
    public static Ed25519PublicKeyParameters fromPublicKey(byte[] publicKeyBytes) {
        if (publicKeyBytes.length != 32) {
            throw new IllegalArgumentException("Public key must be 32 bytes");
        }
        return new Ed25519PublicKeyParameters(publicKeyBytes, 0);
    }

    /**
     * Sign a message with Ed25519.
     * @param message Message to sign (typically a hash)
     * @param keyPair Signing key pair
     * @return 64-byte raw Ed25519 signature
     */
    public static byte[] sign(byte[] message, KeyPair keyPair) {
        Ed25519Signer signer = new Ed25519Signer();
        signer.init(true, keyPair.getPrivateKey());
        signer.update(message, 0, message.length);
        return signer.generateSignature();
    }

    /**
     * Verify an Ed25519 signature.
     * @param message Original message
     * @param signature 64-byte signature
     * @param publicKey Public key
     * @return true if signature is valid
     */
    public static boolean verify(byte[] message, byte[] signature, Ed25519PublicKeyParameters publicKey) {
        if (signature.length != 64) {
            return false;
        }

        Ed25519Signer verifier = new Ed25519Signer();
        verifier.init(false, publicKey);
        verifier.update(message, 0, message.length);
        return verifier.verifySignature(signature);
    }

    /**
     * Verify an Ed25519 signature with raw public key bytes.
     * @param message Original message
     * @param signature 64-byte signature
     * @param publicKeyBytes 32-byte public key
     * @return true if signature is valid
     */
    public static boolean verify(byte[] message, byte[] signature, byte[] publicKeyBytes) {
        try {
            Ed25519PublicKeyParameters publicKey = fromPublicKey(publicKeyBytes);
            return verify(message, signature, publicKey);
        } catch (Exception e) {
            return false;
        }
    }
}
