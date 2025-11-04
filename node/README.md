# Certus Node Components

Java implementation of Certus protocol components.

## Modules

- **client** - SDK for submitting jobs and interacting with escrow
- **executor** - Node that accepts and executes compute jobs
- **verifier** - Node that validates executor results and submits fraud proofs
- **proto** - gRPC protocol definitions and generated code

## Build

```bash
./gradlew clean build
```

## Run Tests

```bash
./gradlew test
```

## Requirements

- Java 17 or higher
- Gradle 8.5
- 4GB RAM minimum
- Arbitrum Sepolia RPC endpoint
