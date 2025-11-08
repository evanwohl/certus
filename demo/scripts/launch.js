import { spawn } from 'child_process';
import chalk from 'chalk';
import path from 'path';
import { fileURLToPath } from 'url';
import fs from 'fs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Ensure data directories exist
const dataDir = path.resolve(__dirname, '../coordinator/data');
if (!fs.existsSync(dataDir)) {
  fs.mkdirSync(dataDir, { recursive: true });
}

const processes = [];

function log(name, color, message) {
  const timestamp = new Date().toLocaleTimeString();
  const prefix = chalk[color].bold(`[${name}]`);
  console.log(`${chalk.gray(timestamp)} ${prefix} ${message}`);
}

function spawnProcess(name, color, command, args, options = {}) {
  log(name, color, `Starting...`);

  const defaultCwd = path.resolve(__dirname, '..');
  const proc = spawn(command, args, {
    env: { ...process.env, ...options.env },
    cwd: options.cwd || defaultCwd,
    shell: true
  });

  proc.stdout.on('data', (data) => {
    const lines = data.toString().trim().split('\n');
    lines.forEach(line => {
      if (line.trim()) log(name, color, line);
    });
  });

  proc.stderr.on('data', (data) => {
    const lines = data.toString().trim().split('\n');
    lines.forEach(line => {
      if (line.trim()) log(name, color, chalk.red(line));
    });
  });

  proc.on('close', (code) => {
    if (code !== 0) {
      log(name, color, chalk.red(`Exited with code ${code}`));
    }
  });

  processes.push({ name, proc });
  return proc;
}

// Banner
console.log(chalk.cyan.bold(`
╔═══════════════════════════════════════════════════════╗
║                                                       ║
║              CERTUS DEMO LAUNCHER                     ║
║                                                       ║
║         Trustless Compute Verification                ║
║                                                       ║
╚═══════════════════════════════════════════════════════╝
`));

console.log(chalk.gray('Starting all services...\n'));

// Check if python-cli is built
const pyCli = path.resolve(__dirname, '../python-cli/target/release/python-cli');
const pyCliExists = fs.existsSync(pyCli) || fs.existsSync(pyCli + '.exe');

if (!pyCliExists) {
  console.log(chalk.yellow('python-cli not found. Building now...\n'));

  const buildProc = spawn('cargo', ['build', '--release'], {
    cwd: path.resolve(__dirname, '../python-cli'),
    shell: true,
    stdio: 'inherit'
  });

  buildProc.on('close', (code) => {
    if (code !== 0) {
      console.log(chalk.red('\nBuild failed. Make sure Rust is installed and python-verifier library is built.'));
      console.log(chalk.yellow('Try running: cd python-verifier && cargo build --release'));
      process.exit(1);
    }

    console.log(chalk.green('\nBuild complete!\n'));
    startServices();
  });
} else {
  startServices();
}

function startServices() {
  // 1. Coordinator
  spawnProcess('Coordinator', 'cyan', 'node', ['coordinator/server.js'], {});

  // Wait a bit for coordinator to start
  setTimeout(() => {
    // 2. Executor
    spawnProcess('Executor', 'green', 'node', ['executor/node.js'], {
      env: { EXECUTOR_ID: 'executor-primary' }
    });

    // 3. Verifiers (3 instances)
    setTimeout(() => {
      spawnProcess('Verifier-1', 'magenta', 'node', ['verifier/node.js'], {
        env: { VERIFIER_ID: 'verifier-nyc' }
      });

      spawnProcess('Verifier-2', 'magenta', 'node', ['verifier/node.js'], {
        env: { VERIFIER_ID: 'verifier-berlin' }
      });

      spawnProcess('Verifier-3', 'magenta', 'node', ['verifier/node.js'], {
        env: { VERIFIER_ID: 'verifier-tokyo' }
      });
    }, 1000);

    // 4. Frontend
    setTimeout(() => {
      spawnProcess('Frontend', 'blue', 'npm', ['run', 'dev'], {
        cwd: path.resolve(__dirname, '../frontend')
      });
    }, 2000);
  }, 1500);

  // After all services start
  setTimeout(() => {
    console.log(chalk.green.bold(`
✓ All services started!

Open your browser:
  → ${chalk.cyan('http://localhost:3000')}

API Endpoint:
  → ${chalk.gray('http://localhost:4000')}

Press ${chalk.red('Ctrl+C')} to stop all services.
`));
  }, 4000);
}

// Cleanup on exit
process.on('SIGINT', () => {
  console.log(chalk.yellow('\n\nShutting down all services...\n'));

  processes.forEach(({ name, proc }) => {
    log(name, 'gray', 'Stopping...');
    proc.kill();
  });

  setTimeout(() => {
    console.log(chalk.green('All services stopped.\n'));
    process.exit(0);
  }, 1000);
});
