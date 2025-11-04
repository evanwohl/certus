package net.certus.verifier;

import net.certus.client.EscrowClient;
import net.certus.crypto.CertusHash;
import net.certus.model.ExecReceipt;
import net.certus.model.JobSpec;
import net.certus.wasm.WasmSandbox;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.web3j.protocol.core.methods.response.TransactionReceipt;

import java.util.Arrays;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * Verifier node that re-executes jobs and detects fraud.
 *
 * Flow:
 * 1. Monitor for submitted receipts (Receipt status in contract)
 * 2. Fetch job spec, wasm, and input
 * 3. Re-execute in deterministic sandbox
 * 4. Compare outputHash with executor's claimed outputHash
 * 5. If mismatch: submit fraudOnChain with proof
 * 6. Collect verifier bounty (20% of slashed collateral)
 */
public class VerifierNode {
    private static final Logger logger = LoggerFactory.getLogger(VerifierNode.class);

    private final EscrowClient escrowClient;
    private final WasmSandbox wasmSandbox;
    private final ExecutorService executorService;

    private volatile boolean running = false;

    public VerifierNode(String rpcUrl, String privateKey, String contractAddress) {
        this.escrowClient = new EscrowClient(rpcUrl, privateKey, contractAddress);
        this.wasmSandbox = new WasmSandbox();
        this.executorService = Executors.newFixedThreadPool(4);

        logger.info("Initialized VerifierNode");
    }

    /**
     * Start the verifier node.
     */
    public void start() {
        logger.info("Starting VerifierNode...");
        running = true;

        // Start receipt monitoring thread
        executorService.submit(this::monitorReceipts);

        logger.info("VerifierNode started");
    }

    /**
     * Stop the verifier node.
     */
    public void stop() {
        logger.info("Stopping VerifierNode...");
        running = false;
        executorService.shutdown();
        wasmSandbox.close();
        escrowClient.close();
        logger.info("VerifierNode stopped");
    }

    /**
     * Monitor for submitted receipts and verify them.
     */
    private void monitorReceipts() {
        logger.info("Receipt monitoring started");

        while (running) {
            try {
                // Query contract for jobs in Receipt status
                // Listen to ReceiptSubmitted events via Web3j event subscriptions
                // Current implementation polls every 5 seconds

                Thread.sleep(5000);

            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                break;
            } catch (Exception e) {
                logger.error("Error in receipt monitoring", e);
            }
        }

        logger.info("Receipt monitoring stopped");
    }

    /**
     * Verify an execution receipt by re-running the job.
     * @param jobSpec Job specification
     * @param receipt Executor's receipt
     * @param wasmBytes Wasm module bytecode
     * @param inputBytes Input data
     * @return VerificationResult
     */
    public VerificationResult verifyReceipt(
        JobSpec jobSpec,
        ExecReceipt receipt,
        byte[] wasmBytes,
        byte[] inputBytes
    ) {
        byte[] jobId = jobSpec.getJobId();
        logger.info("Verifying receipt: jobId={}", CertusHash.toHex(jobId));

        try {
            // 1. Validate hashes
            if (!Arrays.equals(CertusHash.sha256(wasmBytes), jobSpec.getWasmHash())) {
                return VerificationResult.error("Wasm hash mismatch");
            }

            if (!Arrays.equals(CertusHash.sha256(inputBytes), jobSpec.getInputHash())) {
                return VerificationResult.error("Input hash mismatch");
            }

            // 2. Re-execute in sandbox
            WasmSandbox.ExecutionResult result = wasmSandbox.execute(
                wasmBytes,
                inputBytes,
                new WasmSandbox.ExecutionConfig(
                    jobSpec.getFuelLimit(),
                    jobSpec.getMemLimit(),
                    jobSpec.getMaxOutputSize()
                )
            );

            if (!result.isSuccess()) {
                logger.error("Re-execution failed: {}", result.getErrorMessage());
                return VerificationResult.error("Re-execution failed: " + result.getErrorMessage());
            }

            // 3. Compare output hashes
            byte[] recomputedOutput = result.getOutput();
            byte[] recomputedOutputHash = CertusHash.sha256(recomputedOutput);
            byte[] claimedOutputHash = receipt.getOutputHash();

            boolean matches = Arrays.equals(recomputedOutputHash, claimedOutputHash);

            if (matches) {
                logger.info("Verification PASSED: jobId={}, outputHash={}",
                    CertusHash.toHex(jobId), CertusHash.toHex(recomputedOutputHash));

                return VerificationResult.valid(recomputedOutput);
            } else {
                logger.warn("FRAUD DETECTED: jobId={}, claimed={}, recomputed={}",
                    CertusHash.toHex(jobId),
                    CertusHash.toHex(claimedOutputHash),
                    CertusHash.toHex(recomputedOutputHash));

                return VerificationResult.fraud(recomputedOutput, claimedOutputHash, recomputedOutputHash);
            }

        } catch (Exception e) {
            logger.error("Verification error", e);
            return VerificationResult.error("Verification error: " + e.getMessage());
        }
    }

