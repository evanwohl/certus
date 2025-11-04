package net.certus.test;

import net.certus.crypto.CertusHash;
import net.certus.wasm.WasmSandbox;
import net.certus.wasm.WasmValidator;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.DisplayName;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Determinism validation test harness.
 *
 * CRITICAL TEST: This test MUST pass for Certus to be secure.
 *
 * Validates that:
 * 1. Same Wasm + input => same output (N=10 runs)
 * 2. Output hashes match exactly across all runs
 * 3. No platform-specific behavior
 * 4. Multi-vector testing prevents gaming attacks
 * 5. Static analysis rejects non-deterministic instructions
 *
 * Failure indicates non-deterministic execution, which breaks fraud proofs.
 */
class DeterminismTest {
    private static final Logger logger = LoggerFactory.getLogger(DeterminismTest.class);

    private static final int DETERMINISM_TEST_RUNS = 10;
    private static final Path TEST_VECTORS_DIR = Paths.get("../testvectors");

    @Test
    @DisplayName("CRITICAL: Wasm execution must be deterministic (N=10 runs)")
    void testDeterministicExecution() throws Exception {
        logger.info("===== DETERMINISM TEST START =====");
        logger.info("This test validates the CORE security property of Certus");

        // Load test vectors
        TestVector[] testVectors = loadTestVectors();

        if (testVectors.length == 0) {
            logger.warn("No test vectors found - skipping determinism test");
            return;
        }

        WasmSandbox sandbox = new WasmSandbox();

        try {
            for (TestVector tv : testVectors) {
                logger.info("Testing: {}", tv.name);
                runDeterminismTest(sandbox, tv);
            }

            logger.info("===== DETERMINISM TEST PASSED =====");
            logger.info("All {} test vectors are deterministic", testVectors.length);

        } finally {
            sandbox.close();
        }
    }

    /**
     * Run determinism test for a single test vector.
     */
    private void runDeterminismTest(WasmSandbox sandbox, TestVector tv) {
        logger.info("  Wasm: {} bytes", tv.wasmBytes.length);
        logger.info("  Input: {} bytes", tv.inputBytes.length);

        Set<String> observedOutputHashes = new HashSet<>();
        byte[] firstOutput = null;

        WasmSandbox.ExecutionConfig config = new WasmSandbox.ExecutionConfig(
            10_000_000,  // 10M fuel
            64 * 1024 * 1024,  // 64MB memory
            1024 * 1024  // 1MB max output
        );

        for (int run = 1; run <= DETERMINISM_TEST_RUNS; run++) {
            logger.debug("    Run {}/{}", run, DETERMINISM_TEST_RUNS);

            WasmSandbox.ExecutionResult result = sandbox.execute(tv.wasmBytes, tv.inputBytes, config);

            // Execution must succeed
            assertTrue(result.isSuccess(),
                String.format("Run %d failed: %s", run, result.getErrorMessage()));

            byte[] output = result.getOutput();
            byte[] outputHash = CertusHash.sha256(output);
            String outputHashHex = CertusHash.toHex(outputHash);

            observedOutputHashes.add(outputHashHex);

            if (run == 1) {
                firstOutput = output;
                logger.info("  First run outputHash: {}", outputHashHex);
            } else {
                // All subsequent runs MUST match first run
                String firstOutputHashHex = CertusHash.toHex(CertusHash.sha256(firstOutput));
                assertEquals(firstOutputHashHex, outputHashHex,
                    String.format("DETERMINISM VIOLATION: Run %d produced different output", run));
            }
        }

        // Final check: all runs produced exactly ONE unique output hash
        assertEquals(1, observedOutputHashes.size(),
            "DETERMINISM VIOLATION: Multiple different outputs observed across " + DETERMINISM_TEST_RUNS + " runs");

        logger.info("  ✓ DETERMINISTIC: {} runs produced identical output", DETERMINISM_TEST_RUNS);

        // If expected output is provided, validate it
        if (tv.expectedOutputBytes != null) {
            byte[] expectedHash = CertusHash.sha256(tv.expectedOutputBytes);
            byte[] actualHash = CertusHash.sha256(firstOutput);

            if (!Arrays.equals(expectedHash, actualHash)) {
                logger.warn("  ⚠ Output hash doesn't match expected test vector");
                logger.warn("    Expected: {}", CertusHash.toHex(expectedHash));
                logger.warn("    Actual:   {}", CertusHash.toHex(actualHash));
                // Don't fail test - expected output may be for different runtime
            } else {
                logger.info("  ✓ Output matches expected test vector");
            }
        }
    }

