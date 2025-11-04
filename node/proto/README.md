# Certus Protocol Definitions

gRPC protocol definitions for inter-node communication.

## Structure

```
proto/
├── src/main/proto/
│   └── certus.proto     # Service definitions
└── build/              # Generated Java code
```

## Proto Services

### DirectoryService
Node discovery and registration
- `registerExecutor()`
- `registerVerifier()`
- `listExecutors()`
- `getVerifierHeartbeat()`

### JobService
Job lifecycle management
- `submitJob()`
- `acceptJob()`
- `submitReceipt()`
- `getJobStatus()`

## Building

```bash
../gradlew generateProto
```

Generated code appears in `build/generated/source/proto/main/java/`

## Usage

```java
ManagedChannel channel = ManagedChannelBuilder
    .forAddress("directory.certus.network", 50051)
    .usePlaintext()
    .build();

DirectoryServiceGrpc.DirectoryServiceBlockingStub stub =
    DirectoryServiceGrpc.newBlockingStub(channel);
```
