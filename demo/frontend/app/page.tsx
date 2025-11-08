'use client';

import { useState, useEffect, useRef } from 'react';
import styles from './page.module.css';

const API_URL = 'http://localhost:4000';
const WS_URL = 'ws://localhost:4000';

const EXAMPLES = {
  fibonacci: `# Deterministic Fibonacci computation
# Compiled to Wasm, executed remotely, verified by 3 nodes
n = 20
a = 0
b = 1
for _ in range(n):
    temp = a + b
    a = b
    b = temp
OUTPUT = a`,

  sha256: `# Cryptographic proof-of-work
# Hash grinding with deterministic execution
import hashlib

nonce = 0
target = "0000"
found = 0

while nonce < 100000:
    data = f"certus-{nonce}".encode()
    hash_result = hashlib.sha256(data).hexdigest()

    if hash_result.startswith(target):
        found = nonce
        break

    nonce += 1

OUTPUT = found`,

  prime: `# Computational challenge: Semiprime factorization
# Result cryptographically signed by executor
n = 8633
factor = 0

sqrt_n = 1
while sqrt_n * sqrt_n < n:
    sqrt_n += 1

for i in range(2, sqrt_n + 1):
    if n % i == 0:
        factor = i
        break

OUTPUT = factor`
};

interface Job {
  id: string;
  state: string;
  wasm_hash?: string;
  output_hash?: string;
}

interface Log {
  message: string;
  level: string;
  created_at: number;
}

interface Verification {
  verifier_id: string;
  matches: number;
  output_hash: string;
}