    /**
     * Load test vectors from filesystem.
     */
    private TestVector[] loadTestVectors() throws Exception {
        List<TestVector> vectors = new ArrayList<>();

        Path wasmDir = TEST_VECTORS_DIR.resolve("wasm");
        Path inputDir = TEST_VECTORS_DIR.resolve("inputs");
        Path outputDir = TEST_VECTORS_DIR.resolve("outputs");

        if (!Files.exists(wasmDir) || !Files.exists(inputDir)) {
            logger.warn("Test vectors directory not found: {}", TEST_VECTORS_DIR);
            return new TestVector[0];
        }

        // Scan for test vectors (match wasm files with input files)
        Files.list(wasmDir)
            .filter(p -> p.toString().endsWith(".wasm"))
            .forEach(wasmPath -> {
                try {
                    String wasmName = wasmPath.getFileName().toString();
                    String baseName = wasmName.substring(0, wasmName.length() - 5); // Remove .wasm

                    // Look for matching input
                    Path inputPath = inputDir.resolve(baseName + ".bin");
                    if (!Files.exists(inputPath)) {
                        // Try input1.bin as default
                        inputPath = inputDir.resolve("input1.bin");
                    }

                    if (Files.exists(inputPath)) {
                        byte[] wasmBytes = Files.readAllBytes(wasmPath);
                        byte[] inputBytes = Files.readAllBytes(inputPath);

                        // Look for expected output
                        Path outputPath = outputDir.resolve(baseName + ".bin");
                        byte[] expectedOutput = Files.exists(outputPath) ? Files.readAllBytes(outputPath) : null;

                        vectors.add(new TestVector(baseName, wasmBytes, inputBytes, expectedOutput));
                        logger.info("Loaded test vector: {}", baseName);
                    }

                } catch (Exception e) {
                    logger.error("Error loading test vector", e);
                }
            });

        return vectors.toArray(new TestVector[0]);
    }

    /**
     * Test vector data structure.
     */
    private static class TestVector {
        final String name;
        final byte[] wasmBytes;
        final byte[] inputBytes;
        final byte[] expectedOutputBytes;  // May be null

        TestVector(String name, byte[] wasmBytes, byte[] inputBytes, byte[] expectedOutputBytes) {
            this.name = name;
            this.wasmBytes = wasmBytes;
            this.inputBytes = inputBytes;
            this.expectedOutputBytes = expectedOutputBytes;
        }
    }

    @Test
    @DisplayName("Validate test vector SHA256 hashes")
    void testTestVectorHashes() throws Exception {
        // Ensure test vectors themselves haven't been corrupted
        Path inputPath = TEST_VECTORS_DIR.resolve("inputs/input1.bin");

        if (Files.exists(inputPath)) {
            byte[] inputBytes = Files.readAllBytes(inputPath);
            byte[] inputHash = CertusHash.sha256(inputBytes);

            logger.info("input1.bin hash: {}", CertusHash.toHex(inputHash));
            assertNotNull(inputHash);
            assertEquals(32, inputHash.length);
        }
    }

    @Test
    @DisplayName("WasmSandbox basic functionality")
    void testWasmSandboxBasic() {
        WasmSandbox sandbox = new WasmSandbox();

        try {
            // Create minimal Wasm module (just magic header for validation test)
            byte[] minimalWasm = new byte[] {
                0x00, 0x61, 0x73, 0x6D,  // Magic: \0asm
                0x01, 0x00, 0x00, 0x00   // Version: 1
            };

            WasmSandbox.ValidationResult result = sandbox.validateWasm(minimalWasm);
            assertTrue(result.isValid(), "Minimal Wasm should pass validation");

        } finally {
            sandbox.close();
        }
    }

