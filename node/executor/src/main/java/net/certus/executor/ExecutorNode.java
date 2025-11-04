package net.certus.executor;

import net.certus.client.EscrowClient;
import net.certus.crypto.CertusHash;
import net.certus.crypto.CertusKeys;
import net.certus.model.ExecReceipt;
import net.certus.model.JobSpec;
import net.certus.wasm.WasmSandbox;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.web3j.protocol.core.methods.response.TransactionReceipt;

import java.math.BigInteger;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * Executor node that accepts and runs compute jobs.
 *
 * CERTUS Capital Efficiency Integration:
 * - Queries CERTUS stake to determine collateral multiplier
 * - Stake 10,000 CERTUS for 0.8x collateral requirement
 * - Stake 50,000 CERTUS for 0.6x collateral requirement
 * - Filters jobs by acceptable collateral ratio and available capital
 *
 * Flow:
 * 1. Monitor for new jobs (via Directory service or direct queries)
 * 2. Check CERTUS stake for capital efficiency
 * 3. Accept job by posting reduced collateral (if CERTUS staked)
 * 4. Download Wasm module and input data
 * 5. Execute in deterministic sandbox
 * 6. Sign receipt with Ed25519 key
 * 7. Submit receipt on-chain
 * 8. Wait for finalization or handle disputes
 */
public class ExecutorNode {
    private static final Logger logger = LoggerFactory.getLogger(ExecutorNode.class);

    private final EscrowClient escrowClient;
    private final WasmSandbox wasmSandbox;
    private final CertusKeys.KeyPair signingKey;
    private final String executorAddress;
    private final BigInteger maxCollateralCapital;
    private final int minCollateralRatioBps; // 100-300 (1.0x-3.0x)
    private final int maxCollateralRatioBps;
    private final String certusTokenAddress;
    private final ExecutorService executorService;

    private volatile boolean running = false;
    private volatile int cachedCollateralMultiplier = 10000; // 1.0x default (basis points)

    public ExecutorNode(
        String rpcUrl,
        String privateKey,
        String contractAddress,
        String certusTokenAddress,
        byte[] signingKeySeed,
        BigInteger maxCollateralCapital,
        int minCollateralRatioBps,
        int maxCollateralRatioBps
    ) {
        this.escrowClient = new EscrowClient(rpcUrl, privateKey, contractAddress);
        this.wasmSandbox = new WasmSandbox();
        this.signingKey = CertusKeys.fromPrivateKey(signingKeySeed);
        this.executorAddress = deriveEthAddress(privateKey);
        this.maxCollateralCapital = maxCollateralCapital;
        this.minCollateralRatioBps = minCollateralRatioBps;
        this.maxCollateralRatioBps = maxCollateralRatioBps;
        this.certusTokenAddress = certusTokenAddress;
        this.executorService = Executors.newFixedThreadPool(4);

        logger.info("Initialized ExecutorNode: address={}, maxCapital={}, ratioRange={}-{}",
            executorAddress, maxCollateralCapital, minCollateralRatioBps, maxCollateralRatioBps);

        // Query CERTUS capital efficiency on startup
        updateCapitalEfficiency();
    }

    /**
     * Update capital efficiency from CERTUS token contract.
     * Called on startup and periodically.
     */
    private void updateCapitalEfficiency() {
        try {
            int multiplier = escrowClient.getExecutorCollateralMultiplier(executorAddress, certusTokenAddress);
            cachedCollateralMultiplier = multiplier;

            String efficiency = multiplier == 6000 ? "0.6x (50k CERTUS)" :
                               multiplier == 8000 ? "0.8x (10k CERTUS)" : "1.0x (no boost)";

            logger.info("Capital efficiency updated: multiplier={}bps ({})", multiplier, efficiency);
        } catch (Exception e) {
            logger.warn("Failed to query capital efficiency: {}", e.getMessage());
            cachedCollateralMultiplier = 10000; // Default to 1.0x
        }
    }

