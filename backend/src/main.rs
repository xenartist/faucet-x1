use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_instruction,
    transaction::Transaction,
};
use std::{path::PathBuf, str::FromStr, sync::Arc, time::Duration};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{error, info, warn};
use uuid::Uuid;
use rand::Rng;

// X1 Network token constants
const LAMPORTS_PER_XNT: u64 = LAMPORTS_PER_SOL; // 1 XNT = 1e9 lamports

// Math challenge structure
#[derive(Debug, Clone, Serialize)]
struct MathChallenge {
    question: String,
    session_id: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug)]
struct MathChallengeData {
    answer: i32,
    expires_at: DateTime<Utc>,
}

// Math challenge store
#[derive(Debug)]
struct MathChallengeStore {
    challenges: DashMap<String, MathChallengeData>,
}

impl MathChallengeStore {
    fn new() -> Self {
        Self {
            challenges: DashMap::new(),
        }
    }

    fn generate_challenge(&self) -> MathChallenge {
        let mut rng = rand::thread_rng();
        let a = rng.gen_range(1..=20);
        let b = rng.gen_range(1..=20);
        let answer = a + b;
        let session_id = Uuid::new_v4().to_string();
        let expires_at = Utc::now() + chrono::Duration::minutes(5);

        // Store the challenge data
        self.challenges.insert(
            session_id.clone(),
            MathChallengeData {
                answer,
                expires_at,
            },
        );

        MathChallenge {
            question: format!("{} + {} = ?", a, b),
            session_id,
            expires_at,
        }
    }

    fn verify_answer(&self, session_id: &str, user_answer: i32) -> bool {
        if let Some(challenge_data) = self.challenges.get(session_id) {
            let now = Utc::now();
            let correct_answer = challenge_data.answer;
            let expires_at = challenge_data.expires_at;
            
            if now > expires_at {
                // Challenge expired
                drop(challenge_data);
                self.challenges.remove(session_id);
                return false;
            }
            
            let is_correct = correct_answer == user_answer;
            
            if is_correct {
                // Remove challenge after successful verification
                drop(challenge_data);
                self.challenges.remove(session_id);
            }
            
            return is_correct;
        }
        
        false
    }

    fn cleanup_expired(&self) {
        let now = Utc::now();
        self.challenges.retain(|_, challenge_data| {
            now <= challenge_data.expires_at
        });
    }
}

// Application state
#[derive(Clone)]
struct AppState {
    rpc_client: Arc<RpcClient>,
    faucet_keypair: Arc<Keypair>,
    rate_limiter: Arc<RateLimiter>,
    math_store: Arc<MathChallengeStore>,
}

// Rate limiter
#[derive(Debug)]
struct RateLimiter {
    requests: DashMap<String, DateTime<Utc>>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            requests: DashMap::new(),
        }
    }

    fn check_and_update(&self, key: &str) -> bool {
        let now = Utc::now();
        
        if let Some(mut entry) = self.requests.get_mut(key) {
            let last_request = *entry;
            if now.signed_duration_since(last_request).num_hours() < 24 {
                return false; // Still within 24-hour limit
            }
            *entry = now;
        } else {
            self.requests.insert(key.to_string(), now);
        }
        
        true
    }

    // Clean up expired records
    fn cleanup_expired(&self) {
        let now = Utc::now();
        self.requests.retain(|_, last_request| {
            now.signed_duration_since(*last_request).num_hours() < 24
        });
    }
}

// Request structures
#[derive(Deserialize)]
struct AirdropRequest {
    public_key: String,
    math_session_id: String,
    math_answer: i32,
}

