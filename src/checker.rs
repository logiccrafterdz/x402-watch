use anyhow::{anyhow, Result};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{info, error};
use base64::{engine::general_purpose, Engine as _};

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub url: String,
    pub status: CheckStatus,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum CheckStatus {
    Pass,
    Fail,
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
}

impl Checker {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    pub async fn check(&self, name: &str, url: &str) -> CheckResult {
        info!("Checking endpoint: {} ({})", name, url);
        
        match self.do_check(url).await {
            Ok(_) => CheckResult {
                name: name.to_string(),
                url: url.to_string(),
                status: CheckStatus::Pass,
                message: "Valid x402 endpoint (402 status + valid PaymentRequirement)".to_string(),
            },
            Err(e) => {
                error!("Check failed for {}: {}", name, e);
                CheckResult {
                    name: name.to_string(),
                    url: url.to_string(),
                    status: CheckStatus::Fail,
                    message: e.to_string(),
                }
            }
        }
    }

    async fn do_check(&self, url: &str) -> Result<()> {
        let resp = self.client.get(url).send().await?;

        if resp.status() != StatusCode::PAYMENT_REQUIRED {
            return Err(anyhow!("Expected status 402 Payment Required, got {}", resp.status()));
        }

        let header_val = resp.headers()
            .get("PAYMENT-REQUIRED")
            .ok_or_else(|| anyhow!("Missing PAYMENT-REQUIRED header"))?
            .to_str()
            .map_err(|_| anyhow!("Invalid characters in PAYMENT-REQUIRED header"))?;

        // Validate base64
        let decoded_bytes = general_purpose::STANDARD.decode(header_val)
            .map_err(|e| anyhow!("Invalid base64 in PAYMENT-REQUIRED header: {}", e))?;

        let decoded_str = String::from_utf8(decoded_bytes)
            .map_err(|e| anyhow!("Decoded PAYMENT-REQUIRED header is not valid UTF-8: {}", e))?;

        // Parse and validate the PaymentRequirement JSON
        let _payment_requirement: PaymentRequirement = serde_json::from_str(&decoded_str)
            .map_err(|e| anyhow!("Failed to parse PaymentRequirement JSON: {}", e))?;

        Ok(())
    }
}