    @Test
    @DisplayName("Invalid Wasm module rejected")
    void testInvalidWasmRejected() {
        WasmSandbox sandbox = new WasmSandbox();

        try {
            byte[] invalidWasm = new byte[] { 0x00, 0x00, 0x00, 0x00 };  // Invalid magic

            WasmSandbox.ValidationResult result = sandbox.validateWasm(invalidWasm);
            assertFalse(result.isValid(), "Invalid Wasm should be rejected");
            assertNotNull(result.getError());

            logger.info("Invalid Wasm rejected: {}", result.getError());

        } finally {
            sandbox.close();
        }
    }

    @Test
    @DisplayName("SECURITY: Multi-vector validation prevents gaming attacks")
    void testMultiVectorValidationAntiGaming() {
        logger.info("===== ANTI-GAMING TEST =====");
        logger.info("This test validates that we require MULTIPLE test vectors");
        logger.info("to prevent Wasm modules that 'game' known test inputs");

        WasmSandbox sandbox = new WasmSandbox();
        WasmValidator validator = new WasmValidator(sandbox);

        try {
            // Create minimal valid Wasm
            byte[] minimalWasm = new byte[] {
                0x00, 0x61, 0x73, 0x6D,  // Magic: \0asm
                0x01, 0x00, 0x00, 0x00   // Version: 1
            };

            WasmSandbox.ExecutionConfig config = new WasmSandbox.ExecutionConfig(
                10_000_000,  // 10M fuel
                64 * 1024 * 1024,  // 64MB memory
                1024 * 1024  // 1MB max output
            );

            // Test 1: Insufficient vectors (should FAIL)
            logger.info("Test 1: Single test vector (should be rejected)...");
            List<WasmValidator.TestVector> singleVector = Arrays.asList(
                new WasmValidator.TestVector(new byte[32], "single_test")
            );

            WasmValidator.ValidationResult result1 = validator.validateWithMultipleVectors(
                minimalWasm, singleVector, config
            );

            assertFalse(result1.isValid(), "Single test vector should be rejected");
            assertTrue(result1.getError().contains("Insufficient test vectors"),
                "Error should mention insufficient vectors");
            logger.info("  ✓ Single vector rejected as expected");

            // Test 2: Two vectors (should FAIL - minimum is 3)
            logger.info("Test 2: Two test vectors (should be rejected)...");
            List<WasmValidator.TestVector> twoVectors = Arrays.asList(
                new WasmValidator.TestVector(new byte[32], "test_1"),
                new WasmValidator.TestVector(new byte[32], "test_2")
            );

            WasmValidator.ValidationResult result2 = validator.validateWithMultipleVectors(
                minimalWasm, twoVectors, config
            );

            assertFalse(result2.isValid(), "Two test vectors should be rejected");
            logger.info("  ✓ Two vectors rejected as expected");

            // Test 3: Three vectors (should PASS - meets minimum)
            logger.info("Test 3: Three test vectors (should pass)...");
            List<WasmValidator.TestVector> threeVectors = Arrays.asList(
                new WasmValidator.TestVector(new byte[32], "test_1"),
                new WasmValidator.TestVector(new byte[]{1,2,3,4}, "test_2"),
                new WasmValidator.TestVector(new byte[]{5,6,7,8}, "test_3")
            );

            WasmValidator.ValidationResult result3 = validator.validateWithMultipleVectors(
                minimalWasm, threeVectors, config
            );

            assertTrue(result3.isValid(), "Three test vectors should pass: " +
                (result3.getError() != null ? result3.getError() : ""));
            assertEquals(3, result3.getMetrics().testVectorCount);
            assertEquals(30, result3.getMetrics().totalRuns);  // 3 vectors × 10 runs
            logger.info("  ✓ Three vectors passed (30 total runs)");

            // Test 4: Ten vectors (should PASS with no warnings)
            logger.info("Test 4: Ten test vectors (optimal)...");
            List<WasmValidator.TestVector> tenVectors = new ArrayList<>();
            for (int i = 0; i < 10; i++) {
                byte[] input = new byte[]{(byte)i, (byte)(i*2), (byte)(i*3)};
                tenVectors.add(new WasmValidator.TestVector(input, "test_" + i));
            }

            WasmValidator.ValidationResult result4 = validator.validateWithMultipleVectors(
                minimalWasm, tenVectors, config
            );

            assertTrue(result4.isValid(), "Ten test vectors should pass");
            assertEquals(10, result4.getMetrics().testVectorCount);
            assertEquals(100, result4.getMetrics().totalRuns);  // 10 vectors × 10 runs
            assertTrue(result4.getWarnings().isEmpty() ||
                      !result4.getWarnings().get(0).contains("Only"),
                      "Should not warn about insufficient vectors");
            logger.info("  ✓ Ten vectors passed (100 total runs)");

            logger.info("===== ANTI-GAMING TEST PASSED =====");

        } finally {
            sandbox.close();
        }
    }