    /**
     * Check if job is acceptable based on collateral requirements and capital limits.
     */
    public boolean canAcceptJob(JobSpec jobSpec) {
        // Check collateral ratio is within acceptable range
        int jobCollateralRatioBps = jobSpec.getCollateralRatioBps();
        if (jobCollateralRatioBps < minCollateralRatioBps || jobCollateralRatioBps > maxCollateralRatioBps) {
            logger.debug("Job {} rejected: collateral ratio {}bps outside range {}-{}",
                CertusHash.toHex(jobSpec.getJobId()), jobCollateralRatioBps,
                minCollateralRatioBps, maxCollateralRatioBps);
            return false;
        }

        // Calculate effective collateral requirement with CERTUS boost
        BigInteger baseCollateral = jobSpec.getPayAmt().multiply(BigInteger.valueOf(jobCollateralRatioBps)).divide(BigInteger.valueOf(100));
        BigInteger effectiveCollateral = baseCollateral.multiply(BigInteger.valueOf(cachedCollateralMultiplier)).divide(BigInteger.valueOf(10000));

        if (effectiveCollateral.compareTo(maxCollateralCapital) > 0) {
            logger.debug("Job {} rejected: effective collateral {} exceeds max capital {}",
                CertusHash.toHex(jobSpec.getJobId()), effectiveCollateral, maxCollateralCapital);
            return false;
        }

        logger.info("Job {} accepted: payment={}, ratio={}bps, effectiveCollateral={} ({}x boost)",
            CertusHash.toHex(jobSpec.getJobId()), jobSpec.getPayAmt(), jobCollateralRatioBps,
            effectiveCollateral, cachedCollateralMultiplier / 10000.0);

        return true;
    }

    /**
     * Start the executor node.
     */
    public void start() {
        logger.info("Starting ExecutorNode...");
        running = true;

        // Start job monitoring thread
        executorService.submit(this::monitorJobs);

        // Periodically update capital efficiency
        executorService.submit(this::capitalEfficiencyUpdater);

        logger.info("ExecutorNode started");
    }

    /**
     * Stop the executor node.
     */
    public void stop() {
        logger.info("Stopping ExecutorNode...");
        running = false;
        executorService.shutdown();
        wasmSandbox.close();
        escrowClient.close();
        logger.info("ExecutorNode stopped");
    }

    /**
     * Periodically update capital efficiency from CERTUS token.
     */
    private void capitalEfficiencyUpdater() {
        logger.info("Capital efficiency updater started");

        while (running) {
            try {
                // Update every 5 minutes
                Thread.sleep(5 * 60 * 1000);
                updateCapitalEfficiency();

            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                break;
            } catch (Exception e) {
                logger.error("Error updating capital efficiency", e);
            }
        }

        logger.info("Capital efficiency updater stopped");
    }

    /**
     * Monitor for new jobs and process them.
     */
    private void monitorJobs() {
        logger.info("Job monitoring started");

        while (running) {
            try {
                // Query Directory service or contract events for new jobs
                // Current implementation polls every 5 seconds
                // Production deployment should use WebSocket event subscriptions

                Thread.sleep(5000);

            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                break;
            } catch (Exception e) {
                logger.error("Error in job monitoring", e);
            }
        }

        logger.info("Job monitoring stopped");
    }

    /**
     * Accept and execute a job.
     * @param jobSpec Job specification
     * @param wasmBytes Wasm module bytecode
     * @param inputBytes Input data
     */
    public ExecReceipt executeJob(JobSpec jobSpec, byte[] wasmBytes, byte[] inputBytes) throws Exception {
        byte[] jobId = jobSpec.getJobId();
        logger.info("Executing job: {}", CertusHash.toHex(jobId));

        // 1. Validate job parameters
        validateJob(jobSpec, wasmBytes, inputBytes);

        // 2. Accept job on-chain (post effective collateral with CERTUS boost)
        acceptJobOnChain(jobId, jobSpec);

        // 3. Execute in Wasm sandbox
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
            logger.error("Execution failed for job {}: {}", CertusHash.toHex(jobId), result.getErrorMessage());
            throw new ExecutionException("Execution failed: " + result.getErrorMessage());
        }

        byte[] output = result.getOutput();
        byte[] outputHash = CertusHash.sha256(output);

        logger.info("Execution successful: jobId={}, outputHash={}, fuelConsumed={}",
            CertusHash.toHex(jobId), CertusHash.toHex(outputHash), result.getFuelConsumed());

        // 4. Calculate effective deposit used
        BigInteger baseCollateral = jobSpec.getPayAmt()
            .multiply(BigInteger.valueOf(jobSpec.getCollateralRatioBps()))
            .divide(BigInteger.valueOf(100));
        BigInteger effectiveDeposit = baseCollateral
            .multiply(BigInteger.valueOf(cachedCollateralMultiplier))
            .divide(BigInteger.valueOf(10000));

        // 5. Sign receipt
        ExecReceipt receipt = ExecReceipt.sign(
            jobId,
            jobSpec.getWasmHash(),
            jobSpec.getInputHash(),
            outputHash,
            executorAddress,
            effectiveDeposit,
            signingKey
        );

        // 5. Submit receipt on-chain
        submitReceiptOnChain(receipt);

        logger.info("Receipt submitted for job: {}", CertusHash.toHex(jobId));