    /**
     * Submit fraud proof to contract.
     * @param jobId Job identifier
     * @param wasmBytes Full Wasm module
     * @param inputBytes Full input data
     * @param claimedOutput Executor's claimed output
     */
    public TransactionReceipt submitFraudProof(
        byte[] jobId,
        byte[] wasmBytes,
        byte[] inputBytes,
        byte[] claimedOutput
    ) throws Exception {
        logger.info("Submitting fraud proof: jobId={}", CertusHash.toHex(jobId));

        TransactionReceipt receipt = escrowClient.submitFraud(jobId, wasmBytes, inputBytes, claimedOutput);

        logger.info("Fraud proof submitted: jobId={}, txHash={}",
            CertusHash.toHex(jobId), receipt.getTransactionHash());

        return receipt;
    }

    /**
     * Verification result.
     */
    public static class VerificationResult {
        private final Status status;
        private final byte[] recomputedOutput;
        private final byte[] claimedOutputHash;
        private final byte[] recomputedOutputHash;
        private final String errorMessage;

        public enum Status {
            VALID,      // Receipt is correct
            FRAUD,      // Fraud detected
            ERROR       // Verification error
        }

        private VerificationResult(Status status, byte[] recomputedOutput,
                                  byte[] claimedOutputHash, byte[] recomputedOutputHash,
                                  String errorMessage) {
            this.status = status;
            this.recomputedOutput = recomputedOutput;
            this.claimedOutputHash = claimedOutputHash;
            this.recomputedOutputHash = recomputedOutputHash;
            this.errorMessage = errorMessage;
        }

        public static VerificationResult valid(byte[] recomputedOutput) {
            return new VerificationResult(Status.VALID, recomputedOutput, null, null, null);
        }

        public static VerificationResult fraud(byte[] recomputedOutput, byte[] claimedHash, byte[] recomputedHash) {
            return new VerificationResult(Status.FRAUD, recomputedOutput, claimedHash, recomputedHash, null);
        }

        public static VerificationResult error(String errorMessage) {
            return new VerificationResult(Status.ERROR, null, null, null, errorMessage);
        }

        public Status getStatus() { return status; }
        public byte[] getRecomputedOutput() { return recomputedOutput; }
        public byte[] getClaimedOutputHash() { return claimedOutputHash; }
        public byte[] getRecomputedOutputHash() { return recomputedOutputHash; }
        public String getErrorMessage() { return errorMessage; }

        public boolean isValid() { return status == Status.VALID; }
        public boolean isFraud() { return status == Status.FRAUD; }
        public boolean isError() { return status == Status.ERROR; }
    }

    /**
     * Main entry point for verifier node.
     */
    public static void main(String[] args) {
        if (args.length < 3) {
            System.err.println("Usage: VerifierNode <rpcUrl> <privateKey> <contractAddress>");
            System.exit(1);
        }

        String rpcUrl = args[0];
        String privateKey = args[1];
        String contractAddress = args[2];

        VerifierNode node = new VerifierNode(rpcUrl, privateKey, contractAddress);
        node.start();

        // Add shutdown hook
        Runtime.getRuntime().addShutdownHook(new Thread(node::stop));

        // Keep running
        try {
            Thread.currentThread().join();
        } catch (InterruptedException e) {
            node.stop();
        }
    }
}