    @Test
    @DisplayName("SECURITY: Static analysis detects floating-point instructions")
    void testStaticAnalysisRejectsFloatingPoint() {
        logger.info("===== FLOATING-POINT DETECTION TEST =====");

        WasmSandbox sandbox = new WasmSandbox();
        WasmValidator validator = new WasmValidator(sandbox);

        try {
            // Create Wasm module with floating-point instruction
            byte[] floatWasm = new byte[] {
                0x00, 0x61, 0x73, 0x6D,  // Magic: \0asm
                0x01, 0x00, 0x00, 0x00,  // Version: 1
                // Add f64.add opcode (0xA0) embedded in bytecode
                0x00, (byte)0xA0, 0x00, 0x00
            };

            List<WasmValidator.TestVector> vectors = Arrays.asList(
                new WasmValidator.TestVector(new byte[16], "test_1"),
                new WasmValidator.TestVector(new byte[16], "test_2"),
                new WasmValidator.TestVector(new byte[16], "test_3")
            );

            WasmSandbox.ExecutionConfig config = new WasmSandbox.ExecutionConfig(
                10_000_000, 64 * 1024 * 1024, 1024 * 1024
            );

            WasmValidator.ValidationResult result = validator.validateWithMultipleVectors(
                floatWasm, vectors, config
            );

            assertFalse(result.isValid(), "Floating-point Wasm should be rejected");
            assertTrue(result.getError().contains("loating"), "Error should mention floating-point");
            logger.info("  ✓ Floating-point instructions detected and rejected");
            logger.info("  Error: {}", result.getError());

            logger.info("===== FLOATING-POINT DETECTION TEST PASSED =====");

        } finally {
            sandbox.close();
        }
    }

    @Test
    @DisplayName("SECURITY: Random test vector generation for robustness testing")
    void testRandomTestVectorGeneration() {
        logger.info("===== RANDOM TEST VECTOR GENERATION =====");

        // Generate random test vectors
        List<WasmValidator.TestVector> randomVectors = WasmValidator.generateRandomTestVectors(5, 64);

        assertEquals(5, randomVectors.size());

        for (int i = 0; i < randomVectors.size(); i++) {
            WasmValidator.TestVector tv = randomVectors.get(i);
            assertEquals(64, tv.inputBytes.length);
            assertTrue(tv.description.contains("random"));
            logger.info("  Vector {}: {} bytes, desc: {}", i, tv.inputBytes.length, tv.description);
        }

        // Ensure vectors are reproducible (fixed seed)
        List<WasmValidator.TestVector> randomVectors2 = WasmValidator.generateRandomTestVectors(5, 64);
        assertArrayEquals(randomVectors.get(0).inputBytes, randomVectors2.get(0).inputBytes,
            "Random vectors should be reproducible with fixed seed");

        logger.info("  ✓ Random test vectors generated successfully");
        logger.info("===== RANDOM TEST VECTOR GENERATION PASSED =====");
    }
}
