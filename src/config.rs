use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Endpoint {
    pub name: String,
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
}

fn default_method() -> String {
    "GET".to_string()
}
