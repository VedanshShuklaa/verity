use anchor_lang::prelude::*;
use crate::state::Config;
use crate::error::VerityError;

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(
        init,
        payer = authority,
        space = Config::LEN,
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, Config>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<InitializeConfig>,
    fee_bps: u16,
    fee_recipient: Pubkey,
) -> Result<()> {
    require!(fee_bps <= 1000, VerityError::InvalidPrice); // Max 10% fee
    
    let config = &mut ctx.accounts.config;
    config.authority = ctx.accounts.authority.key();
    config.fee_bps = fee_bps;
    config.fee_recipient = fee_recipient;
    
    msg!("Verity marketplace initialized: fee={}bps", fee_bps);
    Ok(())
}