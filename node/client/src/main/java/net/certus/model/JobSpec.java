package net.certus.model;

import net.certus.crypto.CertusHash;

import java.math.BigInteger;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

/**
 * Canonical job specification for Certus compute tasks.
 * Immutable data structure with deterministic serialization.
 */
public class JobSpec {
    private final byte[] jobId;           // 32 bytes
    private final byte[] wasmHash;        // 32 bytes
    private final byte[] inputHash;       // 32 bytes
    private final String payToken;        // ERC20 address
    private final BigInteger payAmt;      // Token amount in smallest units
    private final String client;          // Client ETH address
    private final BigInteger clientDeposit;
    private final long acceptDeadline;    // Unix timestamp (seconds)
    private final long finalizeDeadline;  // Unix timestamp (seconds)
    private final long fuelLimit;         // Wasm instruction budget
    private final long memLimit;          // Memory limit (bytes)
    private final int maxOutputSize;      // Max output size (bytes)
    private final int collateralRatioBps; // Collateral ratio in basis points (10000 = 1.0x)

    private JobSpec(Builder builder) {
        this.jobId = builder.jobId;
        this.wasmHash = builder.wasmHash;
        this.inputHash = builder.inputHash;
        this.payToken = builder.payToken;
        this.payAmt = builder.payAmt;
        this.client = builder.client;
        this.clientDeposit = builder.clientDeposit;
        this.acceptDeadline = builder.acceptDeadline;
        this.finalizeDeadline = builder.finalizeDeadline;
        this.fuelLimit = builder.fuelLimit;
        this.memLimit = builder.memLimit;
        this.maxOutputSize = builder.maxOutputSize;
        this.collateralRatioBps = builder.collateralRatioBps;

        validate();
    }

    private void validate() {
        CertusHash.validateHash32(jobId, "jobId");
        CertusHash.validateHash32(wasmHash, "wasmHash");
        CertusHash.validateHash32(inputHash, "inputHash");

        if (payToken == null || payToken.isEmpty()) {
            throw new IllegalArgumentException("payToken is required");
        }
        if (payAmt == null || payAmt.compareTo(BigInteger.ZERO) <= 0) {
            throw new IllegalArgumentException("payAmt must be > 0");
        }
        if (client == null || client.isEmpty()) {
            throw new IllegalArgumentException("client is required");
        }
        if (acceptDeadline <= 0 || finalizeDeadline <= 0) {
            throw new IllegalArgumentException("Deadlines must be positive");
        }
        if (finalizeDeadline <= acceptDeadline) {
            throw new IllegalArgumentException("finalizeDeadline must be after acceptDeadline");
        }
        if (fuelLimit <= 0 || memLimit <= 0 || maxOutputSize <= 0) {
            throw new IllegalArgumentException("Resource limits must be positive");
        }
    }

    // Getters
    public byte[] getJobId() { return jobId.clone(); }
    public byte[] getWasmHash() { return wasmHash.clone(); }
    public byte[] getInputHash() { return inputHash.clone(); }
    public String getPayToken() { return payToken; }
    public BigInteger getPayAmt() { return payAmt; }
    public String getClient() { return client; }
    public BigInteger getClientDeposit() { return clientDeposit; }
    public long getAcceptDeadline() { return acceptDeadline; }
    public long getFinalizeDeadline() { return finalizeDeadline; }
    public long getFuelLimit() { return fuelLimit; }
    public long getMemLimit() { return memLimit; }
    public int getMaxOutputSize() { return maxOutputSize; }
    public int getCollateralRatioBps() { return collateralRatioBps; }

    /**
     * Compute canonical JobId from components.
     * JobId = SHA256(wasmHash || inputHash || clientPubKey || nonce)
     */
    public static byte[] computeJobId(byte[] wasmHash, byte[] inputHash, byte[] clientPubKey, long nonce) {
        return CertusHash.computeJobId(wasmHash, inputHash, clientPubKey, nonce);
    }

