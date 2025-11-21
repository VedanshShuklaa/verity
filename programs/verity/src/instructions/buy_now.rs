use anchor_lang::prelude::*;
use anchor_lang::solana_program::{program::invoke, system_instruction};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer},
};
use crate::state::{
    Config, Listing, UserVault, STATE_ACTIVE, STATE_SOLD,
    calculate_price, validate_conditions
};
use crate::error::VerityError;

#[derive(Accounts)]
pub struct BuyNow<'info> {
    /// Listing being purchased
    #[account(
        mut,
        close = seller,
        seeds = [b"listing", listing.seller.as_ref(), listing.mint.as_ref()],
        bump = listing.bump,
        constraint = listing.state == STATE_ACTIVE @ VerityError::ListingNotActive
    )]
    pub listing: Account<'info, Listing>,
    
    /// User vault referenced by listing
    #[account(
        seeds = [b"user_vault", listing.seller.as_ref(), listing.mint.as_ref()],
        bump = user_vault.bump,
        constraint = user_vault.key() == listing.user_vault @ VerityError::VaultMismatch
    )]
    pub user_vault: Account<'info, UserVault>,
    
    /// Vault PDA authority
    /// CHECK: PDA signer for vault ATA
    #[account(
        seeds = [b"user_vault", listing.seller.as_ref(), listing.mint.as_ref()],
        bump = user_vault.bump
    )]
    pub vault_pda: UncheckedAccount<'info>,
    
    /// Vault ATA holding the NFT
    #[account(
        mut,
        constraint = vault_ata.key() == user_vault.vault_ata @ VerityError::VaultMismatch,
        constraint = vault_ata.amount == 1 @ VerityError::InvalidTokenAmount
    )]
    pub vault_ata: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub buyer: Signer<'info>,
    
    /// Buyer's ATA to receive NFT
    #[account(
        init,
        payer = buyer,
        associated_token::mint = mint,
        associated_token::authority = buyer
    )]
    pub buyer_ata: Account<'info, TokenAccount>,
    
    /// Seller receives payment
    /// CHECK: Validated via listing.seller
    #[account(
        mut,
        constraint = seller.key() == listing.seller @ VerityError::UnauthorizedSeller
    )]
    pub seller: UncheckedAccount<'info>,
    
    pub mint: Account<'info, Mint>,
    
    #[account(
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, Config>,
    
    /// Fee recipient
    /// CHECK: Validated via config.fee_recipient
    #[account(
        mut,
        constraint = fee_recipient.key() == config.fee_recipient
    )]
    pub fee_recipient: UncheckedAccount<'info>,
    
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn handler(ctx: Context<BuyNow>) -> Result<()> {
    let listing = &mut ctx.accounts.listing;
    let clock = Clock::get()?;
    
    // Validate listing conditions (time window, floor price)
    validate_conditions(&listing.conditions, clock.unix_timestamp, None)?;
    
    // Calculate current price
    let price = calculate_price(&listing.price_config, clock.unix_timestamp);
    
    msg!("Purchase price: {} lamports at timestamp {}", price, clock.unix_timestamp);
    
    // Calculate fees
    let marketplace_fee = (price as u128)
        .checked_mul(ctx.accounts.config.fee_bps as u128)
        .ok_or(VerityError::ArithmeticOverflow)?
        .checked_div(10000)
        .ok_or(VerityError::ArithmeticOverflow)? as u64;
    
    // Simplified royalty (5% - in production, parse metadata)
    let royalty_bps = 500u64;
    let royalty = (price as u128)
        .checked_mul(royalty_bps as u128)
        .ok_or(VerityError::ArithmeticOverflow)?
        .checked_div(10000)
        .ok_or(VerityError::ArithmeticOverflow)? as u64;
    
    let seller_amount = price
        .checked_sub(marketplace_fee)
        .ok_or(VerityError::ArithmeticOverflow)?
        .checked_sub(royalty)
        .ok_or(VerityError::ArithmeticOverflow)?;
    
    msg!(
        "Payment breakdown: price={}, fee={}, royalty={}, seller={}",
        price, marketplace_fee, royalty, seller_amount
    );
    
    // Transfer SOL to seller
    if seller_amount > 0 {
        invoke(
            &system_instruction::transfer(
                ctx.accounts.buyer.key,
                ctx.accounts.seller.key,
                seller_amount,
            ),
            &[
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.seller.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;
    }
    
    // Transfer marketplace fee
    if marketplace_fee > 0 {
        invoke(
            &system_instruction::transfer(
                ctx.accounts.buyer.key,
                ctx.accounts.fee_recipient.key,
                marketplace_fee,
            ),
            &[
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.fee_recipient.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;
    }
    
    // Transfer royalties (simplified - send to seller)
    if royalty > 0 {
        invoke(
            &system_instruction::transfer(
                ctx.accounts.buyer.key,
                ctx.accounts.seller.key,
                royalty,
            ),
            &[
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.seller.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;
    }
    
    // Transfer NFT from vault to buyer (signed by vault PDA)
    let user_vault = &ctx.accounts.user_vault;
    let seeds = &[
        b"user_vault",
        user_vault.owner.as_ref(),
        user_vault.mint.as_ref(),
        &[user_vault.bump],
    ];
    let signer = &[&seeds[..]];
    
    let cpi_accounts = Transfer {
        from: ctx.accounts.vault_ata.to_account_info(),
        to: ctx.accounts.buyer_ata.to_account_info(),
        authority: ctx.accounts.vault_pda.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer,
    );
    token::transfer(cpi_ctx, 1)?;
    
    // Mark listing as sold
    listing.state = STATE_SOLD;
    
    msg!(
        "Purchase completed: buyer={}, seller={}, price={}",
        ctx.accounts.buyer.key(),
        ctx.accounts.seller.key(),
        price
    );
    
    // Listing account closes automatically (close = seller)
    Ok(())
}