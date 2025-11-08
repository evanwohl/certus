/**
 * Job lifecycle states
 */
export const JobState = {
  QUEUED: 'queued',
  COMPILING: 'compiling',
  EXECUTING: 'executing',
  VERIFYING: 'verifying',
  VERIFIED: 'verified',
  FRAUD: 'fraud',
  FAILED: 'failed'
};

/**
 * Node types
 */
export const NodeType = {
  EXECUTOR: 'executor',
  VERIFIER: 'verifier'
};

/**
 * Create job submission payload
 */
export function createJob(pythonCode, challengeId = null) {
  return {
    pythonCode,
    challengeId,
    timestamp: Date.now()
  };
}

/**
 * Create execution receipt
 */
export function createReceipt(jobId, outputHash, wasmHash, executorId, signature) {
  return {
    jobId,
    outputHash,
    wasmHash,
    executorId,
    signature,
    timestamp: Date.now()
  };
}

/**
 * Create verification result
 */
export function createVerification(jobId, outputHash, verifierId, signature, matches) {
  return {
    jobId,
    outputHash,
    verifierId,
    signature,
    matches,
    timestamp: Date.now()
  };
}

/**
 * Create fraud proof
 */
export function createFraudProof(jobId, expectedHash, actualHash, verifierId) {
  return {
    jobId,
    expectedHash,
    actualHash,
    verifierId,
    timestamp: Date.now()
  };
}
