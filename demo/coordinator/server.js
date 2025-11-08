import express from 'express';
import { WebSocketServer } from 'ws';
import cors from 'cors';
import Database from 'better-sqlite3';
import { v4 as uuidv4 } from 'uuid';
import { createServer } from 'http';
import { sha256Hex, generateJobId } from '../shared/crypto.js';
import { JobState } from '../shared/types.js';

const app = express();
const server = createServer(app);
const wss = new WebSocketServer({ server });

app.use(cors());
app.use(express.json({ limit: '10mb' }));

// SQLite database for job state
const db = new Database('coordinator/data/jobs.db');
db.exec(`
  CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    python_code TEXT NOT NULL,
    wasm_hash TEXT,
    input_hash TEXT,
    output_hash TEXT,
    state TEXT NOT NULL,
    executor_id TEXT,
    executor_sig TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
  )
`);

db.exec(`
  CREATE TABLE IF NOT EXISTS verifications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL,
    verifier_id TEXT NOT NULL,
    output_hash TEXT NOT NULL,
    signature TEXT NOT NULL,
    matches INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(job_id) REFERENCES jobs(id)
  )
`);

db.exec(`
  CREATE TABLE IF NOT EXISTS logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL,
    message TEXT NOT NULL,
    level TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(job_id) REFERENCES jobs(id)
  )
`);

// Connected WebSocket clients (executor, verifiers, frontend)
const connections = {
  executor: null,
  verifiers: new Set(),
  frontends: new Set()
};

/**
 * Broadcast update to all frontend clients
 */
function broadcastToFrontends(message) {
  const payload = JSON.stringify(message);
  connections.frontends.forEach(ws => {
    if (ws.readyState === 1) ws.send(payload);
  });
}

/**
 * Add log entry for a job
 */
function addLog(jobId, message, level = 'info') {
  const stmt = db.prepare('INSERT INTO logs (job_id, message, level, created_at) VALUES (?, ?, ?, ?)');
  stmt.run(jobId, message, level, Date.now());

  broadcastToFrontends({
    type: 'log',
    jobId,
    message,
    level,
    created_at: Date.now()
  });
}

/**
 * Update job state
 */
function updateJobState(jobId, state, additionalData = {}) {
  const updates = ['state = ?', 'updated_at = ?'];
  const values = [state, Date.now()];

  Object.entries(additionalData).forEach(([key, value]) => {
    updates.push(`${key} = ?`);
    values.push(value);
  });

  const sql = `UPDATE jobs SET ${updates.join(', ')} WHERE id = ?`;
  const stmt = db.prepare(sql);
  stmt.run(...values, jobId);

  // Fetch updated job and broadcast
  const job = db.prepare('SELECT * FROM jobs WHERE id = ?').get(jobId);
  broadcastToFrontends({
    type: 'job_update',
    job
  });
}

/**
 * Handle WebSocket connections
 */
