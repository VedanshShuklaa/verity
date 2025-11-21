use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;
use crate::state::{
    Listing, UserVault, PriceConfig, PriceType, ListingConditions, STATE_ACTIVE
};
use crate::error::VerityError;

/// Create listing that REFERENCES user vault (doesn't custody NFT)
#[derive(Accounts)]
pub struct CreateListing<'info> {
    /// Listing PDA - stores listing metadata only, not the NFT
    #[account(
        init,
        payer = seller,
        space = Listing::LEN,
        seeds = [b"listing", seller.key().as_ref(), mint.key().as_ref()],
        bump
    )]
    pub listing: Account<'info, Listing>,
    
    /// User vault must already exist and be owned by seller
    #[account(
        seeds = [b"user_vault", seller.key().as_ref(), mint.key().as_ref()],
        bump = user_vault.bump,
        constraint = user_vault.owner == seller.key() @ VerityError::UnauthorizedVaultOwner,
        constraint = user_vault.mint == mint.key() @ VerityError::VaultMismatch
    )]
    pub user_vault: Account<'info, UserVault>,
    
    /// Vault ATA must contain the NFT
    #[account(
        constraint = vault_ata.key() == user_vault.vault_ata @ VerityError::VaultMismatch,
        constraint = vault_ata.amount == 1 @ VerityError::NftNotInVault
    )]
    pub vault_ata: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub seller: Signer<'info>,
    
    /// CHECK: Mint is validated via user_vault
    pub mint: UncheckedAccount<'info>,
    
    pub system_program: Program<'info, System>,
}

pub fn handler(
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
    // Validate price configuration
    require!(start_price > 0, VerityError::InvalidPrice);
    require!(min_price > 0, VerityError::InvalidPrice);
    require!(start_price >= min_price, VerityError::InvalidPrice);
    
    // Validate duration for decay pricing
    if price_type != PriceType::Fixed {
        require!(duration > 0, VerityError::InvalidDuration);
    }
    
    // Validate time window
    if let (Some(from), Some(until)) = (valid_from, valid_until) {
        require!(from < until, VerityError::InvalidTimeWindow);
    }
    
    let listing = &mut ctx.accounts.listing;
    listing.seller = ctx.accounts.seller.key();
    listing.mint = ctx.accounts.user_vault.mint;
    listing.user_vault = ctx.accounts.user_vault.key();
    
    // Price configuration
    listing.price_config = PriceConfig {
        price_type,
        start_price,
        min_price,
        start_ts,
        duration,
    };
    
    // Conditional listing features
    listing.conditions = ListingConditions {
        min_floor,
        valid_from,
        valid_until,
    };
    
    listing.state = STATE_ACTIVE;
    listing.bump = ctx.bumps.listing;
    
    msg!(
        "Listing created: seller={}, mint={}, type={:?}, start_price={}, min_price={}",
        ctx.accounts.seller.key(),
        ctx.accounts.user_vault.mint,
        price_type,
        start_price,
        min_price
    );
    
    if let Some(floor) = min_floor {
        msg!("Floor protection: min_floor={}", floor);
    }
    
    Ok(())
}