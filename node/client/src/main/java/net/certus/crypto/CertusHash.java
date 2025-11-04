package net.certus.crypto;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;

/**
 * Canonical hashing utilities for Certus protocol.
 * All hashes use SHA-256 with big-endian byte order.
 * Hex encoding is lowercase.
 */
public final class CertusHash {

    private CertusHash() {}

    /**
     * Compute SHA-256 hash of bytes.
     * @param data Input bytes
     * @return 32-byte SHA-256 hash
     */
    public static byte[] sha256(byte[] data) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            return digest.digest(data);
        } catch (NoSuchAlgorithmException e) {
            throw new RuntimeException("SHA-256 not available", e);
        }
    }

    /**
     * Compute SHA-256 hash of multiple byte arrays (concatenated).
     * @param parts Input byte arrays to concatenate and hash
     * @return 32-byte SHA-256 hash
     */
    public static byte[] sha256(byte[]... parts) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            for (byte[] part : parts) {
                digest.update(part);
            }
            return digest.digest();
        } catch (NoSuchAlgorithmException e) {
            throw new RuntimeException("SHA-256 not available", e);
        }
    }

    /**
     * Convert bytes to lowercase hex string.
     * @param bytes Input bytes
     * @return Hex string (lowercase, no 0x prefix)
     */
    public static String toHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) {
            sb.append(String.format("%02x", b & 0xff));
        }
        return sb.toString();
    }

    /**
     * Convert hex string to bytes.
     * @param hex Hex string (with or without 0x prefix)
     * @return Byte array
     */
    public static byte[] fromHex(String hex) {
        if (hex.startsWith("0x") || hex.startsWith("0X")) {
            hex = hex.substring(2);
        }

        if (hex.length() % 2 != 0) {
            throw new IllegalArgumentException("Hex string must have even length");
        }

        byte[] bytes = new byte[hex.length() / 2];
        for (int i = 0; i < bytes.length; i++) {
            int index = i * 2;
            bytes[i] = (byte) Integer.parseInt(hex.substring(index, index + 2), 16);
        }
        return bytes;
    }

    /**
     * Compute canonical JobId.
     * JobId = SHA256(wasmHash || inputHash || clientPubKey || nonce)
     *
     * @param wasmHash 32-byte SHA256 hash of Wasm module
     * @param inputHash 32-byte SHA256 hash of input data
     * @param clientPubKey Client's public key (32 bytes for Ed25519)
     * @param nonce 8-byte nonce (big-endian)
     * @return 32-byte JobId
     */
    public static byte[] computeJobId(byte[] wasmHash, byte[] inputHash, byte[] clientPubKey, long nonce) {
        if (wasmHash.length != 32) {
            throw new IllegalArgumentException("wasmHash must be 32 bytes");
        }
        if (inputHash.length != 32) {
            throw new IllegalArgumentException("inputHash must be 32 bytes");
        }
        if (clientPubKey.length != 32) {
            throw new IllegalArgumentException("clientPubKey must be 32 bytes");
        }

        // Encode nonce as big-endian 8 bytes
        ByteBuffer nonceBuffer = ByteBuffer.allocate(8);
        nonceBuffer.order(ByteOrder.BIG_ENDIAN);
        nonceBuffer.putLong(nonce);
        byte[] nonceBytes = nonceBuffer.array();

        return sha256(wasmHash, inputHash, clientPubKey, nonceBytes);
    }

    /**
     * Compute canonical signature message for ExecReceipt.
     * SignHash = SHA256(jobId || wasmHash || inputHash || outputHash || executor)
     *
     * @param jobId 32-byte job identifier
     * @param wasmHash 32-byte wasm hash
     * @param inputHash 32-byte input hash
     * @param outputHash 32-byte output hash
     * @param executorAddress Ethereum address (20 bytes)
     * @return 32-byte message digest to sign
     */
    public static byte[] computeReceiptSignHash(
        byte[] jobId,
        byte[] wasmHash,
        byte[] inputHash,
        byte[] outputHash,
        byte[] executorAddress
    ) {
        if (jobId.length != 32) throw new IllegalArgumentException("jobId must be 32 bytes");
        if (wasmHash.length != 32) throw new IllegalArgumentException("wasmHash must be 32 bytes");
        if (inputHash.length != 32) throw new IllegalArgumentException("inputHash must be 32 bytes");
        if (outputHash.length != 32) throw new IllegalArgumentException("outputHash must be 32 bytes");
        if (executorAddress.length != 20) throw new IllegalArgumentException("executorAddress must be 20 bytes");

        return sha256(jobId, wasmHash, inputHash, outputHash, executorAddress);
    }

    /**
     * Validate that a hash is exactly 32 bytes.
     * @param hash Hash to validate
     * @param name Name for error message
     * @throws IllegalArgumentException if not 32 bytes
     */
    public static void validateHash32(byte[] hash, String name) {
        if (hash == null || hash.length != 32) {
            throw new IllegalArgumentException(name + " must be 32 bytes");
        }
    }
}
