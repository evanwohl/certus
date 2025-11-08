import WebSocket from 'ws';
import { spawn } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';
import { sha256Hex, generateKeypair, sign } from '../shared/crypto.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const EXECUTOR_ID = process.env.EXECUTOR_ID || 'executor-primary';
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
    console.log(`[${EXECUTOR_ID}] Connected to coordinator`);

    // Register as executor
    ws.send(JSON.stringify({
      type: 'register',
      nodeType: 'executor',
      nodeId: EXECUTOR_ID
    }));
  });

  ws.on('message', async (data) => {
    const msg = JSON.parse(data);

    if (msg.type === 'registered') {
      console.log(`[${EXECUTOR_ID}] Registered with coordinator`);
      return;
    }

    if (msg.type === 'compile') {
      await handleCompile(msg);
      return;
    }

    if (msg.type === 'execute') {
      await handleExecute(msg);
      return;
    }
  });

  ws.on('close', () => {
    console.log(`[${EXECUTOR_ID}] Disconnected from coordinator, reconnecting...`);
    setTimeout(connect, 2000);
  });

  ws.on('error', (err) => {
    console.error(`[${EXECUTOR_ID}] WebSocket error:`, err.message);
  });
}

/**
 * Compile Python to Wasm
 */
async function handleCompile(msg) {
  const { jobId, pythonCode } = msg;

  console.log(`[${EXECUTOR_ID}] Compiling job ${jobId.slice(0, 8)}...`);

  try {
    // Call python-verifier CLI to compile
    const result = await runPythonVerifier('compile', pythonCode);

    if (result.error) {
      ws.send(JSON.stringify({
        type: 'error',
        jobId,
        error: result.error
      }));
      return;
    }

    const wasmBytes = Buffer.from(result.wasm, 'base64');
    const wasmHash = sha256Hex(wasmBytes);

    ws.send(JSON.stringify({
      type: 'compiled',
      jobId,
      wasmHash,
      wasmBytes: wasmBytes.toString('base64')
    }));

    console.log(`[${EXECUTOR_ID}] Compiled successfully (hash: ${wasmHash.slice(0, 12)}...)`);

  } catch (error) {
    console.error(`[${EXECUTOR_ID}] Compilation error:`, error.message);
    ws.send(JSON.stringify({
      type: 'error',
      jobId,
      error: error.message
    }));
  }
}

/**
 * Execute Wasm (passes Python code since our CLI needs it)
 */
async function handleExecute(msg) {
  const { jobId, pythonCode } = msg;

  console.log(`[${EXECUTOR_ID}] Executing job ${jobId.slice(0, 8)}...`);

  try {
    // Call python-cli CLI to execute (it takes Python code, not Wasm)
    const result = await runPythonVerifier('execute', pythonCode);

    if (result.error) {
      ws.send(JSON.stringify({
        type: 'error',
        jobId,
        error: result.error
      }));
      return;
    }

    // Use the output_hash from the result (it's already computed)
    const outputHash = result.output_hash || sha256Hex(result.output || '');

    // Sign the output hash
    const signature = await sign(outputHash, keypair.privateKey);

    ws.send(JSON.stringify({
      type: 'executed',
      jobId,
      outputHash,
      signature,
      stdout: result.stdout || []
    }));

    console.log(`[${EXECUTOR_ID}] Executed successfully (output: ${outputHash.slice(0, 12)}...)`);

  } catch (error) {
    console.error(`[${EXECUTOR_ID}] Execution error:`, error.message);
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

// Start executor
connect();
