use anchor_lang::prelude::*;
use anchor_spl::token::{self, CloseAccount, Token, TokenAccount, Transfer};
use crate::state::UserVault;
use crate::error::VerityError;

/// Withdraw NFT from user vault back to owner
/// Can only be done when no active listing exists
#[derive(Accounts)]
pub struct WithdrawFromVault<'info> {
    /// User vault
    #[account(
        mut,
        close = owner,
        seeds = [b"user_vault", owner.key().as_ref(), user_vault.mint.as_ref()],
        bump = user_vault.bump,
        constraint = user_vault.owner == owner.key() @ VerityError::UnauthorizedVaultOwner
    )]
    pub user_vault: Account<'info, UserVault>,
    
    /// Vault PDA authority
    /// CHECK: PDA signer
    #[account(
        seeds = [b"user_vault", owner.key().as_ref(), user_vault.mint.as_ref()],
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
    pub owner: Signer<'info>,
    
    /// Owner's token account to receive NFT
    #[account(
        mut,
        constraint = owner_token_account.owner == owner.key() @ VerityError::UnauthorizedVaultOwner,
        constraint = owner_token_account.mint == user_vault.mint @ VerityError::VaultMismatch
    )]
    pub owner_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<WithdrawFromVault>) -> Result<()> {
    let user_vault = &ctx.accounts.user_vault;
    
    // Transfer NFT from vault back to owner
    let seeds = &[
        b"user_vault",
        user_vault.owner.as_ref(),
        user_vault.mint.as_ref(),
        &[user_vault.bump],
    ];
    let signer = &[&seeds[..]];
    
    let cpi_accounts = Transfer {
        from: ctx.accounts.vault_ata.to_account_info(),
        to: ctx.accounts.owner_token_account.to_account_info(),
        authority: ctx.accounts.vault_pda.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer,
    );
    token::transfer(cpi_ctx, 1)?;
    
    // Close vault ATA
    let cpi_close = CloseAccount {
        account: ctx.accounts.vault_ata.to_account_info(),
        destination: ctx.accounts.owner.to_account_info(),
        authority: ctx.accounts.vault_pda.to_account_info(),
    };
    let cpi_ctx_close = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_close,
        signer,
    );
    token::close_account(cpi_ctx_close)?;
    
    msg!(
        "NFT withdrawn from vault: owner={}, mint={}",
        ctx.accounts.owner.key(),
        user_vault.mint
    );
    
    // User vault closes automatically (close = owner)
    Ok(())
}