export default function Home() {
  const [code, setCode] = useState(EXAMPLES.fibonacci);
  const [jobId, setJobId] = useState<string | null>(null);
  const [job, setJob] = useState<Job | null>(null);
  const [logs, setLogs] = useState<Log[]>([]);
  const [verifications, setVerifications] = useState<Verification[]>([]);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [stats, setStats] = useState({ totalJobs: 0, verifiedJobs: 0, executors: 0, verifiers: 0 });
  const [isLoadingStats, setIsLoadingStats] = useState(true);

  const wsRef = useRef<WebSocket | null>(null);
  const logsEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Connect to WebSocket for real-time updates
  useEffect(() => {
    const ws = new WebSocket(WS_URL);
    wsRef.current = ws;

    ws.onopen = () => {
      ws.send(JSON.stringify({
        type: 'register',
        nodeType: 'frontend',
        nodeId: 'frontend-' + Math.random().toString(36).slice(2, 9)
      }));
    };

    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);

      if (msg.type === 'job_update' && msg.job.id === jobId) {
        setJob(msg.job);
      }

      if (msg.type === 'log' && msg.jobId === jobId) {
        setLogs(prev => [...prev, msg]);
      }
    };

    ws.onerror = (err) => {
      console.error('WebSocket error:', err);
    };

    return () => {
      ws.close();
    };
  }, [jobId]);

  // Fetch stats periodically
  useEffect(() => {
    const fetchStats = async () => {
      try {
        const res = await fetch(`${API_URL}/api/stats`);
        const data = await res.json();
        setStats(data);
        setIsLoadingStats(false);
      } catch (err) {
        console.error('Failed to fetch stats:', err);
        setIsLoadingStats(false);
      }
    };

    fetchStats();
    const interval = setInterval(fetchStats, 5000);
    return () => clearInterval(interval);
  }, []);

  // Fetch job details when jobId changes
  useEffect(() => {
    if (!jobId) return;

    const fetchJob = async () => {
      try {
        const res = await fetch(`${API_URL}/api/jobs/${jobId}`);
        const data = await res.json();
        setJob(data.job);
        setLogs(data.logs);
        setVerifications(data.verifications);
      } catch (err) {
        console.error('Failed to fetch job:', err);
      }
    };

    fetchJob();
    const interval = setInterval(fetchJob, 2000);
    return () => clearInterval(interval);
  }, [jobId]);

  // Auto-scroll logs
  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  // Handle tab key in textarea
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Tab') {
      e.preventDefault();
      const target = e.currentTarget;
      const start = target.selectionStart;
      const end = target.selectionEnd;
      const newValue = code.substring(0, start) + '  ' + code.substring(end);
      setCode(newValue);
      setTimeout(() => {
        target.selectionStart = target.selectionEnd = start + 2;
      }, 0);
    }
  };

  const handleSubmit = async () => {
    if (!code.trim()) return;

    setIsSubmitting(true);
    setJobId(null);
    setJob(null);
    setLogs([]);
    setVerifications([]);

    try {
      const res = await fetch(`${API_URL}/api/jobs`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ pythonCode: code })
      });

      const data = await res.json();
      setJobId(data.jobId);
    } catch (err) {
      console.error('Failed to submit job:', err);
      alert('Failed to submit job. Make sure the coordinator is running.');
    } finally {
      setIsSubmitting(false);
    }
  };

  const loadExample = (exampleKey: keyof typeof EXAMPLES) => {
    setCode(EXAMPLES[exampleKey]);
  };

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
        e.preventDefault();
        if (!isSubmitting && code.trim()) {
          handleSubmit();
        }
      }
      if ((e.metaKey || e.ctrlKey) && e.key === '1') {
        e.preventDefault();
        loadExample('fibonacci');
      }
      if ((e.metaKey || e.ctrlKey) && e.key === '2') {
        e.preventDefault();
        loadExample('sha256');
      }
      if ((e.metaKey || e.ctrlKey) && e.key === '3') {
        e.preventDefault();
        loadExample('prime');
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isSubmitting, code]);

  return (
    <div className={styles.container}>
      {/* Header */}
      <header className={styles.header}>
        <div className={styles.headerContent}>
          <div>
            <a href="https://certuscompute.com" target="_blank" rel="noopener noreferrer" className={styles.titleLink}>
              <h1 className={styles.title}>
                CERTUS<span className={styles.titleAccent}>.</span>
              </h1>
            </a>
            <p className={styles.subtitle}>TRUSTLESS DETERMINISTIC COMPUTE</p>
          </div>

          <div className={styles.stats}>
            <div className={styles.statItem}>
              <span className={styles.statLabel}>Jobs</span>
              <span className={`${styles.statValue} ${isLoadingStats ? styles.loading : ''}`}>
                {isLoadingStats ? '—' : stats.totalJobs}
              </span>
            </div>
            <div className={styles.statItem}>
              <span className={styles.statLabel}>Verified</span>
              <span
                className={`${styles.statValue} ${isLoadingStats ? styles.loading : ''}`}
                style={{ color: isLoadingStats ? 'var(--deep-black)' : 'var(--success)' }}
              >
                {isLoadingStats ? '—' : stats.verifiedJobs}
              </span>
            </div>
            <div className={styles.statItem}>
              <span className={styles.statLabel}>Executors</span>
              <span className={`${styles.statValue} ${isLoadingStats ? styles.loading : ''}`}>
                {isLoadingStats ? '—' : stats.executors}
              </span>
            </div>
            <div className={styles.statItem}>
              <span className={styles.statLabel}>Verifiers</span>
              <span className={`${styles.statValue} ${isLoadingStats ? styles.loading : ''}`}>
                {isLoadingStats ? '—' : stats.verifiers}
              </span>
            </div>
          </div>
        </div>
      </header>

      {/* Main content */}
      <main className={styles.main}>
        {/* Left: Code editor */}
        <div className={styles.editorPanel}>
          <div className={styles.panelHeader}>
            <h2>Python Code</h2>
            <div className={styles.examples}>
              <button
                onClick={() => loadExample('fibonacci')}
                className={styles.exampleBtn}
                title="Load Fibonacci example (Ctrl/Cmd + 1)"
              >
                Fibonacci
              </button>
              <button
                onClick={() => loadExample('sha256')}
                className={styles.exampleBtn}
                title="Load SHA-256 example (Ctrl/Cmd + 2)"
              >
                Proof-of-Work
              </button>
              <button
                onClick={() => loadExample('prime')}
                className={styles.exampleBtn}
                title="Load Prime Factorization example (Ctrl/Cmd + 3)"
              >
                Factorization
              </button>
            </div>
          </div>

          <div className={styles.editorWrapper}>
            <textarea
              ref={textareaRef}
              className={styles.codeEditor}
              value={code}
              onChange={(e) => setCode(e.target.value)}
              onKeyDown={handleKeyDown}
              spellCheck={false}
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
            />
          </div>

          <button
            onClick={handleSubmit}
            disabled={isSubmitting || !code.trim()}
            className={styles.submitBtn}
            title="Submit job (Ctrl/Cmd + Enter)"
          >
            {isSubmitting ? 'Submitting...' : 'Execute & Verify'}
          </button>
        </div>

        {/* Right: Execution theatre */}
        <div className={styles.theatrePanel}>
          <div className={styles.panelHeader}>
            <h2>Execution Theatre</h2>
            {jobId && (
              <span className={styles.jobId}>
                Job: {jobId.slice(0, 8)}...
              </span>
            )}
          </div>

          {!jobId ? (
            <div className={styles.placeholder}>
              <div className={styles.placeholderIcon}>■</div>
              <div className={styles.placeholderContent}>
                <h3>Live Verification Protocol</h3>
                <ul>
                  <li>Python → Deterministic Wasm compilation</li>
                  <li>Remote execution with SHA-256 output hash</li>
                  <li>Ed25519 cryptographic signatures</li>
                  <li>3-node consensus verification</li>
                  <li>Fraud detection via hash comparison</li>
                </ul>
                <p className={styles.placeholderNote}>
                  Submit code to observe real-time cryptographic verification
                </p>
              </div>
            </div>
          ) : (
            <div className={styles.theatre}>
              {/* Status indicator */}
              <div className={styles.statusBar}>
                <div className={`${styles.statusBadge} ${styles[job?.state || 'queued']}`}>
                  {job?.state?.toUpperCase() || 'QUEUED'}
                </div>

                {job?.wasm_hash && (
                  <div className={styles.hashDisplay}>
                    <span className={styles.hashLabel}>Wasm:</span>
                    <code>{job.wasm_hash.slice(0, 12)}...</code>
                  </div>
                )}

                {job?.output_hash && (
                  <div className={styles.hashDisplay}>
                    <span className={styles.hashLabel}>Output:</span>
                    <code>{job.output_hash.slice(0, 12)}...</code>
                  </div>
                )}
              </div>

              {/* Verification progress */}
              {verifications.length > 0 && (
                <div
                  className={styles.verifications}
                  style={{
                    '--progress': `${(verifications.length / 3) * 100}%`
                  } as React.CSSProperties}
                >
                  <h3>Verifiers ({verifications.length}/3)</h3>
                  {verifications.map((v, i) => (
                    <div key={i} className={styles.verification}>
                      <span className={styles.verifierId}>{v.verifier_id}</span>
                      <span className={v.matches ? styles.match : styles.mismatch}>
                        {v.matches ? '✓ MATCH' : '✗ MISMATCH'}
                      </span>
                      <code className={styles.verificationHash}>
                        {v.output_hash.slice(0, 12)}...
                      </code>
                    </div>
                  ))}
                </div>
              )}

              {/* Logs */}
              <div className={styles.logs}>
                {logs.map((log, i) => (
                  <div
                    key={i}
                    className={`${styles.logEntry} ${styles[log.level]}`}
                  >
                    <span className={styles.logTimestamp}>
                      {new Date(log.created_at).toLocaleTimeString()}
                    </span>
                    <span className={styles.logMessage}>{log.message}</span>
                  </div>
                ))}
                <div ref={logsEndRef} />
              </div>
            </div>
          )}
        </div>
      </main>

      {/* Footer */}
      <footer className={styles.footer}>
        <p>
          <span className={styles.footerLabel}>TECH DEMO:</span>
          Real cryptography (Ed25519, SHA-256), real Wasm execution, real multi-verifier consensus. Coordinator centralized for demonstration purposes.
        </p>
      </footer>
    </div>
  );
}
