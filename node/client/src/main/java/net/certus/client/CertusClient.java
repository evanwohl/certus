package net.certus.client;

import net.certus.crypto.CertusHash;
import net.certus.crypto.CertusKeys;
import net.certus.model.JobSpec;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.IOException;
import java.math.BigInteger;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * High-level client SDK for Certus compute jobs.
 * Provides convenient methods for job lifecycle management.
 */
public class CertusClient {
    private static final Logger logger = LoggerFactory.getLogger(CertusClient.class);

    private final EscrowClient escrowClient;
    private final CertusKeys.KeyPair clientKeys;
    private final String clientAddress;

    // Default parameters (from protocol specification)
    private static final BigInteger DEFAULT_CLIENT_DEPOSIT = new BigInteger("5000000"); // 5 USDC
    private static final long DEFAULT_ACCEPT_WINDOW = 120; // 2 minutes
    private static final long DEFAULT_FINALIZE_WINDOW = 300; // 5 minutes
    private static final long DEFAULT_FUEL_LIMIT = 10_000_000;
    private static final long DEFAULT_MEM_LIMIT = 64 * 1024 * 1024; // 64MB
    private static final int DEFAULT_MAX_OUTPUT_SIZE = 1024 * 1024; // 1MB

    public CertusClient(String rpcUrl, String privateKey, String contractAddress, byte[] clientKeySeed) {
        this.escrowClient = new EscrowClient(rpcUrl, privateKey, contractAddress);
        this.clientKeys = CertusKeys.fromPrivateKey(clientKeySeed);
        this.clientAddress = deriveEthAddress(privateKey);

        logger.info("Initialized CertusClient: address={}", clientAddress);
    }

    /**
     * Create and submit a new compute job.
     *
     * @param wasmPath Path to Wasm module file
     * @param inputPath Path to input data file
     * @param payToken ERC20 token address (USDC/USDT/DAI)
     * @param payAmt Payment amount in token smallest units
     * @return JobSpec
     */
    public JobSpec createJob(Path wasmPath, Path inputPath, String payToken, BigInteger payAmt) throws Exception {
        logger.info("Creating job: wasm={}, input={}, payAmt={}", wasmPath, inputPath, payAmt);

        // 1. Read wasm and input files
        byte[] wasmBytes = Files.readAllBytes(wasmPath);
        byte[] inputBytes = Files.readAllBytes(inputPath);

        // 2. Register wasm on-chain
        byte[] wasmHash = escrowClient.registerWasm(wasmBytes);
        logger.info("Wasm registered: hash={}", CertusHash.toHex(wasmHash));

        // 3. Compute hashes and jobId
        byte[] inputHash = CertusHash.sha256(inputBytes);
        long nonce = System.currentTimeMillis();
        byte[] jobId = CertusHash.computeJobId(wasmHash, inputHash, clientKeys.getPublicKeyBytes(), nonce);

        // 4. Create job spec
        long now = System.currentTimeMillis() / 1000;
        JobSpec jobSpec = new JobSpec.Builder()
            .jobId(jobId)
            .wasmHash(wasmHash)
            .inputHash(inputHash)
            .payToken(payToken)
            .payAmt(payAmt)
            .client(clientAddress)
            .clientDeposit(DEFAULT_CLIENT_DEPOSIT)
            .acceptDeadline(now + DEFAULT_ACCEPT_WINDOW)
            .finalizeDeadline(now + DEFAULT_ACCEPT_WINDOW + DEFAULT_FINALIZE_WINDOW)
            .fuelLimit(DEFAULT_FUEL_LIMIT)
            .memLimit(DEFAULT_MEM_LIMIT)
            .maxOutputSize(DEFAULT_MAX_OUTPUT_SIZE)
            .build();

        // 5. Submit job on-chain
        escrowClient.createJob(jobSpec, payToken);

        logger.info("Job created: jobId={}", CertusHash.toHex(jobId));

        return jobSpec;
    }

    /**
     * Finalize a job after verifying the receipt.
     *
     * @param jobId Job identifier
     */
    public void finalizeJob(byte[] jobId) throws Exception {
        logger.info("Finalizing job: {}", CertusHash.toHex(jobId));
        escrowClient.finalizeJob(jobId);
        logger.info("Job finalized: {}", CertusHash.toHex(jobId));
    }

