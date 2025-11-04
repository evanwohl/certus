package net.certus.client;

import net.certus.crypto.CertusHash;
import net.certus.model.JobSpec;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.web3j.abi.FunctionEncoder;
import org.web3j.abi.TypeReference;
import org.web3j.abi.datatypes.*;
import org.web3j.abi.datatypes.generated.Bytes32;
import org.web3j.abi.datatypes.generated.Uint256;
import org.web3j.abi.datatypes.generated.Uint64;
import org.web3j.crypto.Credentials;
import org.web3j.protocol.Web3j;
import org.web3j.protocol.core.methods.response.TransactionReceipt;
import org.web3j.protocol.core.methods.response.EthSendTransaction;
import org.web3j.protocol.core.methods.request.Transaction;
import org.web3j.protocol.http.HttpService;
import org.web3j.tx.RawTransactionManager;
import org.web3j.tx.TransactionManager;
import org.web3j.tx.gas.DefaultGasProvider;

import java.math.BigInteger;
import java.util.Arrays;
import java.util.Collections;
import java.util.Optional;

/**
 * Client for interacting with CertusEscrow smart contract.
 * Handles job creation, finalization, and fraud proof submission.
 */
public class EscrowClient {
    private static final Logger logger = LoggerFactory.getLogger(EscrowClient.class);

    private final Web3j web3j;
    private final Credentials credentials;
    private final String contractAddress;
    private final TransactionManager txManager;

    public EscrowClient(String rpcUrl, String privateKey, String contractAddress) {
        this.web3j = Web3j.build(new HttpService(rpcUrl));
        this.credentials = Credentials.create(privateKey);
        this.contractAddress = contractAddress;
        this.txManager = new RawTransactionManager(web3j, credentials);

        logger.info("Initialized EscrowClient: address={}, contract={}",
            credentials.getAddress(), contractAddress);
    }

    /**
     * Register a Wasm module on-chain.
     * @param wasmBytes Wasm module bytecode (max 24KB)
     * @return SHA256 hash of the Wasm module
     */
    public byte[] registerWasm(byte[] wasmBytes) throws Exception {
        logger.info("Registering Wasm module: {} bytes", wasmBytes.length);

        if (wasmBytes.length > 24 * 1024) {
            throw new IllegalArgumentException("Wasm module exceeds 24KB limit");
        }

        byte[] wasmHash = CertusHash.sha256(wasmBytes);

        // Encode function call: registerWasm(bytes calldata wasm)
        Function function = new Function(
            "registerWasm",
            Arrays.asList(new DynamicBytes(wasmBytes)),
            Collections.singletonList(new TypeReference<Bytes32>() {})
        );

        String encodedFunction = FunctionEncoder.encode(function);

        EthSendTransaction ethSendTx = txManager.sendTransaction(
            DefaultGasProvider.GAS_PRICE,
            DefaultGasProvider.GAS_LIMIT,
            contractAddress,
            encodedFunction,
            BigInteger.ZERO
        );

        String txHash = ethSendTx.getTransactionHash();
        TransactionReceipt receipt = waitForReceipt(txHash);

        logger.info("Wasm registered: hash={}, txHash={}", CertusHash.toHex(wasmHash), receipt.getTransactionHash());

        return wasmHash;
    }

