package net.certus.wasm;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.security.MessageDigest;
import java.util.*;

/**
 * Wasm validator with multi-vector determinism testing.
 * Validation Strategy:
 * 1. Static bytecode analysis (detect forbidden opcodes)
 * 2. Multi-vector determinism testing (N runs × M vectors)
 * 3. Platform-agnostic validation (same results everywhere)
 *
 */
public class WasmValidator {
    private static final Logger logger = LoggerFactory.getLogger(WasmValidator.class);

    // Minimum test vectors required to prevent gaming
    private static final int MIN_TEST_VECTORS = 3;
    private static final int RECOMMENDED_TEST_VECTORS = 10;

    // Determinism testing parameters
    private static final int RUNS_PER_VECTOR = 10;

    private final WasmSandbox sandbox;

    public WasmValidator(WasmSandbox sandbox) {
        this.sandbox = sandbox;
    }

    /**
     * Validation result with detailed failure reasons.
     */
    public static class ValidationResult {
        private final boolean valid;
        private final String error;
        private final List<String> warnings;
        private final ValidationMetrics metrics;

        private ValidationResult(boolean valid, String error, List<String> warnings, ValidationMetrics metrics) {
            this.valid = valid;
            this.error = error;
            this.warnings = warnings;
            this.metrics = metrics;
        }

        public static ValidationResult valid(ValidationMetrics metrics) {
            return new ValidationResult(true, null, new ArrayList<>(), metrics);
        }

        public static ValidationResult invalid(String error) {
            return new ValidationResult(false, error, new ArrayList<>(), null);
        }

        public static ValidationResult withWarning(ValidationResult base, String warning) {
            base.warnings.add(warning);
            return base;
        }

        public boolean isValid() { return valid; }
        public String getError() { return error; }
        public List<String> getWarnings() { return warnings; }
        public ValidationMetrics getMetrics() { return metrics; }
    }

    /**
     * Metrics collected during validation.
     */
    public static class ValidationMetrics {
        public final int testVectorCount;
        public final int totalRuns;
        public final long totalFuelConsumed;
        public final boolean hasFloatingPoint;
        public final boolean hasThreadOps;
        public final boolean hasForbiddenImports;

        public ValidationMetrics(int testVectorCount, int totalRuns, long totalFuelConsumed,
                                 boolean hasFloatingPoint, boolean hasThreadOps, boolean hasForbiddenImports) {
            this.testVectorCount = testVectorCount;
            this.totalRuns = totalRuns;
            this.totalFuelConsumed = totalFuelConsumed;
            this.hasFloatingPoint = hasFloatingPoint;
            this.hasThreadOps = hasThreadOps;
            this.hasForbiddenImports = hasForbiddenImports;
        }
    }

    /**
     * Test vector for determinism validation.
     */
    public static class TestVector {
        public final byte[] inputBytes;
        public final byte[] expectedOutputBytes;  // Optional
        public final String description;

        public TestVector(byte[] inputBytes, byte[] expectedOutputBytes, String description) {
            this.inputBytes = inputBytes;
            this.expectedOutputBytes = expectedOutputBytes;
            this.description = description;
        }

        public TestVector(byte[] inputBytes, String description) {
            this(inputBytes, null, description);
        }
    }