    /**
     * Convert to protobuf message.
     */
    public net.certus.proto.JobSpec toProto() {
        return net.certus.proto.JobSpec.newBuilder()
            .setJobId(com.google.protobuf.ByteString.copyFrom(jobId))
            .setWasmHash(com.google.protobuf.ByteString.copyFrom(wasmHash))
            .setInputHash(com.google.protobuf.ByteString.copyFrom(inputHash))
            .setPayToken(payToken)
            .setPayAmt(payAmt.toString())
            .setClient(client)
            .setClientDeposit(clientDeposit.toString())
            .setAcceptDeadline(acceptDeadline)
            .setFinalizeDeadline(finalizeDeadline)
            .setFuelLimit(fuelLimit)
            .setMemLimit(memLimit)
            .setMaxOutputSize(maxOutputSize)
            .setCollateralRatioBps(collateralRatioBps)
            .build();
    }

    /**
     * Create from protobuf message.
     */
    public static JobSpec fromProto(net.certus.proto.JobSpec proto) {
        return new Builder()
            .jobId(proto.getJobId().toByteArray())
            .wasmHash(proto.getWasmHash().toByteArray())
            .inputHash(proto.getInputHash().toByteArray())
            .payToken(proto.getPayToken())
            .payAmt(new BigInteger(proto.getPayAmt()))
            .client(proto.getClient())
            .clientDeposit(new BigInteger(proto.getClientDeposit()))
            .acceptDeadline(proto.getAcceptDeadline())
            .finalizeDeadline(proto.getFinalizeDeadline())
            .fuelLimit(proto.getFuelLimit())
            .memLimit(proto.getMemLimit())
            .maxOutputSize(proto.getMaxOutputSize())
            .collateralRatioBps(proto.getCollateralRatioBps())
            .build();
    }

    public static class Builder {
        private byte[] jobId;
        private byte[] wasmHash;
        private byte[] inputHash;
        private String payToken;
        private BigInteger payAmt;
        private String client;
        private BigInteger clientDeposit;
        private long acceptDeadline;
        private long finalizeDeadline;
        private long fuelLimit;
        private long memLimit;
        private int maxOutputSize;
        private int collateralRatioBps = 15000; // Default 1.5x

        public Builder jobId(byte[] jobId) { this.jobId = jobId; return this; }
        public Builder wasmHash(byte[] wasmHash) { this.wasmHash = wasmHash; return this; }
        public Builder inputHash(byte[] inputHash) { this.inputHash = inputHash; return this; }
        public Builder payToken(String payToken) { this.payToken = payToken; return this; }
        public Builder payAmt(BigInteger payAmt) { this.payAmt = payAmt; return this; }
        public Builder client(String client) { this.client = client; return this; }
        public Builder clientDeposit(BigInteger clientDeposit) { this.clientDeposit = clientDeposit; return this; }
        public Builder acceptDeadline(long acceptDeadline) { this.acceptDeadline = acceptDeadline; return this; }
        public Builder finalizeDeadline(long finalizeDeadline) { this.finalizeDeadline = finalizeDeadline; return this; }
        public Builder fuelLimit(long fuelLimit) { this.fuelLimit = fuelLimit; return this; }
        public Builder memLimit(long memLimit) { this.memLimit = memLimit; return this; }
        public Builder maxOutputSize(int maxOutputSize) { this.maxOutputSize = maxOutputSize; return this; }
        public Builder collateralRatioBps(int collateralRatioBps) { this.collateralRatioBps = collateralRatioBps; return this; }

        public JobSpec build() {
            return new JobSpec(this);
        }
    }

    @Override
    public String toString() {
        return String.format("JobSpec{jobId=%s, wasmHash=%s, inputHash=%s, payAmt=%s, client=%s}",
            CertusHash.toHex(jobId).substring(0, 8),
            CertusHash.toHex(wasmHash).substring(0, 8),
            CertusHash.toHex(inputHash).substring(0, 8),
            payAmt.toString(),
            client
        );
    }
}