    /**
     * Create a new compute job.
     * @param jobSpec Job specification
     * @param payTokenAddress ERC20 token address (USDC/USDT/DAI)
     * @return Transaction receipt
     */
    public TransactionReceipt createJob(JobSpec jobSpec, String payTokenAddress) throws Exception {
        logger.info("Creating job: {}", CertusHash.toHex(jobSpec.getJobId()));

        // First, approve token spending
        approveToken(payTokenAddress, jobSpec.getPayAmt().add(jobSpec.getClientDeposit()));

        // Encode createJob function call
        Function function = new Function(
            "createJob",
            Arrays.asList(
                new Bytes32(jobSpec.getJobId()),
                new Address(payTokenAddress),
                new Uint256(jobSpec.getPayAmt()),
                new Bytes32(jobSpec.getWasmHash()),
                new Bytes32(jobSpec.getInputHash()),
                new Uint64(BigInteger.valueOf(jobSpec.getAcceptDeadline())),
                new Uint64(BigInteger.valueOf(jobSpec.getFinalizeDeadline())),
                new Uint64(BigInteger.valueOf(jobSpec.getFuelLimit())),
                new Uint64(BigInteger.valueOf(jobSpec.getMemLimit())),
                new org.web3j.abi.datatypes.generated.Uint32(BigInteger.valueOf(jobSpec.getMaxOutputSize()))
            ),
            Collections.emptyList()
        );

        String encodedFunction = FunctionEncoder.encode(function);

        EthSendTransaction ethSendTx = txManager.sendTransaction(
            DefaultGasProvider.GAS_PRICE,
            DefaultGasProvider.GAS_LIMIT.multiply(BigInteger.valueOf(2)), // Higher gas for storage
            contractAddress,
            encodedFunction,
            BigInteger.ZERO
        );

        String txHash = ethSendTx.getTransactionHash();
        TransactionReceipt receipt = waitForReceipt(txHash);

        logger.info("Job created: jobId={}, txHash={}",
            CertusHash.toHex(jobSpec.getJobId()), receipt.getTransactionHash());

        return receipt;
    }

    /**
     * Finalize a job (client calls after verifying receipt).
     * @param jobId Job identifier
     * @return Transaction receipt
     */
    public TransactionReceipt finalizeJob(byte[] jobId) throws Exception {
        logger.info("Finalizing job: {}", CertusHash.toHex(jobId));

        Function function = new Function(
            "finalize",
            Collections.singletonList(new Bytes32(jobId)),
            Collections.emptyList()
        );

        String encodedFunction = FunctionEncoder.encode(function);

        EthSendTransaction ethSendTx = txManager.sendTransaction(
            DefaultGasProvider.GAS_PRICE,
            DefaultGasProvider.GAS_LIMIT,
            contractAddress,
            encodedFunction,
            BigInteger.ZERO
        );

        String txHash = ethSendTx.getTransactionHash();
        TransactionReceipt receipt = waitForReceipt(txHash);

        logger.info("Job finalized: jobId={}, txHash={}", CertusHash.toHex(jobId), receipt.getTransactionHash());

        return receipt;
    }

    /**
     * Accept a job by posting executor collateral.
     * @param jobId Job identifier
     * @param executorDeposit Amount of collateral to post
     * @return Transaction receipt
     */
    public TransactionReceipt acceptJob(byte[] jobId, BigInteger executorDeposit, String tokenAddress) throws Exception {
        logger.info("Accepting job: jobId={}, deposit={}", CertusHash.toHex(jobId), executorDeposit);

        approveToken(tokenAddress, executorDeposit);

        Function function = new Function(
            "acceptJob",
            Collections.singletonList(new Bytes32(jobId)),
            Collections.emptyList()
        );

        String encodedFunction = FunctionEncoder.encode(function);

        EthSendTransaction ethSendTx = txManager.sendTransaction(
            DefaultGasProvider.GAS_PRICE,
            DefaultGasProvider.GAS_LIMIT,
            contractAddress,
            encodedFunction,
            BigInteger.ZERO
        );

        String txHash = ethSendTx.getTransactionHash();
        TransactionReceipt receipt = waitForReceipt(txHash);

        logger.info("Job accepted: jobId={}, txHash={}", CertusHash.toHex(jobId), receipt.getTransactionHash());

        return receipt;
    }

