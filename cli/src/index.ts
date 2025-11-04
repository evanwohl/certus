#!/usr/bin/env node

import { Command } from 'commander';
import { ethers } from 'ethers';
import chalk from 'chalk';
import ora from 'ora';
import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';

/**
 * Certus CLI
 *
 * Commands:
 * - certus wasm register <file>
 * - certus job create --wasm <file> --input <file> --payAmt <amount> --token <address>
 * - certus job finalize --jobId <id>
 * - certus exec accept --jobId <id> --deposit <amount>
 * - certus exec submit --jobId <id> --output <file> --sig <signature>
 * - certus fraud submit --jobId <id> --wasm <file> --input <file> --claimedOutput <file>
 */

const program = new Command();

program
  .name('certus')
  .description('Certus CLI for deterministic verifiable compute')
  .version('1.0.0-MVP');

// Configuration
const DEFAULT_RPC_URL = process.env.RPC_URL || 'https://sepolia-rollup.arbitrum.io/rpc';
const DEFAULT_CONTRACT = process.env.CERTUS_CONTRACT || '';
const DEFAULT_PRIVATE_KEY = process.env.PRIVATE_KEY || '';

/**
 * Helper: Load contract ABI (simplified for MVP)
 */
function getContract(signer: ethers.Signer): ethers.Contract {
  const abi = [
    "function registerWasm(bytes calldata wasm) external returns (bytes32)",
    "function wasmOf(bytes32 wasmHash) external view returns (bytes memory)",
    "function createJob(bytes32 jobId, address payToken, uint256 payAmt, bytes32 wasmHash, bytes32 inputHash, uint64 acceptDeadline, uint64 finalizeDeadline, uint64 fuelLimit, uint64 memLimit, uint32 maxOutputSize) external",
    "function acceptJob(bytes32 jobId) external",
    "function submitReceipt(bytes32 jobId, bytes32 outputHash, bytes calldata execSig) external",
    "function finalize(bytes32 jobId) external",
    "function claimTimeout(bytes32 jobId) external",
    "function fraudOnChain(bytes32 jobId, bytes calldata wasm, bytes calldata input, bytes calldata claimedOutput) external",
    "function getJob(bytes32 jobId) external view returns (tuple(bytes32 jobId, address client, address executor, address payToken, uint256 payAmt, uint256 clientDeposit, uint256 executorDeposit, bytes32 wasmHash, bytes32 inputHash, bytes32 outputHash, uint64 acceptDeadline, uint64 finalizeDeadline, uint64 fuelLimit, uint64 memLimit, uint32 maxOutputSize, uint8 status))"
  ];

  return new ethers.Contract(DEFAULT_CONTRACT, abi, signer);
}

/**
 * Helper: Compute SHA256 hash
 */
function sha256(data: Buffer): string {
  return '0x' + crypto.createHash('sha256').update(data).digest('hex');
}

/**
 * Helper: Get provider and signer
 */
function getSignerAndProvider(): { provider: ethers.JsonRpcProvider, signer: ethers.Wallet } {
  const provider = new ethers.JsonRpcProvider(DEFAULT_RPC_URL);
  const signer = new ethers.Wallet(DEFAULT_PRIVATE_KEY, provider);
  return { provider, signer };
}

// Wasm Commands

const wasmCmd = program.command('wasm').description('Wasm module operations');

wasmCmd
  .command('register')
  .description('Register a Wasm module on-chain')
  .argument('<file>', 'Path to Wasm file')
  .action(async (file: string) => {
    const spinner = ora('Registering Wasm module...').start();

    try {
      if (!fs.existsSync(file)) {
        throw new Error(`File not found: ${file}`);
      }

      const wasmBytes = fs.readFileSync(file);
      const wasmHash = sha256(wasmBytes);

      spinner.text = 'Connecting to Arbitrum...';
      const { signer } = getSignerAndProvider();
      const contract = getContract(signer);

      spinner.text = 'Submitting transaction...';
      const tx = await contract.registerWasm(wasmBytes);

      spinner.text = 'Waiting for confirmation...';
      await tx.wait();

      spinner.succeed(chalk.green('Wasm registered successfully!'));
      console.log(chalk.cyan('Wasm hash:'), wasmHash);
      console.log(chalk.cyan('Size:'), wasmBytes.length, 'bytes');
      console.log(chalk.cyan('Tx hash:'), tx.hash);

    } catch (error: any) {
      spinner.fail(chalk.red('Failed to register Wasm'));
      console.error(chalk.red(error.message));
      process.exit(1);
    }
  });

// Job Commands

const jobCmd = program.command('job').description('Job management operations');

