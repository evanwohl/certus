import WebSocket from 'ws';
import { spawn } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';
import { sha256Hex, generateKeypair, sign } from '../shared/crypto.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const VERIFIER_ID = process.env.VERIFIER_ID || 'verifier-1';
const COORDINATOR_URL = process.env.COORDINATOR_URL || 'ws://localhost:4000';

// Path to python-cli binary (built in workspace root target dir)
const PYTHON_CLI = process.platform === 'win32'
  ? path.resolve(__dirname, '../../target/release/python-cli.exe')
  : path.resolve(__dirname, '../../target/release/python-cli');

let ws = null;
let keypair = null;

/**
 * Connect to coordinator
 */
async function connect() {
  keypair = await generateKeypair();

  ws = new WebSocket(COORDINATOR_URL);

  ws.on('open', () => {
    console.log(`[${VERIFIER_ID}] Connected to coordinator`);

    // Register as verifier
    ws.send(JSON.stringify({
      type: 'register',
      nodeType: 'verifier',
      nodeId: VERIFIER_ID
    }));
  });

  ws.on('message', async (data) => {
    const msg = JSON.parse(data);

    if (msg.type === 'registered') {
      console.log(`[${VERIFIER_ID}] Registered with coordinator`);
      return;
    }

    if (msg.type === 'verify') {
      await handleVerify(msg);
      return;
    }
  });

  ws.on('close', () => {
    console.log(`[${VERIFIER_ID}] Disconnected from coordinator, reconnecting...`);
    setTimeout(connect, 2000);
  });

  ws.on('error', (err) => {
    console.error(`[${VERIFIER_ID}] WebSocket error:`, err.message);
  });
}

/**
 * Re-execute and verify result
 */
async function handleVerify(msg) {
  const { jobId, pythonCode, expectedHash } = msg;

  console.log(`[${VERIFIER_ID}] Verifying job ${jobId.slice(0, 8)}...`);

  try {
    // Execute Python directly (our CLI does both compile and execute)
    const execResult = await runPythonVerifier('execute', pythonCode);

    if (execResult.error) {
      ws.send(JSON.stringify({
        type: 'error',
        jobId,
        error: execResult.error
      }));
      return;
    }

    // Use the output_hash from the result (it's already computed)
    const outputHash = execResult.output_hash || sha256Hex(execResult.output || '');
    const matches = outputHash === expectedHash;

    // Sign the output hash
    const signature = await sign(outputHash, keypair.privateKey);

    ws.send(JSON.stringify({
      type: 'verified',
      jobId,
      outputHash,
      signature,
      matches
    }));

    const status = matches ? '✓ MATCH' : '✗ MISMATCH';
    console.log(`[${VERIFIER_ID}] ${status} (expected: ${expectedHash.slice(0, 12)}..., got: ${outputHash.slice(0, 12)}...)`);

  } catch (error) {
    console.error(`[${VERIFIER_ID}] Verification error:`, error.message);
    ws.send(JSON.stringify({
      type: 'error',
      jobId,
      error: error.message
    }));
  }
}

/**
 * Run python-cli command
 */
function runPythonVerifier(command, input) {
  return new Promise((resolve, reject) => {
    const proc = spawn(PYTHON_CLI, [command], {
      stdio: ['pipe', 'pipe', 'pipe']
    });

    let stdout = '';
    let stderr = '';

    proc.stdout.on('data', (data) => {
      stdout += data.toString();
    });

    proc.stderr.on('data', (data) => {
      stderr += data.toString();
    });

    // Write input (Python code or Wasm bytes)
    if (typeof input === 'string') {
      proc.stdin.write(input);
    } else {
      proc.stdin.write(input);
    }
    proc.stdin.end();

    proc.on('close', (code) => {
      if (code !== 0) {
        reject(new Error(stderr || 'Command failed'));
        return;
      }

      try {
        const result = JSON.parse(stdout);
        resolve(result);
      } catch (err) {
        reject(new Error('Failed to parse output: ' + stdout));
      }
    });

    proc.on('error', (err) => {
      reject(err);
    });
  });
}

// Start verifier
connect();
