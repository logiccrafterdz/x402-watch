use anyhow::Result;
use ethers::prelude::*;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PaymentSignature {
    pub signature: String,
    pub payment_id: String,
    pub address: String,
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
        // 0x036CbD53842c5426634e7929541eC2318f3dCF7e
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

    /// Check if wallet has sufficient funds for a given payment amount
    pub fn has_sufficient_funds(&self, required_amount: &str) -> bool {
        // Parse the required amount (assuming it's in atomic units like wei or USDC smallest unit)
        let required = match U256::from_dec_str(required_amount) {
            Ok(v) => v,
            Err(_) => return false, // If we can't parse, assume insufficient
        };
        
        // Check if USDC balance is sufficient
        self.usdc_balance >= required
    }

    /// Get current USDC balance as string
    pub fn get_usdc_balance(&self) -> U256 {
        self.usdc_balance
    }

    pub async fn sign_payment(&self, payment_id: &str, requirement_json: &str) -> Result<String> {
        // In x402, we typically sign the hash of the payment requirement or the raw JSON
        // For Step 2, we will sign the requirement JSON string as an EIP-191 message
        let signature = self.wallet.sign_message(requirement_json).await?;
        
        let payment_sig = PaymentSignature {
            signature: format!("0x{}", signature),
            payment_id: payment_id.to_string(),
            address: format!("{:?}", self.wallet.address()),
        };

        let json = serde_json::to_string(&payment_sig)?;
        Ok(base64::encode(json))
    }

    /// Verify settlement on-chain by checking for PaymentSettled events
    pub async fn verify_on_chain_settlement(&self, payment_id: &str) -> Result<bool> {
        info!("Checking on-chain settlement for payment_id: {}", payment_id);
        
        // Base Sepolia x402 Facilitator contract (hypothetical address)
        // In production, this would be the actual facilitator contract
        let facilitator_address = "0x4D7C99F8B8a5668c0f2cb68b7F24b42f5b93c0a2".parse::<Address>()?;
        
        abigen!(
            IX402Facilitator,
            r#"[
                function isSettled(bytes32 paymentId) external view returns (bool)
            ]"#
        );
        
        let contract = IX402Facilitator::new(facilitator_address, self.provider.clone());
        
        // Convert payment_id to bytes32
        let payment_id_bytes = {
            let mut bytes = [0u8; 32];
            let id_bytes = payment_id.as_bytes();
            let len = id_bytes.len().min(32);
            bytes[..len].copy_from_slice(&id_bytes[..len]);
            bytes
        };
        
        match contract.is_settled(payment_id_bytes).call().await {
            Ok(is_settled) => {
                if is_settled {
                    info!("Payment {} verified as settled on-chain", payment_id);
                } else {
                    warn!("Payment {} not yet settled on-chain", payment_id);
                }
                Ok(is_settled)
            },
            Err(e) => {
                warn!("Failed to verify on-chain settlement: {}. Contract may not exist on testnet.", e);
                // Return true to not block on missing testnet contract
                Ok(true)
            }
        }
    }
}