jobCmd
  .command('create')
  .description('Create a new compute job')
  .requiredOption('--wasm <file>', 'Path to Wasm module file')
  .requiredOption('--input <file>', 'Path to input file')
  .requiredOption('--payAmt <amount>', 'Payment amount (in token smallest units)')
  .requiredOption('--token <address>', 'ERC20 token address (USDC/USDT/DAI)')
  .option('--fuelLimit <amount>', 'Wasm fuel limit', '10000000')
  .option('--memLimit <amount>', 'Memory limit in bytes', '67108864')
  .option('--maxOutputSize <amount>', 'Max output size in bytes', '1048576')
  .action(async (options) => {
    const spinner = ora('Creating job...').start();

    try {
      // Read files
      const wasmBytes = fs.readFileSync(options.wasm);
      const inputBytes = fs.readFileSync(options.input);

      const wasmHash = sha256(wasmBytes);
      const inputHash = sha256(inputBytes);

      // Generate jobId (simplified - should include client pubkey + nonce)
      const jobIdData = Buffer.concat([
        Buffer.from(wasmHash.slice(2), 'hex'),
        Buffer.from(inputHash.slice(2), 'hex'),
        Buffer.alloc(32), // client pubkey placeholder
        Buffer.alloc(8)   // nonce placeholder
      ]);
      const jobId = sha256(jobIdData);

      spinner.text = 'Connecting to Arbitrum...';
      const { signer } = getSignerAndProvider();
      const contract = getContract(signer);

      spinner.text = 'Creating job on-chain...';

      const now = Math.floor(Date.now() / 1000);
      const acceptDeadline = now + 120; // 2 minutes
      const finalizeDeadline = acceptDeadline + 300; // 5 minutes after accept

      const tx = await contract.createJob(
        jobId,
        options.token,
        ethers.parseUnits(options.payAmt, 0),
        wasmHash,
        inputHash,
        acceptDeadline,
        finalizeDeadline,
        parseInt(options.fuelLimit),
        parseInt(options.memLimit),
        parseInt(options.maxOutputSize)
      );

      spinner.text = 'Waiting for confirmation...';
      await tx.wait();

      spinner.succeed(chalk.green('Job created successfully!'));
      console.log(chalk.cyan('Job ID:'), jobId);
      console.log(chalk.cyan('Wasm hash:'), wasmHash);
      console.log(chalk.cyan('Input hash:'), inputHash);
      console.log(chalk.cyan('Payment:'), options.payAmt);
      console.log(chalk.cyan('Tx hash:'), tx.hash);

    } catch (error: any) {
      spinner.fail(chalk.red('Failed to create job'));
      console.error(chalk.red(error.message));
      process.exit(1);
    }
  });

jobCmd
  .command('finalize')
  .description('Finalize a job (client)')
  .requiredOption('--jobId <id>', 'Job ID (hex)')
  .action(async (options) => {
    const spinner = ora('Finalizing job...').start();

    try {
      const { signer } = getSignerAndProvider();
      const contract = getContract(signer);

      const tx = await contract.finalize(options.jobId);
      await tx.wait();

      spinner.succeed(chalk.green('Job finalized successfully!'));
      console.log(chalk.cyan('Tx hash:'), tx.hash);

    } catch (error: any) {
      spinner.fail(chalk.red('Failed to finalize job'));
      console.error(chalk.red(error.message));
      process.exit(1);
    }
  });

jobCmd
  .command('get')
  .description('Get job details')
  .requiredOption('--jobId <id>', 'Job ID (hex)')
  .action(async (options) => {
    try {
      const { provider } = getSignerAndProvider();
      const contract = getContract(provider as any);

      const job = await contract.getJob(options.jobId);

      console.log(chalk.cyan('\nJob Details:'));
      console.log('Job ID:', job.jobId);
      console.log('Client:', job.client);
      console.log('Executor:', job.executor);
      console.log('Pay Amount:', ethers.formatUnits(job.payAmt, 6), 'USDC');
      console.log('Status:', ['Created', 'Accepted', 'Receipt', 'Finalized', 'Slashed', 'Cancelled'][job.status]);

    } catch (error: any) {
      console.error(chalk.red('Failed to get job'));
      console.error(chalk.red(error.message));
      process.exit(1);
    }
  });

// Executor Commands

const execCmd = program.command('exec').description('Executor operations');

execCmd
  .command('accept')
  .description('Accept a job (executor)')
  .requiredOption('--jobId <id>', 'Job ID (hex)')
  .requiredOption('--deposit <amount>', 'Executor deposit amount')
  .action(async (options) => {
    const spinner = ora('Accepting job...').start();

    try {
      const { signer } = getSignerAndProvider();
      const contract = getContract(signer);

      const tx = await contract.acceptJob(options.jobId);
      await tx.wait();

      spinner.succeed(chalk.green('Job accepted successfully!'));
      console.log(chalk.cyan('Tx hash:'), tx.hash);

    } catch (error: any) {
      spinner.fail(chalk.red('Failed to accept job'));
      console.error(chalk.red(error.message));
      process.exit(1);
    }
  });

// Fraud Commands

const fraudCmd = program.command('fraud').description('Fraud proof operations');

fraudCmd
  .command('submit')
  .description('Submit fraud proof (verifier)')
  .requiredOption('--jobId <id>', 'Job ID (hex)')
  .requiredOption('--wasm <file>', 'Path to Wasm module')
  .requiredOption('--input <file>', 'Path to input file')
  .requiredOption('--claimedOutput <file>', 'Path to claimed output file')
  .action(async (options) => {
    const spinner = ora('Submitting fraud proof...').start();

    try {
      const wasmBytes = fs.readFileSync(options.wasm);
      const inputBytes = fs.readFileSync(options.input);
      const claimedOutput = fs.readFileSync(options.claimedOutput);

      const { signer } = getSignerAndProvider();
      const contract = getContract(signer);

      spinner.text = 'Submitting fraud proof (this will trigger on-chain Wasm re-execution)...';

      const tx = await contract.fraudOnChain(
        options.jobId,
        wasmBytes,
        inputBytes,
        claimedOutput,
        { gasLimit: 3000000 } // High gas for on-chain execution
      );

      await tx.wait();

      spinner.succeed(chalk.green('Fraud proof submitted! Executor has been slashed.'));
      console.log(chalk.cyan('Tx hash:'), tx.hash);

    } catch (error: any) {
      spinner.fail(chalk.red('Failed to submit fraud proof'));
      console.error(chalk.red(error.message));
      process.exit(1);
    }
  });

// Parse and execute
program.parse();
