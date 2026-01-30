use anyhow::Result;
use ethers::prelude::*;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};
use base64::{engine::general_purpose, Engine as _};
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
}

impl WalletManager {
    pub async fn new(private_key: &str) -> Result<Self> {
        let wallet = LocalWallet::from_str(private_key)?
            .with_chain_id(84532u64); // Base Sepolia
        
        let provider = Provider::<Http>::try_from("https://sepolia.base.org")?;
        
        Ok(Self {
            wallet,
            provider: Arc::new(provider),
        })
    }

    pub async fn check_balances(&self) -> Result<()> {
        let address = self.wallet.address();
        info!("Checking balances for wallet: {:?}", address);

        // Check ETH balance for gas
        let eth_balance = self.provider.get_balance(address, None).await?;
        info!("ETH Balance: {} ETH", ethers::utils::format_ether(eth_balance));
        
        if eth_balance == U256::zero() {
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
        let usdc_balance = contract.balance_of(address).call().await?;
        let decimals = contract.decimals().call().await?;
        
        info!("USDC Balance: {} USDC", usdc_balance.as_u64() as f64 / 10f64.powi(decimals as i32));

        if usdc_balance == U256::zero() {
            warn!("Warning: USDC balance is zero. Validation might fail for paid endpoints.");
        }

        Ok(())
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
        Ok(general_purpose::STANDARD.encode(json))
    }
}
