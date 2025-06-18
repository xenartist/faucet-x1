# X1 Testnet Faucet

A simple X1 testnet faucet program with a frontend web interface and Rust backend API.

## Features

- 🌐 Simple HTML+JS frontend interface
- 🦀 High-performance Rust backend API
- 🔒 Anti-bot rate limiting mechanism (24-hour limit)
- 💰 1 XNT airdrop per request
- 🚀 Connected to X1 Testnet
- 🔑 Uses your existing Solana CLI keypair

## Project Structure

```
faucet-x1/
├── frontend/
│   └── index.html          # Frontend web page
├── backend/
│   ├── Cargo.toml         # Rust dependencies
│   └── src/
│       └── main.rs        # Backend main program
└── README.md              # Project documentation
```

## Prerequisites

### Backend
- Rust 1.70+ 
- Cargo
- Solana CLI tools

### Frontend
- Modern browser (ES6+ support)

## Installation and Running

### 1. Install Solana CLI (if not already installed)

```bash
sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
```

### 2. Generate or Configure Keypair

If you don't have a Solana keypair yet:
```bash
solana-keygen new
```

If you want to use an existing keypair:
```bash
solana config set --keypair <path-to-your-keypair>
```

The faucet will automatically use the keypair configured in `~/.config/solana/id.json`.

### 3. Configure Network to X1 Testnet

```bash
solana config set --url https://rpc.testnet.x1.xyz
```

### 4. Fund Your Keypair

Get some XNT for your faucet account:
```bash
solana airdrop 2
```

Note: Make sure you're connected to X1 testnet when requesting airdrops.

### 5. Start Backend Service

```bash
cd backend
cargo run
```

The first run will download dependencies, which may take a few minutes.

After starting, the service will display:
- Your faucet public key address
- Current balance
- Service listening address: http://localhost:80

### 6. Access Web Interface

Visit http://localhost:8080 in your browser to use the airdrop service.
The backend now serves the frontend automatically!

## API Endpoints

### POST /airdrop
Request XNT airdrop

**Request Body:**
```json
{
  "public_key": "Your Solana public key address"
}
```

**Success Response:**
```json
{
  "signature": "Transaction signature",
  "message": "Successfully airdropped 1 XNT to xxx"
}
```

**Error Response:**
```json
{
  "error": "Error message"
}
```

### GET /health
Health check

**Response:**
```json
{
  "status": "healthy",
  "timestamp": "2024-01-01T00:00:00Z"
}
```

## Rate Limiting

- Each public key address + IP combination can only request once every 24 hours
- Uses in-memory cache to record request history
- Automatically cleans up expired records every hour

## Configuration

The faucet service automatically detects and uses your Solana CLI configuration:

- **Keypair**: `~/.config/solana/id.json`
- **Network**: Uses X1 testnet (https://rpc.testnet.x1.xyz)
- **Airdrop Amount**: 1 XNT per request

You can check your current Solana configuration with:
```bash
solana config get
```

## Security Notes

⚠️ **Important: This is a test program, not suitable for production environments**

Production environments should consider:
- Secure storage of private keys (use environment variables or key management services)
- Database persistence for rate limiting data
- Stricter input validation
- Monitoring and logging
- HTTPS encryption
- More sophisticated anti-bot measures

## Development Notes

### Main Dependencies

**Backend (Rust):**
- `axum` - Web framework
- `solana-client` - Solana RPC client
- `solana-sdk` - Solana SDK
- `dashmap` - Concurrent hash map
- `chrono` - Time handling
- `dirs` - Directory utilities

**Frontend (JavaScript):**
- Native HTML5 + ES6
- Fetch API for HTTP requests
- Responsive design

### Custom Configuration

You can modify the following in `backend/src/main.rs`:
- Airdrop amount (currently 1 XNT)
- Rate limit duration (currently 24 hours)
- RPC node address
- Service listening port

## Troubleshooting

### Common Issues

1. **"Keypair file not found"**
   - Run `solana-keygen new` to generate a keypair
   - Or set an existing keypair with `solana config set --keypair <path>`

2. **"Failed to connect to X1 testnet"**
   - Check network connection
   - Verify X1 testnet nodes are available
   - Ensure you're using the correct RPC URL

3. **"Insufficient faucet balance"**
   - Fund the faucet account with more XNT using `solana airdrop 2`
   - Make sure you're connected to X1 testnet when requesting airdrops

4. **"Invalid public key format"**
   - Ensure you're entering a valid Solana public key address

5. **CORS errors**
   - Backend is configured to allow all origins, if issues persist try using a local HTTP server

### Useful Commands

Check your keypair's public key:
```bash
solana-keygen pubkey ~/.config/solana/id.json
```

Check your balance:
```bash
solana balance
```

View transaction history:
```bash
solana transaction-history <signature>
```

Check current network configuration:
```bash
solana config get
```

## License

MIT License
