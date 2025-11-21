use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod state;

use instructions::*;
use state::PriceType;

declare_id!("2pw3GT51qRHmrobvzmBzso2iHCBv5rN8MTNTNMxEyG2P");

#[program]
pub mod verity {
    use super::*;

    /// Initialize marketplace configuration
    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        fee_bps: u16,
        fee_recipient: Pubkey,
    ) -> Result<()> {
        initialize_config::handler(ctx, fee_bps, fee_recipient)
    }

    /// Create user-owned vault for NFT (escrowless architecture)
    pub fn initialize_user_vault(
        ctx: Context<InitializeUserVault>,
    ) -> Result<()> {
        initialize_user_vault::handler(ctx)
    }

    /// Create listing that references user vault (doesn't custody NFT)
    pub fn create_listing(
        ctx: Context<CreateListing>,
        price_type: PriceType,
        start_price: u64,
        min_price: u64,
        start_ts: i64,
        duration: i64,
        min_floor: Option<u64>,
        valid_from: Option<i64>,
        valid_until: Option<i64>,
    ) -> Result<()> {
        create_listing::handler(
            ctx,
            price_type,
            start_price,
            min_price,
            start_ts,
            duration,
            min_floor,
            valid_from,
            valid_until,
        )
    }

    /// Purchase NFT atomically from user vault
    pub fn buy_now(ctx: Context<BuyNow>) -> Result<()> {
        buy_now::handler(ctx)
    }

    /// Cancel listing (NFT stays in user vault)
    pub fn cancel_listing(ctx: Context<CancelListing>) -> Result<()> {
        cancel_listing::handler(ctx)
    }

    /// Withdraw NFT from user vault (when no active listing)
    pub fn withdraw_from_vault(
        ctx: Context<WithdrawFromVault>,
    ) -> Result<()> {
        withdraw_from_vault::handler(ctx)
    }
}