// Response structures
#[derive(Serialize)]
struct AirdropResponse {
    signature: String,
    message: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// Error types
#[derive(thiserror::Error, Debug)]
enum FaucetError {
    #[error("Invalid public key format")]
    InvalidPublicKey,
    #[error("Too many requests, please try again in 24 hours")]
    RateLimited,
    #[error("Solana network error: {0}")]
    SolanaError(String),
    #[error("Insufficient funds")]
    InsufficientFunds,
    #[error("Math challenge verification failed")]
    MathVerificationFailed,
}

impl FaucetError {
    fn status_code(&self) -> StatusCode {
        match self {
            FaucetError::InvalidPublicKey => StatusCode::BAD_REQUEST,
            FaucetError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            FaucetError::SolanaError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            FaucetError::InsufficientFunds => StatusCode::SERVICE_UNAVAILABLE,
            FaucetError::MathVerificationFailed => StatusCode::BAD_REQUEST,
        }
    }
}

// Load keypair from Solana CLI config
async fn load_keypair_from_config() -> anyhow::Result<Keypair> {
    // Get the home directory
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?;
    
    // Construct path to Solana CLI keypair file
    let keypair_path: PathBuf = home_dir.join(".config/solana/id.json");
    
    info!("Loading keypair from: {}", keypair_path.display());
    
    // Check if file exists
    if !keypair_path.exists() {
        return Err(anyhow::anyhow!(
            "Keypair file not found at {}. Please run 'solana-keygen new' to generate a keypair",
            keypair_path.display()
        ));
    }
    
    // Read the file
    let keypair_data = tokio::fs::read_to_string(&keypair_path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {}", e))?;
    
    // Parse the JSON array
    let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_data)
        .map_err(|e| anyhow::anyhow!("Failed to parse keypair JSON: {}", e))?;
    
    // Create keypair from bytes
    if keypair_bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "Invalid keypair file: expected 64 bytes, got {}",
            keypair_bytes.len()
        ));
    }
    
    Keypair::from_bytes(&keypair_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to create keypair from bytes: {}", e))
}

// API endpoints

// Health check endpoint
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "timestamp": Utc::now().to_rfc3339()
    }))
}

// Math challenge endpoint
async fn get_math_challenge(
    State(state): State<AppState>,
) -> Json<MathChallenge> {
    let challenge = state.math_store.generate_challenge();
    Json(challenge)
}

