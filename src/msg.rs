use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::{Uint128, Binary};
use crate::state::{Config, ProcessedDeposit, Withdrawal};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InstantiateMsg {
    pub owner: String,
    pub backend_wallet: String,
    pub registry_contract: String,
    pub registry_hash: String,
    pub fee_basis_points: u16,
    pub fee_enabled: bool,
    pub bridge_enabled: bool,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    ProcessDeposit {
        recipient: String,
        amount: Uint128,
        monero_tx_id: String,
    },
    Receive {
        sender: String,
        from: String,
        amount: Uint128,
        memo: Option<String>,
        msg: Binary,
    },
    UpdateConfig { config: Config },
    CompleteWithdrawal { withdrawal_id: u64 },
    RegisterDepositAddress {},
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveMsg {
    Withdraw {
        monero_address: String,
    },
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetConfig {},
    IsProcessed { monero_tx_id: String },
    GetDeposit { monero_tx_id: String },
    GetPendingWithdrawals { address: String },
    GetCompletedWithdrawals { address: String },
    GetDepositIndex { address: String },
    GetAllDepositIndices {
        start_after: Option<String>,
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ConfigResponse {
    pub config: Config,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct IsProcessedResponse {
    pub is_processed: bool,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DepositResponse {
    pub deposit: Option<ProcessedDeposit>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct WithdrawalsResponse {
    pub withdrawals: Vec<Withdrawal>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct RegisterDepositAddressResponse {
    pub index: u32,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DepositIndexResponse {
    pub index: Option<u32>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AllDepositIndicesResponse {
    pub indices: Vec<(String, u32)>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrateMsg {
    Migrate {
        registry_contract: String,
        registry_hash: String,
    },
    Upgrade {},
}

// SNIP-20 messages for calling the XMR token contract
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Snip20ExecuteMsg {
    Mint {
        recipient: String,
        amount: Uint128,
        memo: Option<String>,
        padding: Option<String>,
    },
    Burn {
        amount: Uint128,
        memo: Option<String>,
        padding: Option<String>,
    },
    Send {
        recipient: String,
        recipient_code_hash: String,
        amount: Uint128,
        msg: Option<Binary>,
        memo: Option<String>,
        padding: Option<String>,
    },
}

// Message for the exchange contract receive callback
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeReceiveMsg {
    SwapToErthAndBurn {},
}
