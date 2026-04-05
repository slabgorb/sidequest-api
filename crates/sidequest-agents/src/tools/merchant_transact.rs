//! Merchant transaction validation tool (ADR-057 Phase 3).
//!
//! Validates a merchant transaction against the merchant's inventory.
//! Replaces the narrator's `merchant_transactions` JSON field with a
//! typed tool call.

use crate::orchestrator::MerchantTransactionExtracted;

/// Input for the `merchant_transact` tool call.
#[derive(Debug, Clone)]
pub struct MerchantTransactInput {
    /// Transaction type: "buy" or "sell".
    pub transaction_type: String,
    /// Item identifier (snake_case name matching inventory).
    pub item_id: String,
    /// Merchant NPC name.
    pub merchant: String,
}

/// Error returned when a merchant transaction is invalid.
#[derive(Debug, thiserror::Error)]
pub enum MerchantTransactError {
    /// Transaction type is not "buy" or "sell".
    #[error("invalid transaction type: \"{0}\" — expected \"buy\" or \"sell\"")]
    InvalidTransactionType(String),
    /// Item ID is empty.
    #[error("item_id must not be empty")]
    EmptyItemId,
    /// Merchant name is empty.
    #[error("merchant name must not be empty")]
    EmptyMerchant,
    /// Item not found in merchant's inventory (buy only).
    #[error("item \"{0}\" not found in merchant inventory")]
    ItemNotInInventory(String),
}

/// Validate a merchant transaction and produce a `MerchantTransactionExtracted`.
///
/// For "buy" transactions, the `item_id` must exist in the merchant's inventory.
/// For "sell" transactions, inventory validation is relaxed (merchants can accept
/// items they don't currently stock).
#[tracing::instrument(name = "tool.merchant_transact", skip_all, fields(
    transaction_type = %input.transaction_type,
    item_id = %input.item_id,
    merchant = %input.merchant,
))]
pub fn transact_merchant(
    input: MerchantTransactInput,
    merchant_inventory: &Vec<String>,
) -> Result<MerchantTransactionExtracted, MerchantTransactError> {
    // Validate transaction type
    if input.transaction_type != "buy" && input.transaction_type != "sell" {
        tracing::warn!(valid = false, "merchant transaction rejected: invalid type");
        return Err(MerchantTransactError::InvalidTransactionType(
            input.transaction_type,
        ));
    }

    // Validate item_id
    if input.item_id.is_empty() {
        tracing::warn!(valid = false, "merchant transaction rejected: empty item_id");
        return Err(MerchantTransactError::EmptyItemId);
    }

    // Validate merchant name
    if input.merchant.is_empty() {
        tracing::warn!(valid = false, "merchant transaction rejected: empty merchant");
        return Err(MerchantTransactError::EmptyMerchant);
    }

    // For buy transactions, validate item exists in merchant inventory
    if input.transaction_type == "buy" && !merchant_inventory.contains(&input.item_id) {
        tracing::warn!(
            valid = false,
            item_id = %input.item_id,
            "merchant transaction rejected: item not in inventory"
        );
        return Err(MerchantTransactError::ItemNotInInventory(input.item_id));
    }

    let txn = MerchantTransactionExtracted {
        transaction_type: input.transaction_type,
        item_id: input.item_id,
        merchant: input.merchant,
    };

    tracing::info!(
        valid = true,
        transaction_type = %txn.transaction_type,
        item_id = %txn.item_id,
        merchant = %txn.merchant,
        "merchant transaction validated"
    );

    Ok(txn)
}