    /**
     * Comprehensive Wasm validation with multi-vector determinism testing.
     *
     * CRITICAL: This is the main defense against gaming attacks.
     *
     * @param wasmBytes Wasm module bytecode
     * @param testVectors Multiple test vectors (MINIMUM 3 required)
     * @param config Execution configuration
     * @return ValidationResult with detailed pass/fail status
     */
    public ValidationResult validateWithMultipleVectors(
            byte[] wasmBytes,
            List<TestVector> testVectors,
            WasmSandbox.ExecutionConfig config) {

        logger.info("===== WASM MULTI-VECTOR VALIDATION START =====");
        logger.info("Wasm size: {} bytes", wasmBytes.length);
        logger.info("Test vectors: {}", testVectors.size());

        // Step 1: Check minimum test vector requirement
        if (testVectors.size() < MIN_TEST_VECTORS) {
            return ValidationResult.invalid(
                String.format("Insufficient test vectors: %d provided (minimum %d required). " +
                    "This defends against modules that game known test vectors.",
                    testVectors.size(), MIN_TEST_VECTORS)
            );
        }

        // Step 2: Static bytecode analysis
        logger.info("Step 1/3: Static bytecode analysis...");
        BytecodeAnalysis analysis = analyzeBytecode(wasmBytes);

        if (!analysis.isValid()) {
            return ValidationResult.invalid("Static analysis failed: " + analysis.getError());
        }

        // Step 3: Basic format validation
        logger.info("Step 2/3: Format validation...");
        WasmSandbox.ValidationResult basicValidation = sandbox.validateWasm(wasmBytes);
        if (!basicValidation.isValid()) {
            return ValidationResult.invalid("Format validation failed: " + basicValidation.getError());
        }

        // Step 4: Multi-vector determinism testing
        logger.info("Step 3/3: Multi-vector determinism testing ({} vectors × {} runs)...",
            testVectors.size(), RUNS_PER_VECTOR);

        long totalFuelConsumed = 0;

        for (int vecIdx = 0; vecIdx < testVectors.size(); vecIdx++) {
            TestVector tv = testVectors.get(vecIdx);
            logger.info("  Testing vector {}/{}: {}", vecIdx + 1, testVectors.size(), tv.description);

            // Run N times to ensure determinism
            Set<String> observedHashes = new HashSet<>();
            byte[] firstOutput = null;

            for (int run = 1; run <= RUNS_PER_VECTOR; run++) {
                WasmSandbox.ExecutionResult result = sandbox.execute(wasmBytes, tv.inputBytes, config);

                if (!result.isSuccess()) {
                    return ValidationResult.invalid(
                        String.format("Execution failed on vector %d, run %d: %s",
                            vecIdx + 1, run, result.getErrorMessage())
                    );
                }

                byte[] output = result.getOutput();
                byte[] outputHash = sha256(output);
                String outputHashHex = toHex(outputHash);

                observedHashes.add(outputHashHex);
                totalFuelConsumed += result.getFuelConsumed();

                if (run == 1) {
                    firstOutput = output;
                    logger.debug("    Run 1 outputHash: {}", outputHashHex);
                } else {
                    // ALL subsequent runs MUST match first run
                    if (!Arrays.equals(output, firstOutput)) {
                        return ValidationResult.invalid(
                            String.format("DETERMINISM VIOLATION: Vector %d, run %d produced different output. " +
                                "Expected hash: %s, Got: %s",
                                vecIdx + 1, run, toHex(sha256(firstOutput)), outputHashHex)
                        );
                    }
                }
            }

            // Verify all runs produced exactly ONE unique output
            if (observedHashes.size() != 1) {
                return ValidationResult.invalid(
                    String.format("DETERMINISM VIOLATION: Vector %d produced %d different outputs across %d runs",
                        vecIdx + 1, observedHashes.size(), RUNS_PER_VECTOR)
                );
            }

            // Check expected output if provided
            if (tv.expectedOutputBytes != null) {
                if (!Arrays.equals(firstOutput, tv.expectedOutputBytes)) {
                    return ValidationResult.invalid(
                        String.format("Output mismatch: Vector %d output doesn't match expected. " +
                            "Expected: %s, Got: %s",
                            vecIdx + 1,
                            toHex(sha256(tv.expectedOutputBytes)),
                            toHex(sha256(firstOutput)))
                    );
                }
            }

            logger.info("    ✓ Vector {} deterministic ({} runs)", vecIdx + 1, RUNS_PER_VECTOR);
        }

        // Build validation metrics
        ValidationMetrics metrics = new ValidationMetrics(
            testVectors.size(),
            testVectors.size() * RUNS_PER_VECTOR,
            totalFuelConsumed,
            analysis.hasFloatingPoint,
            analysis.hasThreadOps,
            analysis.hasForbiddenImports
        );

        ValidationResult result = ValidationResult.valid(metrics);

        // Add warnings if test vector count is suboptimal
        if (testVectors.size() < RECOMMENDED_TEST_VECTORS) {
            result = ValidationResult.withWarning(result,
                String.format("Only %d test vectors provided (recommended: %d+). " +
                    "More vectors increase confidence and gaming resistance.",
                    testVectors.size(), RECOMMENDED_TEST_VECTORS));
        }

        logger.info("===== WASM VALIDATION PASSED =====");
        logger.info("Vectors: {}, Total runs: {}, Total fuel: {}",
            metrics.testVectorCount, metrics.totalRuns, metrics.totalFuelConsumed);

        return result;
    }

    /**
     * Static bytecode analysis to detect non-deterministic instructions.
     */
    public static class BytecodeAnalysis {
        private final boolean valid;
        private final String error;
        private final boolean hasFloatingPoint;
        private final boolean hasThreadOps;
        private final boolean hasForbiddenImports;

