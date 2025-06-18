#!/bin/bash

echo "🚀 Starting X1 Testnet Faucet Service..."
echo ""

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ Error: Rust/Cargo not found"
    echo "Please install Rust first: https://rustup.rs/"
    exit 1
fi

# Check if Solana CLI is installed
if ! command -v solana &> /dev/null; then
    echo "❌ Error: Solana CLI not found"
    echo "Please install Solana CLI first:"
    echo "sh -c \"$(curl -sSfL https://release.anza.xyz/stable/install)\""
    exit 1
fi

# Check if keypair exists
KEYPAIR_PATH="$HOME/.config/solana/id.json"
if [ ! -f "$KEYPAIR_PATH" ]; then
    echo "❌ Error: X1 Testnet keypair not found at $KEYPAIR_PATH"
    echo "Please generate a keypair first:"
    echo "  solana-keygen new"
    exit 1
fi

# Display current configuration
echo "📋 Current X1 Testnet Configuration:"
solana config get

echo ""
echo "💰 Checking faucet account balance..."
PUBKEY=$(solana-keygen pubkey ~/.config/solana/id.json)
echo "Faucet public key: $PUBKEY"

# Check balance with error handling
echo "Fetching balance..."
BALANCE_OUTPUT=$(solana balance --output json 2>/dev/null)
if [ $? -eq 0 ] && [ -n "$BALANCE_OUTPUT" ]; then
    BALANCE=$(echo "$BALANCE_OUTPUT" | grep -o '"value":[0-9]*' | cut -d':' -f2)
    if [ -n "$BALANCE" ] && [ "$BALANCE" -lt 1000000000 ]; then  # Less than 1 XNT (in lamports)
        echo "⚠️  Warning: Low balance! Please fund your account:"
        echo "  Make sure you're connected to X1 testnet (https://rpc.testnet.x1.xyz)"
    elif [ -n "$BALANCE" ]; then
        XNT_BALANCE=$(echo "scale=4; $BALANCE / 1000000000" | bc -l 2>/dev/null || echo "unknown")
        echo "✅ Balance: $XNT_BALANCE XNT"
    fi
else
    echo "⚠️  Warning: Could not fetch balance. Please ensure:"
    echo "  - You're connected to X1 testnet: solana config set --url https://rpc.testnet.x1.xyz"
    echo "  - Your account is funded"
fi

# Enter backend directory
cd backend

echo ""
echo "📦 Checking dependencies..."
cargo check

# Check if we need sudo for port 80
PORT=80
USE_SUDO=false

echo ""
echo "🔍 Checking port $PORT permissions..."

# Test if we can bind to port 80 without sudo
if [ "$PORT" -lt 1024 ]; then
    echo "⚠️  Port $PORT requires elevated privileges"
    
    # Check if we're already running as root
    if [ "$EUID" -eq 0 ]; then
        echo "✅ Running as root, no sudo needed"
        USE_SUDO=false
    else
        echo "🔐 Will use sudo to bind to port $PORT"
        USE_SUDO=true
        
        # Test sudo access
        if ! sudo -n true 2>/dev/null; then
            echo "🔑 Please enter your password for sudo access:"
            sudo -v
            if [ $? -ne 0 ]; then
                echo "❌ Error: sudo access required for port $PORT"
                exit 1
            fi
        fi
    fi
else
    echo "✅ Port $PORT doesn't require elevated privileges"
fi

echo ""
echo "🏃 Starting backend service..."
echo "Service will run at: http://localhost:$PORT"
echo ""
echo "After startup:"
echo "1. The service will use your configured X1 Testnet keypair"
echo "2. Visit http://localhost:$PORT to use the faucet"
echo "3. No need to open HTML file separately!"
echo ""

# Start the service with or without sudo
if [ "$USE_SUDO" = true ]; then
    echo "🔐 Starting with elevated privileges..."
    # Preserve environment variables for cargo and solana
    sudo -E env PATH="$PATH" HOME="$HOME" USER="$USER" cargo run
else
    cargo run
fi 