// Airdrop handler function
async fn handle_airdrop(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<AirdropRequest>,
) -> Result<Json<AirdropResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received airdrop request: {}", payload.public_key);

    // Verify math challenge first
    if !state.math_store.verify_answer(&payload.math_session_id, payload.math_answer) {
        warn!("Math challenge verification failed for session: {}", payload.math_session_id);
        return Err((
            FaucetError::MathVerificationFailed.status_code(),
            Json(ErrorResponse {
                error: FaucetError::MathVerificationFailed.to_string(),
            }),
        ));
    }

    // Parse public key
    let recipient_pubkey = Pubkey::from_str(&payload.public_key)
        .map_err(|_| {
            warn!("Invalid public key format: {}", payload.public_key);
            (
                FaucetError::InvalidPublicKey.status_code(),
                Json(ErrorResponse {
                    error: FaucetError::InvalidPublicKey.to_string(),
                }),
            )
        })?;

    // Get client IP (for rate limiting)
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .split(',')
        .next()
        .unwrap_or("unknown");

    // Use combination of public key and IP as rate limit key
    let rate_limit_key = format!("{}:{}", payload.public_key, client_ip);

    // Check rate limit
    if !state.rate_limiter.check_and_update(&rate_limit_key) {
        warn!("Rate limit triggered: {}", rate_limit_key);
        return Err((
            FaucetError::RateLimited.status_code(),
            Json(ErrorResponse {
                error: FaucetError::RateLimited.to_string(),
            }),
        ));
    }

    // Get latest blockhash
    let recent_blockhash = state
        .rpc_client
        .get_latest_blockhash()
        .map_err(|e| {
            error!("Failed to get latest blockhash: {}", e);
            (
                FaucetError::SolanaError(e.to_string()).status_code(),
                Json(ErrorResponse {
                    error: FaucetError::SolanaError(e.to_string()).to_string(),
                }),
            )
        })?;

    // Check faucet account balance
    let faucet_balance = state
        .rpc_client
        .get_balance(&state.faucet_keypair.pubkey())
        .map_err(|e| {
            error!("Failed to get faucet balance: {}", e);
            (
                FaucetError::SolanaError(e.to_string()).status_code(),
                Json(ErrorResponse {
                    error: FaucetError::SolanaError(e.to_string()).to_string(),
                }),
            )
        })?;

    // Check if balance is sufficient (1 XNT + fees)
    let airdrop_amount = LAMPORTS_PER_XNT; // 1 XNT
    if faucet_balance < airdrop_amount + 5000 {
        // Reserve 5000 lamports for fees
        error!("Insufficient faucet balance: {} lamports", faucet_balance);
        return Err((
            FaucetError::InsufficientFunds.status_code(),
            Json(ErrorResponse {
                error: FaucetError::InsufficientFunds.to_string(),
            }),
        ));
    }

    // Create transfer instruction
    let transfer_instruction = system_instruction::transfer(
        &state.faucet_keypair.pubkey(),
        &recipient_pubkey,
        airdrop_amount,
    );

    // Create transaction
    let mut transaction = Transaction::new_with_payer(
        &[transfer_instruction],
        Some(&state.faucet_keypair.pubkey()),
    );

    // Sign transaction
    transaction.sign(&[state.faucet_keypair.as_ref()], recent_blockhash);

    // Send and confirm transaction
    let signature = state
        .rpc_client
        .send_and_confirm_transaction_with_spinner_and_commitment(
            &transaction,
            CommitmentConfig::confirmed(),
        )
        .map_err(|e| {
            error!("Failed to send transaction: {}", e);
            (
                FaucetError::SolanaError(e.to_string()).status_code(),
                Json(ErrorResponse {
                    error: FaucetError::SolanaError(e.to_string()).to_string(),
                }),
            )
        })?;

    info!("Airdrop successful! Signature: {}, Recipient: {}", signature, recipient_pubkey);

    Ok(Json(AirdropResponse {
        signature: signature.to_string(),
        message: format!("Successfully airdropped 1 XNT to {}", recipient_pubkey),
    }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("Starting X1 Testnet Faucet Service...");

    // Load faucet keypair from Solana CLI config
    let faucet_keypair = match load_keypair_from_config().await {
        Ok(keypair) => {
            info!("Successfully loaded faucet keypair from ~/.config/solana/id.json");
            Arc::new(keypair)
        }
        Err(e) => {
            error!("Failed to load keypair: {}", e);
            error!("Please ensure you have a Solana keypair generated. Run:");
            error!("  solana-keygen new");
            error!("Or if you want to use an existing keypair:");
            error!("  solana config set --keypair <path-to-your-keypair>");
            return Err(e);
        }
    };
    
    info!("Faucet public key: {}", faucet_keypair.pubkey());

    // Connect to X1 testnet
    let rpc_url = "https://rpc.testnet.x1.xyz";
    let rpc_client = Arc::new(RpcClient::new_with_commitment(
        rpc_url.to_string(),
        CommitmentConfig::confirmed(),
    ));

    // Check connection
    match rpc_client.get_health() {
        Ok(_) => info!("Successfully connected to X1 testnet"),
        Err(e) => {
            error!("Failed to connect to X1 testnet: {}", e);
            return Err(anyhow::anyhow!("Unable to connect to X1 network"));
        }
    }

    // Check faucet balance
    match rpc_client.get_balance(&faucet_keypair.pubkey()) {
        Ok(balance) => {
            let xnt_balance = balance as f64 / LAMPORTS_PER_XNT as f64;
            info!("Faucet account balance: {:.4} XNT ({} lamports)", xnt_balance, balance);
            if balance < LAMPORTS_PER_XNT {
                warn!("⚠️  Low balance! Please fund the faucet account with XNT");
            }
        }
        Err(e) => {
            error!("Failed to check faucet balance: {}", e);
            warn!("Please ensure the faucet account is funded with XNT");
        }
    }

    // Create rate limiter and math store
    let rate_limiter = Arc::new(RateLimiter::new());
    let math_store = Arc::new(MathChallengeStore::new());

    // Start cleanup tasks
    let rate_limiter_cleanup = rate_limiter.clone();
    let math_store_cleanup = math_store.clone();
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Clean up every hour
        loop {
            interval.tick().await;
            rate_limiter_cleanup.cleanup_expired();
            math_store_cleanup.cleanup_expired();
            info!("Cleaned up expired rate limit and math challenge records");
        }
    });

    // Create application state
    let app_state = AppState {
        rpc_client,
        faucet_keypair,
        rate_limiter,
        math_store,
    };

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Create routes
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/challenge", get(get_math_challenge))
        .route("/airdrop", post(handle_airdrop))
        .nest_service("/", ServeDir::new("../frontend"))
        .layer(cors)
        .with_state(app_state);

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:80").await?;
    info!("Server started at http://0.0.0.0:80");

    axum::serve(listener, app).await?;

    Ok(())
} 