    /**
     * Submit execution receipt with output hash and signature.
     * @param jobId Job identifier
     * @param outputHash SHA256 hash of execution output
     * @param executorSig Ed25519 signature over receipt
     * @return Transaction receipt
     */
    public TransactionReceipt submitReceipt(byte[] jobId, byte[] outputHash, byte[] executorSig) throws Exception {
        logger.info("Submitting receipt: jobId={}, outputHash={}",
            CertusHash.toHex(jobId), CertusHash.toHex(outputHash));

        Function function = new Function(
            "submitReceipt",
            Arrays.asList(
                new Bytes32(jobId),
                new Bytes32(outputHash),
                new DynamicBytes(executorSig)
            ),
            Collections.emptyList()
        );

        String encodedFunction = FunctionEncoder.encode(function);

        EthSendTransaction ethSendTx = txManager.sendTransaction(
            DefaultGasProvider.GAS_PRICE,
            DefaultGasProvider.GAS_LIMIT,
            contractAddress,
            encodedFunction,
            BigInteger.ZERO
        );

        String txHash = ethSendTx.getTransactionHash();
        TransactionReceipt receipt = waitForReceipt(txHash);

        logger.info("Receipt submitted: jobId={}, txHash={}", CertusHash.toHex(jobId), receipt.getTransactionHash());

        return receipt;
    }

    /**
     * Submit fraud proof to slash dishonest executor.
     * @param jobId Job identifier
     * @param wasmBytes Full Wasm module
     * @param inputBytes Full input data
     * @param claimedOutput Executor's claimed output
     * @return Transaction receipt
     */
    public TransactionReceipt submitFraud(byte[] jobId, byte[] wasmBytes, byte[] inputBytes, byte[] claimedOutput) throws Exception {
        logger.info("Submitting fraud proof: jobId={}", CertusHash.toHex(jobId));

        Function function = new Function(
            "fraudOnChain",
            Arrays.asList(
                new Bytes32(jobId),
                new DynamicBytes(wasmBytes),
                new DynamicBytes(inputBytes),
                new DynamicBytes(claimedOutput)
            ),
            Collections.emptyList()
        );

        String encodedFunction = FunctionEncoder.encode(function);

        EthSendTransaction ethSendTx = txManager.sendTransaction(
            DefaultGasProvider.GAS_PRICE,
            BigInteger.valueOf(3_000_000), // High gas for on-chain Wasm execution
            contractAddress,
            encodedFunction,
            BigInteger.ZERO
        );

        String txHash = ethSendTx.getTransactionHash();
        TransactionReceipt receipt = waitForReceipt(txHash);

        logger.info("Fraud proof submitted: jobId={}, txHash={}", CertusHash.toHex(jobId), receipt.getTransactionHash());

        return receipt;
    }

    /**
     * Get executor collateral multiplier from CERTUS token contract.
     * Returns basis points (10000 = 1.0x, 8000 = 0.8x, 6000 = 0.6x)
     */
    public int getExecutorCollateralMultiplier(String executorAddress, String certusTokenAddress) throws Exception {
        // Returns default 1.0x collateral requirement (10000 bps)
        // Production: query CERTUS token contract for executor stake-based discount
        return 10000;
    }

    /**
     * Approve ERC20 token spending.
     */
    private void approveToken(String tokenAddress, BigInteger amount) throws Exception {
        Function function = new Function(
            "approve",
            Arrays.asList(new Address(contractAddress), new Uint256(amount)),
            Collections.emptyList()
        );

        String encodedFunction = FunctionEncoder.encode(function);

        txManager.sendTransaction(
            DefaultGasProvider.GAS_PRICE,
            DefaultGasProvider.GAS_LIMIT,
            tokenAddress,
            encodedFunction,
            BigInteger.ZERO
        );

        logger.debug("Token approved: token={}, amount={}", tokenAddress, amount);
    }

    private TransactionReceipt waitForReceipt(String txHash) throws Exception {
        Optional<TransactionReceipt> receiptOptional;
        int attempts = 0;
        int maxAttempts = 40; // 40 attempts * 3 seconds = 2 minutes max

        do {
            receiptOptional = web3j.ethGetTransactionReceipt(txHash).send().getTransactionReceipt();
            if (receiptOptional.isPresent()) {
                return receiptOptional.get();
            }
            Thread.sleep(3000); // Wait 3 seconds between checks
            attempts++;
        } while (attempts < maxAttempts);

        throw new RuntimeException("Transaction receipt not received after " + maxAttempts + " attempts");
    }

    public void close() {
        web3j.shutdown();
    }
}
