use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer},
};
use crate::state::{Listing, STATE_OPEN};
use crate::error::VerityError;

#[derive(Accounts)]
#[instruction(start_price: u64, min_price: u64, start_ts: i64, duration: i64)]
pub struct InitializeListing<'info> {
    #[account(
        init,
        payer = seller,
        space = Listing::LEN,
        seeds = [b"listing", seller.key().as_ref(), mint.key().as_ref()],
        bump
    )]
    pub listing: Account<'info, Listing>,
    
    /// CHECK: PDA authority for vault
    #[account(
        seeds = [b"listing", seller.key().as_ref(), mint.key().as_ref()],
        bump
    )]
    pub listing_pda: UncheckedAccount<'info>,
    
    #[account(mut)]
    pub seller: Signer<'info>,
    
    #[account(
        mut,
        constraint = seller_token_account.owner == seller.key() @ VerityError::UnauthorizedSeller,
        constraint = seller_token_account.mint == mint.key() @ VerityError::UnsupportedMint,
        constraint = seller_token_account.amount == 1 @ VerityError::InvalidTokenAmount
    )]
    pub seller_token_account: Account<'info, TokenAccount>,
    
    #[account(
        init,
        payer = seller,
        associated_token::mint = mint,
        associated_token::authority = listing_pda
    )]
    pub vault_ata: Account<'info, TokenAccount>,
    
    pub mint: Account<'info, Mint>,
    
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn handler(
    ctx: Context<InitializeListing>,
    start_price: u64,
    min_price: u64,
    start_ts: i64,
    duration: i64,
) -> Result<()> {
    // Validate inputs
    require!(duration > 0, VerityError::InvalidDuration);
    require!(start_price >= min_price, VerityError::InvalidPrice);
    require!(min_price > 0, VerityError::InvalidPrice);
    
    // Verify it's a standard SPL token NFT (decimals = 0, supply = 1)
    require!(ctx.accounts.mint.decimals == 0, VerityError::UnsupportedMint);
    require!(ctx.accounts.mint.supply == 1, VerityError::UnsupportedMint);
    
    let listing = &mut ctx.accounts.listing;
    listing.seller = ctx.accounts.seller.key();
    listing.mint = ctx.accounts.mint.key();
    listing.vault_ata = ctx.accounts.vault_ata.key();
    listing.start_price = start_price;
    listing.min_price = min_price;
    listing.start_ts = start_ts;
    listing.duration = duration;
    listing.bump = ctx.bumps.listing;
    listing.state = STATE_OPEN;
    
    // Transfer NFT from seller to vault using standard token transfer
    let cpi_accounts = Transfer {
        from: ctx.accounts.seller_token_account.to_account_info(),
        to: ctx.accounts.vault_ata.to_account_info(),
        authority: ctx.accounts.seller.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
    token::transfer(cpi_ctx, 1)?;
    
    msg!(
        "Listing created: mint={}, price={}->{} over {}s",
        ctx.accounts.mint.key(),
        start_price,
        min_price,
        duration
    );
    
    Ok(())
}