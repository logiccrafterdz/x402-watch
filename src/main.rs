mod config;
mod checker;
mod wallet;

use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;
use anyhow::{anyhow, Result};
use tokio::time::sleep;
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;
use dotenv::dotenv;

use crate::config::Config;
use crate::checker::Checker;
use crate::wallet::WalletManager;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to endpoints.yaml
    #[arg(short, long, default_value = "endpoints.yaml")]
    config: PathBuf,

    /// Interval for periodic mode (e.g. 5s, 10m, 1h)
    #[arg(short, long)]
    interval: Option<String>,

    /// Timeout for each request (e.g. 5s, 10s)
    #[arg(short, long, default_value = "10s")]
    timeout: String,

    /// Timeout for settlement verification (e.g. 30s, 60s)
    #[arg(long, default_value = "60s")]
    settlement_timeout: String,

    /// Verify actual settlement via facilitator or on-chain
    #[arg(long, default_value_t = false)]
    verify_settlement: bool,

    /// List of endpoint URLs (overrides config file if provided)
    #[arg(short, long)]
    urls: Vec<String>,

    /// HTTP method to use (GET, POST)
    #[arg(long, default_value = "GET")]
    method: String,

    /// Output format (human or json)
    #[arg(short = 'o', long, default_value = "human")]
    format: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let args = Args::parse();

    // Setup logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let timeout_duration = parse_duration(&args.timeout)?;
    let settlement_timeout_duration = parse_duration(&args.settlement_timeout)?;

    // Initialize Wallet if PRIVATE_KEY is present
    let wallet_manager = if let Ok(pk) = std::env::var("X402_WATCH_PRIVATE_KEY") {
        if pk.trim().is_empty() {
            None
        } else {
            let mut wm = WalletManager::new(&pk).await?;
            // Perform startup balance check
            if let Err(e) = wm.check_balances().await {
                error!("Wallet initialization failed: {}", e);
                return Err(anyhow!("Failed to initialize wallet: {}", e));
            }
            Some(wm)
        }
    } else {
        info!("X402_WATCH_PRIVATE_KEY not set. Running in dry-run mode (Step 1 only).");
        None
    };

    let config = if !args.urls.is_empty() {
        Config {
            endpoints: args.urls.into_iter().map(|u| config::Endpoint {
                name: u.clone(),
                url: u,
                method: args.method.clone(),
            }).collect(),
        }
    } else if args.config.exists() {
        let content = std::fs::read_to_string(&args.config)?;
        serde_yaml::from_str(&content)?
    } else {
        info!("No endpoints provided via flags or config file. Creating example endpoints.yaml");
        let example = Config {
            endpoints: vec![
                config::Endpoint {
                    name: "X402 Demo".to_string(),
                    url: "https://api.x402.org/demo".to_string(),
                    method: "GET".to_string(),
                }
            ],
        };
        let yaml = serde_yaml::to_string(&example)?;
        std::fs::write(&args.config, yaml)?;
        example
    };

    let checker = Checker::new(
        timeout_duration, 
        settlement_timeout_duration,
        args.verify_settlement,
        wallet_manager
    );

    if let Some(interval_str) = args.interval {
        let duration = parse_duration(&interval_str)?;
        info!("Starting periodic mode with interval: {:?}", duration);
        loop {
            run_checks(&checker, &config, &args.format).await?;
            sleep(duration).await;
        }
    } else {
        run_checks(&checker, &config, &args.format).await?;
    }

    Ok(())
}

async fn run_checks(checker: &Checker, config: &Config, format: &str) -> Result<()> {
    let mut results = Vec::new();
    for endpoint in &config.endpoints {
        let result = checker.check(&endpoint.name, &endpoint.url, &endpoint.method).await;
        results.push(result);
    }

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        print_report(&results);
    }

    Ok(())
}

fn print_report(results: &[checker::CheckResult]) {
    println!("\n--- x402 Health Report ---");
    println!("{:<20} | {:<5} | {:<25} | Message", "Endpoint", "Status", "Error Code");
    println!("{}", "-".repeat(100));
    for res in results {
        let status_str = match res.status {
            checker::CheckStatus::Pass => "PASS",
            checker::CheckStatus::Fail => "FAIL",
        };
        let error_code = res.error_code.as_deref().unwrap_or("-");
        println!("{:<20} | {:<6} | {:<25} | {}", res.name, status_str, error_code, res.message);
    }
    println!("{}\n", "-".repeat(100));
}

fn parse_duration(s: &str) -> Result<Duration> {
    let mut num_str = String::new();
    let mut unit = String::new();

    for c in s.chars() {
        if c.is_ascii_digit() {
            num_str.push(c);
        } else {
            unit.push(c);
        }
    }

    if num_str.is_empty() {
        return Err(anyhow!("Invalid duration format: {}", s));
    }

    let num: u64 = num_str.parse()?;
    match unit.to_lowercase().trim() {
        "s" | "" => Ok(Duration::from_secs(num)),
        "m" => Ok(Duration::from_secs(num * 60)),
        "h" => Ok(Duration::from_secs(num * 3600)),
        _ => Err(anyhow!("Invalid duration unit: {}. Use s, m, or h.", unit)),
    }
}
