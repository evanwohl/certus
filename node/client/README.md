# Certus Client SDK

Java SDK for submitting compute jobs to the Certus protocol.

## Usage

```java
EscrowClient client = new EscrowClient(
    rpcUrl,
    privateKey,
    contractAddress
);

// Create job
byte[] wasmHash = CertusHash.sha256(wasmBytes);
byte[] inputHash = CertusHash.sha256(inputBytes);
BigInteger payment = BigInteger.valueOf(1000000);

TransactionReceipt receipt = client.createJob(
    jobId,
    wasmHash,
    inputHash,
    payment
);

// Check status
JobStatus status = client.getJobStatus(jobId);

// Finalize after execution
client.finalizeJob(jobId);
```

## Key Classes

- **EscrowClient** - Main interface to CertusEscrow contract
- **CertusHash** - SHA256 hashing utilities
- **CertusKeys** - Ed25519 key management
- **JobSpec** - Job specification model
- **ExecReceipt** - Execution receipt model
