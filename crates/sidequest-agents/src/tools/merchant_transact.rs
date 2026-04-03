//! Merchant transaction validation tool (ADR-057 Phase 3).
//!
//! Validates a merchant_transact tool call from the narrator sidecar.
//! Accepts "buy" or "sell" transaction types with item ID and merchant name.
//! Rejects empty/whitespace fields and invalid transaction types.
//! Produces `MerchantTransactionExtracted` for the orchestrator layer.

use crate::orchestrator::MerchantTransactionExtracted;

/// Validated result of a `merchant_transact` tool call.
///
/// Fields are private with getters to enforce validation invariants
/// (non-empty, trimmed, valid type). Constructed only through `validate_merchant_transact`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MerchantTransactResult {
    transaction_type: String,
    item_id: String,
    merchant: String,
}

impl MerchantTransactResult {
    /// The transaction type — "buy" or "sell" (normalized to lowercase).
    pub fn transaction_type(&self) -> &str {
        &self.transaction_type
    }

    /// The item identifier (snake_case name matching inventory).
    pub fn item_id(&self) -> &str {
        &self.item_id
    }

    /// The merchant NPC name.
    pub fn merchant(&self) -> &str {
        &self.merchant
    }

    /// Convert to `MerchantTransactionExtracted` for the orchestrator layer.
    pub fn to_merchant_transaction_extracted(&self) -> MerchantTransactionExtracted {
        MerchantTransactionExtracted {
            transaction_type: self.transaction_type.clone(),
            item_id: self.item_id.clone(),
            merchant: self.merchant.clone(),
        }
    }
}

/// Error returned when a merchant_transact tool call has invalid fields.
#[derive(Debug, thiserror::Error)]
#[error("invalid merchant_transact: {0}")]
pub struct InvalidMerchantTransact(String);

/// Validate a merchant_transact tool call.
///
/// `transaction_type` must be "buy" or "sell" (case-insensitive).
/// `item_id` and `merchant` must be non-empty after trimming.
#[tracing::instrument(name = "tool.merchant_transact", skip_all, fields(
    transaction_type = %transaction_type,
    item_id = %item_id,
    merchant = %merchant,
))]
pub fn validate_merchant_transact(
    transaction_type: &str,
    item_id: &str,
    merchant: &str,
) -> Result<MerchantTransactResult, InvalidMerchantTransact> {
    let transaction_type = transaction_type.trim().to_lowercase();
    let item_id = item_id.trim();
    let merchant = merchant.trim();

    if transaction_type.is_empty() {
        tracing::warn!(valid = false, "transaction_type is empty");
        return Err(InvalidMerchantTransact("transaction_type is empty".to_string()));
    }
    if transaction_type != "buy" && transaction_type != "sell" {
        tracing::warn!(valid = false, tx_type = %transaction_type, "invalid transaction_type");
        return Err(InvalidMerchantTransact(format!(
            "transaction_type must be 'buy' or 'sell', got '{transaction_type}'"
        )));
    }
    if item_id.is_empty() {
        tracing::warn!(valid = false, "item_id is empty");
        return Err(InvalidMerchantTransact("item_id is empty".to_string()));
    }
    if merchant.is_empty() {
        tracing::warn!(valid = false, "merchant is empty");
        return Err(InvalidMerchantTransact("merchant is empty".to_string()));
    }

    let result = MerchantTransactResult {
        transaction_type,
        item_id: item_id.to_string(),
        merchant: merchant.to_string(),
    };

    tracing::info!(
        valid = true,
        transaction_type = result.transaction_type(),
        item_id = result.item_id(),
        merchant = result.merchant(),
        "merchant_transact validated"
    );

    Ok(result)
}
