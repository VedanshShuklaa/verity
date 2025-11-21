use anchor_lang::prelude::*;
use crate::state::AttestorState;
use crate::error::VerityError;

#[derive(Accounts)]
pub struct InitializeAttestorState<'info> {
    #[account(
        init,
        payer = attestor,
        space = AttestorState::LEN,
        seeds = [b"attestor_state", attestor.key().as_ref()],
        bump
    )]
    pub attestor_state: Account<'info, AttestorState>,

    #[account(mut)]
    pub attestor: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitializeAttestorState>) -> Result<()> {
    let st = &mut ctx.accounts.attestor_state;
    // safe guard just in case: ensure uninitialized
    require!(
        st.attestor == Pubkey::default(),
        VerityError::AlreadyInitialized
    );

    st.attestor = ctx.accounts.attestor.key();
    st.last_nonce = 0u64;

    msg!("AttestorState initialized for {}", st.attestor);
    Ok(())
}