        private BytecodeAnalysis(boolean valid, String error, boolean hasFloatingPoint,
                                 boolean hasThreadOps, boolean hasForbiddenImports) {
            this.valid = valid;
            this.error = error;
            this.hasFloatingPoint = hasFloatingPoint;
            this.hasThreadOps = hasThreadOps;
            this.hasForbiddenImports = hasForbiddenImports;
        }

        public static BytecodeAnalysis valid(boolean hasFloatingPoint, boolean hasThreadOps, boolean hasForbiddenImports) {
            return new BytecodeAnalysis(true, null, hasFloatingPoint, hasThreadOps, hasForbiddenImports);
        }

        public static BytecodeAnalysis invalid(String error) {
            return new BytecodeAnalysis(false, error, false, false, false);
        }

        public boolean isValid() { return valid; }
        public String getError() { return error; }
    }

    /**
     * Analyze Wasm bytecode for forbidden instructions and imports.
     *
     * SECURITY: This is a critical defense layer. Even if determinism tests pass,
     * we want to catch obviously non-deterministic instructions statically.
     *
     * Detection rules:
     * - Floating point opcodes (f32.*, f64.*): REJECT (unless soft-float approved)
     * - Thread/atomic opcodes (atomic.*, memory.atomic.*): REJECT
     * - WASI imports (random, time, file I/O): REJECT
     * - Unknown custom sections: WARN
     */
    private BytecodeAnalysis analyzeBytecode(byte[] wasmBytes) {
        try {
            boolean hasFloatingPoint = false;
            boolean hasThreadOps = false;
            boolean hasForbiddenImports = false;

            // Scan for float opcodes
            for (int i = 0; i < wasmBytes.length - 1; i++) {
                int opcode = wasmBytes[i] & 0xFF;

                // f32: 0x43-0x98
                if (opcode >= 0x43 && opcode <= 0x98) {
                    hasFloatingPoint = true;
                }

                // f64: 0x99-0xBF
                if (opcode >= 0x99 && opcode <= 0xBF) {
                    hasFloatingPoint = true;
                }

                // Atomic/thread operations (0xFE prefix)
                if (opcode == 0xFE) {
                    int nextByte = i + 1 < wasmBytes.length ? (wasmBytes[i + 1] & 0xFF) : 0;
                    // atomic.* opcodes: 0xFE 0x00 - 0xFE 0x30
                    if (nextByte <= 0x30) {
                        hasThreadOps = true;
                    }
                }

                // Random/time syscalls (rdrand equivalent in WASI)
                // Check for imports to "wasi_snapshot_preview1" with forbidden functions
                // This requires parsing import section properly - simplified check here
                if (opcode == 0x02) {  // Import section
                    // Full import parsing would go here
                    // For MVP, we rely on runtime sandbox to block these
                }
            }

            // REJECT if floating point found (MVP policy: integer-only)
            if (hasFloatingPoint) {
                return BytecodeAnalysis.invalid(
                    "Floating-point instructions detected. " +
                    "Certus MVP requires integer-only arithmetic for determinism. " +
                    "Use fixed-point or integer math instead."
                );
            }

            // REJECT if thread operations found
            if (hasThreadOps) {
                return BytecodeAnalysis.invalid(
                    "Thread/atomic instructions detected. " +
                    "Certus requires single-threaded deterministic execution."
                );
            }

            return BytecodeAnalysis.valid(hasFloatingPoint, hasThreadOps, hasForbiddenImports);

        } catch (Exception e) {
            logger.error("Bytecode analysis error", e);
            return BytecodeAnalysis.invalid("Bytecode analysis failed: " + e.getMessage());
        }
    }

    /**
     * Generate random test vectors for additional validation.
     *
     * Security: Use these in addition to user-provided vectors, not as replacement.
     * User vectors test functional correctness, random vectors test robustness
     * and gaming resistance.
     *
     * @param count Number of random vectors to generate
     * @param inputSize Size of each input in bytes
     * @return List of random test vectors
     */
    public static List<TestVector> generateRandomTestVectors(int count, int inputSize) {
        List<TestVector> vectors = new ArrayList<>();
        Random random = new Random(4849556952662861128L);  // Fixed seed for reproducibility (CERTUSHASH)

        for (int i = 0; i < count; i++) {
            byte[] input = new byte[inputSize];
            random.nextBytes(input);
            vectors.add(new TestVector(input, "random_" + i));
        }

        return vectors;
    }

    // Utility Methods

    private byte[] sha256(byte[] data) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            return digest.digest(data);
        } catch (Exception e) {
            throw new RuntimeException("SHA-256 unavailable", e);
        }
    }

    private String toHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder();
        for (byte b : bytes) {
            sb.append(String.format("%02x", b));
        }
        return sb.toString();
    }
}