wss.on('connection', (ws) => {
  let nodeType = null;
  let nodeId = null;

  ws.on('message', async (data) => {
    const msg = JSON.parse(data);

    // Node registration
    if (msg.type === 'register') {
      nodeType = msg.nodeType;
      nodeId = msg.nodeId;

      if (nodeType === 'executor') {
        connections.executor = ws;
        console.log(`[Coordinator] Executor ${nodeId} connected`);
      } else if (nodeType === 'verifier') {
        connections.verifiers.add(ws);
        console.log(`[Coordinator] Verifier ${nodeId} connected`);
      } else if (nodeType === 'frontend') {
        connections.frontends.add(ws);
        console.log(`[Coordinator] Frontend client connected`);
      }

      ws.send(JSON.stringify({ type: 'registered', nodeId }));
      return;
    }

    // Compilation complete
    if (msg.type === 'compiled') {
      const { jobId, wasmHash, wasmBytes } = msg;

      updateJobState(jobId, JobState.EXECUTING, {
        wasm_hash: wasmHash
      });

      addLog(jobId, `Compiled to Wasm (${wasmBytes.length} bytes)\nSHA256: ${wasmHash}`);

      // Send to executor (include Python code since our CLI needs it)
      const job = db.prepare('SELECT * FROM jobs WHERE id = ?').get(jobId);
      if (connections.executor && job) {
        connections.executor.send(JSON.stringify({
          type: 'execute',
          jobId,
          wasmBytes,
          pythonCode: job.python_code
        }));
      }
      return;
    }

    // Execution complete
    if (msg.type === 'executed') {
      const { jobId, outputHash, signature, stdout } = msg;

      updateJobState(jobId, JobState.VERIFYING, {
        output_hash: outputHash,
        executor_id: nodeId,
        executor_sig: signature
      });

      addLog(jobId, `Execution complete\nOutput SHA256: ${outputHash}`);

      if (stdout && stdout.length > 0) {
        stdout.forEach(line => addLog(jobId, `  ${line}`, 'stdout'));
      }

      // Send to all verifiers
      const job = db.prepare('SELECT * FROM jobs WHERE id = ?').get(jobId);
      connections.verifiers.forEach(verifierWs => {
        if (verifierWs.readyState === 1) {
          verifierWs.send(JSON.stringify({
            type: 'verify',
            jobId,
            wasmHash: job.wasm_hash,
            pythonCode: job.python_code,
            expectedHash: outputHash
          }));
        }
      });
      return;
    }

    // Verification complete
    if (msg.type === 'verified') {
      const { jobId, outputHash, signature, matches } = msg;

      // Store verification
      const stmt = db.prepare(
        'INSERT INTO verifications (job_id, verifier_id, output_hash, signature, matches, created_at) VALUES (?, ?, ?, ?, ?, ?)'
      );
      stmt.run(jobId, nodeId, outputHash, signature, matches ? 1 : 0, Date.now());

      const matchStr = matches ? '✓ MATCH' : '✗ MISMATCH';
      addLog(jobId, `Verifier ${nodeId}: ${matchStr}\nHash: ${outputHash}`);

      // Broadcast verification update immediately
      const verifications = db.prepare('SELECT * FROM verifications WHERE job_id = ?').all(jobId);
      broadcastToFrontends({
        type: 'verification_update',
        jobId,
        verifications
      });

      // Check if we have enough verifications
      if (verifications.length >= 3) {
        const allMatch = verifications.every(v => v.matches === 1);

        if (allMatch) {
          updateJobState(jobId, JobState.VERIFIED);
          addLog(jobId, `Consensus reached: 3/3 verifiers agree`, 'success');
        } else {
          updateJobState(jobId, JobState.FRAUD);
          addLog(jobId, `FRAUD DETECTED: Verifiers disagree`, 'error');
        }
      }
      return;
    }

    // Execution failed
    if (msg.type === 'error') {
      const { jobId, error } = msg;
      updateJobState(jobId, JobState.FAILED);
      addLog(jobId, `Error: ${error}`, 'error');
      return;
    }
  });

  ws.on('close', () => {
    if (nodeType === 'executor' && connections.executor === ws) {
      connections.executor = null;
      console.log('[Coordinator] Executor disconnected');
    } else if (nodeType === 'verifier') {
      connections.verifiers.delete(ws);
      console.log(`[Coordinator] Verifier ${nodeId} disconnected`);
    } else if (nodeType === 'frontend') {
      connections.frontends.delete(ws);
      console.log('[Coordinator] Frontend disconnected');
    }
  });
});

/**
 * Submit new job
 */
app.post('/api/jobs', async (req, res) => {
  const { pythonCode, challengeId } = req.body;

  if (!pythonCode || pythonCode.trim().length === 0) {
    return res.status(400).json({ error: 'Python code is required' });
  }

  // Generate job ID
  const codeHash = sha256Hex(pythonCode);
  const inputHash = sha256Hex(''); // No input for now
  const nonce = uuidv4();
  const jobId = generateJobId(codeHash, inputHash, nonce);

  // Store job
  const stmt = db.prepare(
    'INSERT INTO jobs (id, python_code, input_hash, state, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)'
  );
  const now = Date.now();
  stmt.run(jobId, pythonCode, inputHash, JobState.COMPILING, now, now);

  addLog(jobId, 'Job submitted, compiling to Wasm...');

  // Send to executor for compilation
  if (connections.executor) {
    connections.executor.send(JSON.stringify({
      type: 'compile',
      jobId,
      pythonCode
    }));
  } else {
    updateJobState(jobId, JobState.FAILED);
    addLog(jobId, 'No executor available', 'error');
  }

  res.json({ jobId });
});

/**
 * Get job details
 */
app.get('/api/jobs/:jobId', (req, res) => {
  const { jobId } = req.params;

  const job = db.prepare('SELECT * FROM jobs WHERE id = ?').get(jobId);
  if (!job) {
    return res.status(404).json({ error: 'Job not found' });
  }

  const verifications = db.prepare('SELECT * FROM verifications WHERE job_id = ?').all(jobId);
  const logs = db.prepare('SELECT * FROM logs WHERE job_id = ? ORDER BY created_at ASC').all(jobId);

  res.json({
    job,
    verifications,
    logs
  });
});

/**
 * Get recent jobs
 */
app.get('/api/jobs', (req, res) => {
  const jobs = db.prepare('SELECT * FROM jobs ORDER BY created_at DESC LIMIT 20').all();
  res.json(jobs);
});

/**
 * Get network stats
 */
app.get('/api/stats', (req, res) => {
  const totalJobs = db.prepare('SELECT COUNT(*) as count FROM jobs').get().count;
  const verifiedJobs = db.prepare('SELECT COUNT(*) as count FROM jobs WHERE state = ?').get(JobState.VERIFIED).count;
  const fraudJobs = db.prepare('SELECT COUNT(*) as count FROM jobs WHERE state = ?').get(JobState.FRAUD).count;

  res.json({
    totalJobs,
    verifiedJobs,
    fraudJobs,
    executors: connections.executor ? 1 : 0,
    verifiers: connections.verifiers.size
  });
});

const PORT = process.env.PORT || 4000;
server.listen(PORT, () => {
  console.log(`[Coordinator] Server running on http://localhost:${PORT}`);
  console.log(`[Coordinator] WebSocket server ready`);
});