    /**
     * Submit fraud proof if executor's receipt is incorrect.
     *
     * @param jobId Job identifier
     * @param wasmBytes Full Wasm module
     * @param inputBytes Full input data
     * @param claimedOutput Executor's claimed output
     */
    public void submitFraud(byte[] jobId, byte[] wasmBytes, byte[] inputBytes, byte[] claimedOutput) throws Exception {
        logger.info("Submitting fraud proof: {}", CertusHash.toHex(jobId));
        escrowClient.submitFraud(jobId, wasmBytes, inputBytes, claimedOutput);
        logger.info("Fraud proof submitted: {}", CertusHash.toHex(jobId));
    }

    /**
     * Create job with custom parameters.
     */
    public static class JobBuilder {
        private Path wasmPath;
        private Path inputPath;
        private String payToken;
        private BigInteger payAmt;
        private BigInteger clientDeposit = DEFAULT_CLIENT_DEPOSIT;
        private long acceptWindow = DEFAULT_ACCEPT_WINDOW;
        private long finalizeWindow = DEFAULT_FINALIZE_WINDOW;
        private long fuelLimit = DEFAULT_FUEL_LIMIT;
        private long memLimit = DEFAULT_MEM_LIMIT;
        private int maxOutputSize = DEFAULT_MAX_OUTPUT_SIZE;

        public JobBuilder wasmPath(Path wasmPath) { this.wasmPath = wasmPath; return this; }
        public JobBuilder inputPath(Path inputPath) { this.inputPath = inputPath; return this; }
        public JobBuilder payToken(String payToken) { this.payToken = payToken; return this; }
        public JobBuilder payAmt(BigInteger payAmt) { this.payAmt = payAmt; return this; }
        public JobBuilder clientDeposit(BigInteger clientDeposit) { this.clientDeposit = clientDeposit; return this; }
        public JobBuilder acceptWindow(long acceptWindow) { this.acceptWindow = acceptWindow; return this; }
        public JobBuilder finalizeWindow(long finalizeWindow) { this.finalizeWindow = finalizeWindow; return this; }
        public JobBuilder fuelLimit(long fuelLimit) { this.fuelLimit = fuelLimit; return this; }
        public JobBuilder memLimit(long memLimit) { this.memLimit = memLimit; return this; }
        public JobBuilder maxOutputSize(int maxOutputSize) { this.maxOutputSize = maxOutputSize; return this; }

        public JobSpec build(CertusClient client) throws Exception {
            if (wasmPath == null || inputPath == null || payToken == null || payAmt == null) {
                throw new IllegalArgumentException("Required fields not set");
            }

            // Read files
            byte[] wasmBytes = Files.readAllBytes(wasmPath);
            byte[] inputBytes = Files.readAllBytes(inputPath);

            // Register wasm
            byte[] wasmHash = client.escrowClient.registerWasm(wasmBytes);

            // Compute jobId
            byte[] inputHash = CertusHash.sha256(inputBytes);
            long nonce = System.currentTimeMillis();
            byte[] jobId = CertusHash.computeJobId(wasmHash, inputHash, client.clientKeys.getPublicKeyBytes(), nonce);

            // Create spec
            long now = System.currentTimeMillis() / 1000;
            JobSpec jobSpec = new JobSpec.Builder()
                .jobId(jobId)
                .wasmHash(wasmHash)
                .inputHash(inputHash)
                .payToken(payToken)
                .payAmt(payAmt)
                .client(client.clientAddress)
                .clientDeposit(clientDeposit)
                .acceptDeadline(now + acceptWindow)
                .finalizeDeadline(now + acceptWindow + finalizeWindow)
                .fuelLimit(fuelLimit)
                .memLimit(memLimit)
                .maxOutputSize(maxOutputSize)
                .build();

            // Submit on-chain
            client.escrowClient.createJob(jobSpec, payToken);

            return jobSpec;
        }
    }

    private String deriveEthAddress(String privateKey) {
        org.web3j.crypto.Credentials creds = org.web3j.crypto.Credentials.create(privateKey);
        return creds.getAddress();
    }

    public void close() {
        escrowClient.close();
    }
}
