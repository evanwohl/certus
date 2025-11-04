package net.certus.model;

import net.certus.crypto.CertusHash;
import net.certus.crypto.CertusKeys;

import java.math.BigInteger;

/**
 * Execution receipt signed by executor.
 * Canonical representation for signature verification.
 */
public class ExecReceipt {
    private final byte[] jobId;
    private final byte[] wasmHash;
    private final byte[] inputHash;
    private final byte[] outputHash;
    private final String executor;           // ETH address
    private final BigInteger executorDeposit;
    private final byte[] executorSig;        // 64-byte Ed25519 signature

    private ExecReceipt(Builder builder) {
        this.jobId = builder.jobId;
        this.wasmHash = builder.wasmHash;
        this.inputHash = builder.inputHash;
        this.outputHash = builder.outputHash;
        this.executor = builder.executor;
        this.executorDeposit = builder.executorDeposit;
        this.executorSig = builder.executorSig;

        validate();
    }

    private void validate() {
        CertusHash.validateHash32(jobId, "jobId");
        CertusHash.validateHash32(wasmHash, "wasmHash");
        CertusHash.validateHash32(inputHash, "inputHash");
        CertusHash.validateHash32(outputHash, "outputHash");

        if (executor == null || executor.isEmpty()) {
            throw new IllegalArgumentException("executor is required");
        }
        if (executorDeposit == null || executorDeposit.compareTo(BigInteger.ZERO) <= 0) {
            throw new IllegalArgumentException("executorDeposit must be > 0");
        }
        if (executorSig == null || executorSig.length != 64) {
            throw new IllegalArgumentException("executorSig must be 64 bytes");
        }
    }

    // Getters
    public byte[] getJobId() { return jobId.clone(); }
    public byte[] getWasmHash() { return wasmHash.clone(); }
    public byte[] getInputHash() { return inputHash.clone(); }
    public byte[] getOutputHash() { return outputHash.clone(); }
    public String getExecutor() { return executor; }
    public BigInteger getExecutorDeposit() { return executorDeposit; }
    public byte[] getExecutorSig() { return executorSig.clone(); }

    /**
     * Compute the canonical message hash for signing this receipt.
     * SignHash = SHA256(jobId || wasmHash || inputHash || outputHash || executorAddress)
     */
    public byte[] computeSignHash() {
        // Convert executor ETH address to 20 bytes
        byte[] executorBytes = addressToBytes(executor);
        return CertusHash.computeReceiptSignHash(jobId, wasmHash, inputHash, outputHash, executorBytes);
    }

    /**
     * Verify the executor's signature on this receipt.
     * @param executorPublicKey Executor's Ed25519 public key (32 bytes)
     * @return true if signature is valid
     */
    public boolean verifySignature(byte[] executorPublicKey) {
        byte[] signHash = computeSignHash();
        return CertusKeys.verify(signHash, executorSig, executorPublicKey);
    }

    /**
     * Sign this receipt with a key pair.
     * @param keyPair Executor's Ed25519 key pair
     * @return New ExecReceipt with signature filled
     */
    public static ExecReceipt sign(
        byte[] jobId,
        byte[] wasmHash,
        byte[] inputHash,
        byte[] outputHash,
        String executor,
        BigInteger executorDeposit,
        CertusKeys.KeyPair keyPair
    ) {
        // Compute sign hash
        byte[] executorBytes = addressToBytes(executor);
        byte[] signHash = CertusHash.computeReceiptSignHash(jobId, wasmHash, inputHash, outputHash, executorBytes);

        // Sign
        byte[] signature = CertusKeys.sign(signHash, keyPair);

        return new Builder()
            .jobId(jobId)
            .wasmHash(wasmHash)
            .inputHash(inputHash)
            .outputHash(outputHash)
            .executor(executor)
            .executorDeposit(executorDeposit)
            .executorSig(signature)
            .build();
    }

    /**
     * Convert Ethereum hex address to 20 bytes.
     */
    private static byte[] addressToBytes(String address) {
        if (address.startsWith("0x") || address.startsWith("0X")) {
            address = address.substring(2);
        }
        if (address.length() != 40) {
            throw new IllegalArgumentException("Ethereum address must be 40 hex chars (20 bytes)");
        }
        return CertusHash.fromHex(address);
    }

    /**
     * Convert to protobuf message.
     */
    public net.certus.proto.ExecReceipt toProto() {
        return net.certus.proto.ExecReceipt.newBuilder()
            .setJobId(com.google.protobuf.ByteString.copyFrom(jobId))
            .setWasmHash(com.google.protobuf.ByteString.copyFrom(wasmHash))
            .setInputHash(com.google.protobuf.ByteString.copyFrom(inputHash))
            .setOutputHash(com.google.protobuf.ByteString.copyFrom(outputHash))
            .setExecutor(executor)
            .setExecutorDeposit(executorDeposit.toString())
            .setExecutorSig(com.google.protobuf.ByteString.copyFrom(executorSig))
            .build();
    }

    /**
     * Create from protobuf message.
     */
    public static ExecReceipt fromProto(net.certus.proto.ExecReceipt proto) {
        return new Builder()
            .jobId(proto.getJobId().toByteArray())
            .wasmHash(proto.getWasmHash().toByteArray())
            .inputHash(proto.getInputHash().toByteArray())
            .outputHash(proto.getOutputHash().toByteArray())
            .executor(proto.getExecutor())
            .executorDeposit(new BigInteger(proto.getExecutorDeposit()))
            .executorSig(proto.getExecutorSig().toByteArray())
            .build();
    }

    public static class Builder {
        private byte[] jobId;
        private byte[] wasmHash;
        private byte[] inputHash;
        private byte[] outputHash;
        private String executor;
        private BigInteger executorDeposit;
        private byte[] executorSig;

        public Builder jobId(byte[] jobId) { this.jobId = jobId; return this; }
        public Builder wasmHash(byte[] wasmHash) { this.wasmHash = wasmHash; return this; }
        public Builder inputHash(byte[] inputHash) { this.inputHash = inputHash; return this; }
        public Builder outputHash(byte[] outputHash) { this.outputHash = outputHash; return this; }
        public Builder executor(String executor) { this.executor = executor; return this; }
        public Builder executorDeposit(BigInteger executorDeposit) { this.executorDeposit = executorDeposit; return this; }
        public Builder executorSig(byte[] executorSig) { this.executorSig = executorSig; return this; }

        public ExecReceipt build() {
            return new ExecReceipt(this);
        }
    }

    @Override
    public String toString() {
        return String.format("ExecReceipt{jobId=%s, outputHash=%s, executor=%s}",
            CertusHash.toHex(jobId).substring(0, 8),
            CertusHash.toHex(outputHash).substring(0, 8),
            executor
        );
    }
}
