use anchor_lang::prelude::*;
use crate::state::{Listing, UserVault, STATE_ACTIVE, STATE_CANCELLED};
use crate::error::VerityError;

/// Cancel listing - NFT stays in user vault (escrowless design)
#[derive(Accounts)]
pub struct CancelListing<'info> {
    /// Listing to cancel
    #[account(
        mut,
        close = seller,
        seeds = [b"listing", listing.seller.as_ref(), listing.mint.as_ref()],
        bump = listing.bump,
        constraint = listing.state == STATE_ACTIVE @ VerityError::ListingNotActive
    )]
    pub listing: Account<'info, Listing>,
    
    /// User vault - NFT remains here (no transfer needed)
    #[account(
        seeds = [b"user_vault", seller.key().as_ref(), listing.mint.as_ref()],
        bump = user_vault.bump,
        constraint = user_vault.key() == listing.user_vault @ VerityError::VaultMismatch
    )]
    pub user_vault: Account<'info, UserVault>,
    
    #[account(
        mut,
        constraint = seller.key() == listing.seller @ VerityError::UnauthorizedSeller
    )]
    pub seller: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<CancelListing>) -> Result<()> {
    let listing = &mut ctx.accounts.listing;
    
    // Mark listing as cancelled
    listing.state = STATE_CANCELLED;
    
    msg!(
        "Listing cancelled: seller={}, mint={} (NFT remains in user vault)",
        ctx.accounts.seller.key(),
        listing.mint
    );
    
    // NFT stays in user vault - seller retains control
    // Listing account closes automatically (close = seller)
    Ok(())
}