# x402-watch

A lightweight health monitoring tool for public x402-enabled APIs.

Built for the x402 ecosystem as an observability tool for public x402 APIs. **Now fully compliant with x402 Protocol Specification v2.**

## Purpose
`x402-watch` periodically verifies that x402-compliant endpoints behave correctly by asserting that:
1. They return `402 Payment Required` when accessed without payment.
2. They include a valid `PAYMENT-REQUIRED` header with `x402Version: 2`.
3. The header contains a well-formed, base64-encoded `PaymentRequired` object with `accepts` array and `payTo` field.

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
cargo run -- --urls https://api.x402.org/demo --verify-settlement --settlement-timeout 60s
```
The tool will automatically detect the key, check your balance, and attempt the full payment cycle.

## Practical Workflow Example

### 1. Prepare `endpoints.yaml`
Create a configuration file with the APIs you want to monitor:
```yaml
endpoints:
  - name: "X402 Demo"
    url: "https://api.x402.org/demo"
  - name: "My Protected API"
    url: "https://api.myapp.com/v1/protected"
```

### 2. Run the Watcher
Monitor your endpoints in periodic mode with JSON output for your observability pipeline:
```bash
$env:X402_WATCH_PRIVATE_KEY="0x..."
cargo run -- --config endpoints.yaml --interval 10m --format json
```

### 3. Sample Output 
```text
--- x402 Health Report ---
Endpoint             | Status | Error Code                | Message
----------------------------------------------------------------------------------------------------
X402 Demo            | PASS   | -                         | Full payment lifecycle verified
My Protected API     | FAIL   | SETTLEMENT_FAILURE        | Settlement failure: expected 200 OK after signing, got 403 Forbidden
----------------------------------------------------------------------------------------------------
```

### JSON Output
Output results in JSON format for CI/CD pipelines:
```bash
./target/release/x402-watch --format json
```
JSON results will include `error_code` fields for automated diagnostics (e.g., `INSUFFICIENT_FUNDS`, `SETTLEMENT_FAILURE`).

## Features
-  **x402 v2 Compliant**: Uses `x402Version`, `payTo`, and structured `PaymentPayload`
-  **GET & POST Support**: Configure HTTP method per endpoint via `--method` flag or `endpoints.yaml`
-  **Dry-run validation**: Verify 402 response and PaymentRequirement structure
-  **Full payment lifecycle**: Sign and submit payments with automatic retries
-  **Balance verification**: Check ETH and USDC balances before signing
-  **Smart retry logic**: Exponential backoff for 202 Accepted / 425 Too Early
-  **Settlement verification**: HTTP facilitator + on-chain fallback
-  **Multiple output formats**: Human-readable and JSON for CI/CD
-  **Periodic monitoring**: Configurable interval for continuous checks

## POST Request Support

Configure POST requests per endpoint in `endpoints.yaml`:
```yaml
endpoints:
  - name: "POST Protected API"
    url: "https://api.example.com/v1/create"
    method: "POST"
  - name: "GET Demo"
    url: "https://api.x402.org/demo"
    method: "GET"
```

Or use the `--method` flag for CLI-specified URLs:
```bash
cargo run -- --urls https://api.example.com/data --method POST
```

## Testing & Verification

### Testing Against Coinbase Reference Server

To verify full x402 v2 compliance, test against the reference examples from the [coinbase/x402](https://github.com/coinbase/x402) repository:

1. **Clone and run a reference server locally:**
```bash
git clone https://github.com/coinbase/x402.git
cd x402/examples/typescript/express
npm install
npm start
```

2. **Test with x402-watch:**
```bash
cargo run -- --urls http://localhost:4021/weather
```

Expected output for a compliant endpoint:
```text
Endpoint             | Status | Error Code                | Message
----------------------------------------------------------------------------------------------------
http://localhost:4021/weather | PASS   | -                         | Full payment lifecycle verified (x402 v2 compliant)
```

### Schema Validation Checklist

This tool validates the following x402 v2 fields:
- `x402Version`: Must be `2` (number, not string)
- `accepts`: Array of payment requirements
- `payTo`: Recipient wallet address in each requirement
- `scheme`, `network`, `asset`, `amount`, `maxTimeoutSeconds`: Required fields

### POST Method Testing
```bash
# Test POST endpoint
cargo run -- --urls https://api.example.com/create --method POST

# Or via endpoints.yaml with method field
cargo run -- --config endpoints.yaml
```

## License
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.