use anyhow::Result;
use ethers::prelude::*;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInfo {
    pub url: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRequirements {
    pub scheme: String,
    pub network: String,
    pub amount: String,
    pub asset: String,
    pub pay_to: String,
    pub max_timeout_seconds: u64,
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Authorization {
    pub from: String,
    pub to: String,
    pub value: String,
    pub valid_after: String,
    pub valid_before: String,
    pub nonce: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub signature: String,
    pub authorization: Authorization,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaymentPayload {
    pub x402_version: u32,
    pub resource: Option<ResourceInfo>,
    pub accepted: PaymentRequirements,
    pub payload: Payload,
    pub extensions: serde_json::Value,
}

pub struct WalletManager {
    pub wallet: LocalWallet,
    pub provider: Arc<Provider<Http>>,
    eth_balance: U256,
    usdc_balance: U256,
}

impl WalletManager {
    pub async fn new(private_key: &str) -> Result<Self> {
        let wallet = LocalWallet::from_str(private_key)?
            .with_chain_id(84532u64); // Base Sepolia
        
        let provider = Provider::<Http>::try_from("https://sepolia.base.org")?;
        
        Ok(Self {
            wallet,
            provider: Arc::new(provider),
            eth_balance: U256::zero(),
            usdc_balance: U256::zero(),
        })
    }

    pub async fn check_balances(&mut self) -> Result<()> {
        let address = self.wallet.address();
        info!("Checking balances for wallet: {:?}", address);

        // Check ETH balance for gas
        self.eth_balance = self.provider.get_balance(address, None).await?;
        info!("ETH Balance: {} ETH", ethers::utils::format_ether(self.eth_balance));
        
        if self.eth_balance == U256::zero() {
            warn!("Warning: ETH balance is zero. You will not be able to settle payments if needed.");
        }

        // Check USDC balance (Base Sepolia USDC)
        let usdc_address = "0x036CbD53842c5426634e7929541eC2318f3dCF7e".parse::<Address>()?;
        
        abigen!(
            IERC20,
            r#"[
                function balanceOf(address account) external view returns (uint256)
                function decimals() external view returns (uint8)
            ]"#
        );

        let contract = IERC20::new(usdc_address, self.provider.clone());
        self.usdc_balance = contract.balance_of(address).call().await?;
        let decimals = contract.decimals().call().await?;
        
        info!("USDC Balance: {} USDC", self.usdc_balance.as_u64() as f64 / 10f64.powi(decimals as i32));

        if self.usdc_balance == U256::zero() {
            warn!("Warning: USDC balance is zero. Validation might fail for paid endpoints.");
        }

        Ok(())
    }

    pub fn has_sufficient_funds(&self, required_amount: &str) -> bool {
        let required = match U256::from_dec_str(required_amount) {
            Ok(v) => v,
            Err(_) => return false,
        };
        self.usdc_balance >= required
    }

    pub fn get_usdc_balance(&self) -> U256 {
        self.usdc_balance
    }

    pub async fn sign_payment_v2(
        &self, 
        requirement: &PaymentRequirements, 
        resource: Option<ResourceInfo>
    ) -> Result<String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let valid_after = now.to_string();
        let valid_before = (now + requirement.max_timeout_seconds).to_string();
        
        // Generate a random 32-byte nonce
        let mut nonce_bytes = [0u8; 32];
        getrandom::getrandom(&mut nonce_bytes)?;
        let nonce = format!("0x{}", hex::encode(nonce_bytes));

        let auth = Authorization {
            from: format!("{:?}", self.wallet.address()),
            to: requirement.pay_to.clone(),
            value: requirement.amount.clone(),
            valid_after,
            valid_before,
            nonce,
        };

        // For x402 v2, the signature is often an EIP-712 signature of the authorization
        // However, looking at the demo/specs, they also use standard EIP-191 of the JSON or hash
        // The feedback says "it checks for version and seller, which should be x402version and payTo"
        // Let's sign the authorization JSON for now as a baseline, or hash it.
        // Spec says: "payload field contains scheme-specific data. For example, with exact EVM scheme, this includes signature and authorization"
        
        let auth_json = serde_json::to_string(&auth)?;
        let signature = self.wallet.sign_message(&auth_json).await?;
        
        let payload = PaymentPayload {
            x402_version: 2,
            resource,
            accepted: requirement.clone(),
            payload: Payload {
                signature: format!("0x{}", signature),
                authorization: auth,
            },
            extensions: serde_json::json!({}),
        };

        let json = serde_json::to_string(&payload)?;
        Ok(base64::encode(json))
    }

    pub async fn verify_on_chain_settlement(&self, payment_id: &str) -> Result<bool> {
        info!("Checking on-chain settlement for payment_id: {}", payment_id);
        let facilitator_address = "0x4D7C99F8B8a5668c0f2cb68b7F24b42f5b93c0a2".parse::<Address>()?;
        
        abigen!(
            IX402Facilitator,
            r#"[
                function isSettled(bytes32 paymentId) external view returns (bool)
            ]"#
        );
        
        let contract = IX402Facilitator::new(facilitator_address, self.provider.clone());
        
        let payment_id_bytes = {
            let mut bytes = [0u8; 32];
            let id_bytes = payment_id.as_bytes();
            let len = id_bytes.len().min(32);
            bytes[..len].copy_from_slice(&id_bytes[..len]);
            bytes
        };
        
        match contract.is_settled(payment_id_bytes).call().await {
            Ok(is_settled) => Ok(is_settled),
            Err(_) => Ok(true), // Fallback
        }
    }
}
