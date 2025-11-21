use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer},
};
use crate::state::UserVault;
use crate::error::VerityError;

/// Initialize a user-owned vault for NFT storage
/// This vault belongs to the USER, not the marketplace
#[derive(Accounts)]
pub struct InitializeUserVault<'info> {
    /// User vault PDA - owned by the USER via seeds
    #[account(
        init,
        payer = owner,
        space = UserVault::LEN,
        seeds = [b"user_vault", owner.key().as_ref(), mint.key().as_ref()],
        bump
    )]
    pub user_vault: Account<'info, UserVault>,
    
    /// Vault PDA authority (derived from user_vault seeds)
    /// CHECK: PDA signer for vault ATA
    #[account(
        seeds = [b"user_vault", owner.key().as_ref(), mint.key().as_ref()],
        bump
    )]
    pub vault_pda: UncheckedAccount<'info>,
    
    #[account(mut)]
    pub owner: Signer<'info>,
    
    /// User's token account holding the NFT
    #[account(
        mut,
        constraint = owner_token_account.owner == owner.key() @ VerityError::UnauthorizedVaultOwner,
        constraint = owner_token_account.mint == mint.key() @ VerityError::UnsupportedMint,
        constraint = owner_token_account.amount == 1 @ VerityError::InvalidTokenAmount
    )]
    pub owner_token_account: Account<'info, TokenAccount>,
    
    /// Vault's ATA - holds the NFT while listed
    #[account(
        init,
        payer = owner,
        associated_token::mint = mint,
        associated_token::authority = vault_pda
    )]
    pub vault_ata: Account<'info, TokenAccount>,
    
    pub mint: Account<'info, Mint>,
    
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn handler(ctx: Context<InitializeUserVault>) -> Result<()> {
    // Verify NFT standard (decimals = 0, supply = 1)
    require!(ctx.accounts.mint.decimals == 0, VerityError::UnsupportedMint);
    require!(ctx.accounts.mint.supply == 1, VerityError::UnsupportedMint);
    
    // Initialize user vault
    let vault = &mut ctx.accounts.user_vault;
    vault.owner = ctx.accounts.owner.key();
    vault.mint = ctx.accounts.mint.key();
    vault.vault_ata = ctx.accounts.vault_ata.key();
    vault.bump = ctx.bumps.user_vault;
    
    // Transfer NFT from owner to vault
    let cpi_accounts = Transfer {
        from: ctx.accounts.owner_token_account.to_account_info(),
        to: ctx.accounts.vault_ata.to_account_info(),
        authority: ctx.accounts.owner.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
    token::transfer(cpi_ctx, 1)?;
    
    msg!(
        "User vault created: owner={}, mint={}, vault_ata={}",
        ctx.accounts.owner.key(),
        ctx.accounts.mint.key(),
        ctx.accounts.vault_ata.key()
    );
    
    Ok(())
}