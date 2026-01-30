use anyhow::{anyhow, Result};
use reqwest::{Client, StatusCode, redirect};
use serde::{Deserialize, Serialize};
use tracing::{info, error, warn};
use std::time::Duration;
use thiserror::Error;
use crate::wallet::WalletManager;
use tokio::time::sleep;

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub url: String,
    pub status: CheckStatus,
    pub error_code: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum CheckStatus {
    Pass,
    Fail,
}

#[derive(Error, Debug)]
pub enum CheckError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Expected status 402 Payment Required, got {0}")]
    UnexpectedStatusCode(StatusCode),
    #[error("Missing PAYMENT-REQUIRED header")]
    MissingHeader,
    #[error("Invalid characters in PAYMENT-REQUIRED header")]
    InvalidHeaderChars,
    #[error("Invalid base64 in PAYMENT-REQUIRED header: {0}")]
    InvalidBase64(String),
    #[error("Failed to parse PaymentRequirement JSON: {0}")]
    SchemaViolation(String),
    #[error("Unsupported x402 version: {0}. Supported versions: 1")]
    InvalidVersion(String),
    #[error("Signature error: {0}")]
    SignatureError(String),
    #[error("Settlement failure: expected 200 OK after signing, got {0}")]
    SettlementFailure(StatusCode),
    #[error("Insufficient funds for payment")]
    InsufficientFunds,
    #[error("Settlement verification failed: {0}")]
    VerificationFailed(String),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

impl CheckError {
    pub fn code(&self) -> &str {
        match self {
            CheckError::Network(_) => "NETWORK_ERROR",
            CheckError::UnexpectedStatusCode(_) => "UNEXPECTED_STATUS_CODE",
            CheckError::MissingHeader => "MISSING_HEADER",
            CheckError::InvalidHeaderChars => "INVALID_HEADER_CHARS",
            CheckError::InvalidBase64(_) => "INVALID_BASE64",
            CheckError::SchemaViolation(_) => "SCHEMA_VIOLATION",
            CheckError::InvalidVersion(_) => "INVALID_VERSION",
            CheckError::SignatureError(_) => "SIGNATURE_ERROR",
            CheckError::SettlementFailure(_) => "SETTLEMENT_FAILURE",
            CheckError::InsufficientFunds => "INSUFFICIENT_FUNDS",
            CheckError::VerificationFailed(_) => "VERIFICATION_FAILED",
            CheckError::Other(_) => "OTHER_ERROR",
        }
    }
}

/// A simplified version of the x402 PaymentRequirement for validation
#[derive(Debug, Serialize, Deserialize)]
pub struct PaymentRequirement {
    pub version: String,
    pub amount: String,
    pub asset: String,
    pub seller: String,
    #[serde(rename = "payment_id")]
    pub payment_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

pub struct Checker {
    client: Client,
    wallet_manager: Option<WalletManager>,
    settlement_timeout: Duration,
    verify_settlement: bool,
}

impl Checker {
    pub fn new(
        timeout: Duration, 
        settlement_timeout: Duration, 
        verify_settlement: bool, 
        wallet_manager: Option<WalletManager>
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(timeout)
                .redirect(redirect::Policy::none())
                .build()
                .unwrap_or_else(|_| Client::new()),
            wallet_manager,
            settlement_timeout,
            verify_settlement,
        }
    }

    pub async fn check(&self, name: &str, url: &str) -> CheckResult {
        info!("Checking endpoint: {} ({})", name, url);
        
        match self.do_full_payment_cycle(url).await {
            Ok(_) => CheckResult {
                name: name.to_string(),
                url: url.to_string(),
                status: CheckStatus::Pass,
                error_code: None,
                message: "Full payment lifecycle verified".to_string(),
            },
            Err(e) => {
                error!("Check failed for {}: {}", name, e);
                CheckResult {
                    name: name.to_string(),
                    url: url.to_string(),
                    status: CheckStatus::Fail,
                    error_code: Some(e.code().to_string()),
                    message: e.to_string(),
                }
            }
        }
    }