        return receipt;
    }

    /**
     * Validate job before accepting.
     */
    private void validateJob(JobSpec jobSpec, byte[] wasmBytes, byte[] inputBytes) {
        // Verify hashes
        byte[] computedWasmHash = CertusHash.sha256(wasmBytes);
        byte[] computedInputHash = CertusHash.sha256(inputBytes);

        if (!java.util.Arrays.equals(computedWasmHash, jobSpec.getWasmHash())) {
            throw new IllegalArgumentException("Wasm hash mismatch");
        }

        if (!java.util.Arrays.equals(computedInputHash, jobSpec.getInputHash())) {
            throw new IllegalArgumentException("Input hash mismatch");
        }

        // Validate Wasm for determinism
        WasmSandbox.ValidationResult validation = wasmSandbox.validateWasm(wasmBytes);
        if (!validation.isValid()) {
            throw new IllegalArgumentException("Wasm validation failed: " + validation.getError());
        }

        // Check if job is acceptable (collateral ratio + capital limits)
        if (!canAcceptJob(jobSpec)) {
            throw new IllegalArgumentException("Job not acceptable: collateral requirements exceed limits");
        }

        logger.info("Job validation passed: {}", CertusHash.toHex(jobSpec.getJobId()));
    }

    /**
     * Accept job on-chain by posting effective collateral (with CERTUS boost).
     */
    private void acceptJobOnChain(byte[] jobId, JobSpec jobSpec) throws Exception {
        logger.info("Accepting job on-chain: {}", CertusHash.toHex(jobId));

        // Calculate effective collateral with CERTUS capital efficiency
        BigInteger baseCollateral = jobSpec.getPayAmt()
            .multiply(BigInteger.valueOf(jobSpec.getCollateralRatioBps()))
            .divide(BigInteger.valueOf(100));

        BigInteger effectiveCollateral = baseCollateral
            .multiply(BigInteger.valueOf(cachedCollateralMultiplier))
            .divide(BigInteger.valueOf(10000));

        logger.info("Collateral calculation: base={}, multiplier={}bps, effective={}",
            baseCollateral, cachedCollateralMultiplier, effectiveCollateral);

        // Get token address from job spec
        String tokenAddress = jobSpec.getPayToken();

        escrowClient.acceptJob(jobId, effectiveCollateral, tokenAddress);

        logger.info("Job accepted on-chain with effective collateral: {}", effectiveCollateral);
    }

    /**
     * Submit execution receipt on-chain.
     */
    private void submitReceiptOnChain(ExecReceipt receipt) throws Exception {
        logger.info("Submitting receipt on-chain: jobId={}", CertusHash.toHex(receipt.getJobId()));

        escrowClient.submitReceipt(
            receipt.getJobId(),
            receipt.getOutputHash(),
            receipt.getExecutorSig()
        );

        logger.info("Receipt submitted on-chain: {}", CertusHash.toHex(receipt.getJobId()));
    }

    /**
     * Derive Ethereum address from private key.
     */
    private String deriveEthAddress(String privateKey) {
        org.web3j.crypto.Credentials creds = org.web3j.crypto.Credentials.create(privateKey);
        return creds.getAddress();
    }

    public static class ExecutionException extends Exception {
        public ExecutionException(String message) {
            super(message);
        }
    }

    /**
     * Main entry point for executor node.
     */
    public static void main(String[] args) {
        if (args.length < 7) {
            System.err.println("Usage: ExecutorNode <rpcUrl> <privateKey> <contractAddress> <certusTokenAddress> <signingKeySeed> <maxCapital> <minRatio> <maxRatio>");
            System.err.println("Example: ExecutorNode https://arb-sepolia.g.alchemy.com/... 0x... 0x... 0x... abc123... 10000000000 100 300");
            System.exit(1);
        }

        String rpcUrl = args[0];
        String privateKey = args[1];
        String contractAddress = args[2];
        String certusTokenAddress = args[3];
        byte[] signingKeySeed = CertusHash.fromHex(args[4]);
        BigInteger maxCollateralCapital = new BigInteger(args[5]); // e.g., 10000000000 = $10,000 USDC
        int minCollateralRatioBps = Integer.parseInt(args[6]); // e.g., 100 = 1.0x
        int maxCollateralRatioBps = Integer.parseInt(args[7]); // e.g., 300 = 3.0x

        ExecutorNode node = new ExecutorNode(
            rpcUrl,
            privateKey,
            contractAddress,
            certusTokenAddress,
            signingKeySeed,
            maxCollateralCapital,
            minCollateralRatioBps,
            maxCollateralRatioBps
        );
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
