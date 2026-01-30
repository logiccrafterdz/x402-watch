# x402-watch

A lightweight health monitoring tool for public x402-enabled APIs.

## Purpose
`x402-watch` periodically verifies that x402-compliant endpoints behave correctly by asserting that:
1. They return `402 Payment Required` when accessed without payment.
2. They include a valid `PAYMENT-REQUIRED` header.
3. The header contains a well-formed, base64-encoded `PaymentRequirement` object.

## Usage

### Installation
Ensure you have Rust installed.
```bash
cargo build --release
```

### Running the CLI
Run a single check against a list of URLs:
```bash
./target/release/x402-watch --urls https://api.example.com/data
```

Run with a configuration file:
```bash
./target/release/x402-watch --config endpoints.yaml
```

### Periodic Mode
Monitor endpoints every 5 minutes:
```bash
./target/release/x402-watch --interval 5m
```

### JSON Output
Output results in JSON format for CI/CD pipelines:
```bash
./target/release/x402-watch --format json
```

## Next Steps
- Implement payment signing and testnet settlement.
- Add support for facilitators.
- Add a web dashboard for visualization.
