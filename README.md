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

### Full Payment Validation (Step 2)
To verify the full payment lifecycle (402 -> Signing -> 200 OK), you must provide a private key with testnet USDC on Base Sepolia.

1. Set your private key:
```bash
export X402_WATCH_PRIVATE_KEY=your_private_key_here
# On Windows PowerShell:
# $env:X402_WATCH_PRIVATE_KEY="your_private_key_here"
```

2. Run the tool:
```bash
cargo run -- --urls https://api.example.com/data
```
The tool will automatically detect the key, check your balance, and attempt the full payment cycle.

### JSON Output
Output results in JSON format for CI/CD pipelines:
```bash
./target/release/x402-watch --format json
```
JSON results will include `error_code` fields for automated diagnostics (e.g., `INSUFFICIENT_FUNDS`, `SETTLEMENT_FAILURE`).

## Next Steps
- Implement payment signing and testnet settlement.
- Add support for facilitators.
- Add a web dashboard for visualization.

## License
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.