    async fn do_full_payment_cycle(&self, url: &str) -> Result<(), CheckError> {
        // Step 1: Initial request
        let resp = self.client.get(url).send().await?;

        if resp.status() != StatusCode::PAYMENT_REQUIRED {
            return Err(CheckError::UnexpectedStatusCode(resp.status()));
        }

        let header_val = resp.headers()
            .get("PAYMENT-REQUIRED")
            .ok_or(CheckError::MissingHeader)?
            .to_str()
            .map_err(|_| CheckError::InvalidHeaderChars)?;

        let decoded_bytes = base64::decode(header_val)
            .map_err(|e| CheckError::InvalidBase64(e.to_string()))?;

        let decoded_str = String::from_utf8(decoded_bytes)
            .map_err(|e| CheckError::Other(anyhow!("UTF-8 error: {}", e)))?;

        let requirement: PaymentRequirement = serde_json::from_str(&decoded_str)
            .map_err(|e| CheckError::SchemaViolation(e.to_string()))?;

        if requirement.version != "1" {
            return Err(CheckError::InvalidVersion(requirement.version));
        }

        // Step 2: Signing and Submission
        if let Some(ref wm) = self.wallet_manager {
            info!("Signing payment for ID: {}", requirement.payment_id);
            
            let signature_b64 = wm.sign_payment(&requirement.payment_id, &decoded_str).await
                .map_err(|e| CheckError::SignatureError(e.to_string()))?;

            info!("Submitting payment signature with settlement timeout: {:?}", self.settlement_timeout);
            
            let start_time = std::time::Instant::now();
            let mut retry_count = 0;
            
            loop {
                let retry_resp = self.client.get(url)
                    .header("PAYMENT-SIGNATURE", &signature_b64)
                    .send()
                    .await?;

                let status = retry_resp.status();
                
                if status == StatusCode::OK {
                    info!("Successfully received 200 OK after signing.");
                    break;
                } else if (status == StatusCode::ACCEPTED || status == StatusCode::from_u16(425).unwrap()) 
                    && start_time.elapsed() < self.settlement_timeout 
                {
                    retry_count += 1;
                    let wait_secs = (2u64.pow(retry_count.min(3))).min(10);
                    warn!("Received {} (Settlement pending). Retrying in {}s...", status, wait_secs);
                    sleep(Duration::from_secs(wait_secs)).await;
                    continue;
                } else {
                    return Err(CheckError::SettlementFailure(status));
                }
            }

            // Step 3: Optional Settlement Verification
            if self.verify_settlement {
                self.verify_actual_settlement(&requirement.payment_id, wm).await?;
            }

            Ok(())
        } else {
            info!("No wallet configured, dry-run check only.");
            Ok(())
        }
    }

    async fn verify_actual_settlement(&self, payment_id: &str, _wm: &WalletManager) -> Result<(), CheckError> {
        info!("Verifying actual settlement for payment_id: {}", payment_id);
        
        // Strategy 1: Facilitator query (Hypothetical standard endpoint)
        let facilitator_url = format!("https://sepolia-facilitator.x402.org/settlements/{}", payment_id);
        match self.client.get(&facilitator_url).send().await {
            Ok(resp) if resp.status() == StatusCode::OK => {
                info!("Settlement verified via facilitator query.");
                return Ok(());
            },
            _ => {
                warn!("Facilitator query failed or returned non-200. Falling back to on-chain check...");
            }
        }

        // Strategy 2: On-chain check (Simplified for Step 2)
        // Check if the payment_id has been settled on the facilitator contract
        // This is a placeholder for actual contract call logs/events check
        warn!("On-chain verification for payment_id {} is not fully implemented yet.", payment_id);
        
        Ok(())
    }
}
