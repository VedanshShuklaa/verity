use anchor_lang::prelude::*;
use crate::state::{Attestation, AttestorState, Config, Listing, STATE_OPEN};
use crate::error::VerityError;
use anchor_spl::token::{self, CloseAccount, Token, TokenAccount, Transfer};

#[derive(Accounts)]
#[instruction(collection: Pubkey, floor: u64)]
pub struct CreateAttestation<'info> {
    /// attestor_state MUST already exist (initialize with InitializeAttestorState)
    #[account(
        mut,
        seeds = [b"attestor_state", attestor.key().as_ref()],
        bump
    )]
    pub attestor_state: Account<'info, AttestorState>,

    /// Attestation PDA derived from attestor + last_nonce (attestor_state must pre-exist)
    #[account(
        init,
        payer = attestor,
        space = Attestation::LEN,
        seeds = [
            b"attestation",
            attestor.key().as_ref(),
            &attestor_state.last_nonce.to_le_bytes()
        ],
        bump
    )]
    pub attestation: Account<'info, Attestation>,

    #[account(mut)]
    pub attestor: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, Config>,

    pub system_program: Program<'info, System>,
}

pub fn create_attestation_handler(
    ctx: Context<CreateAttestation>,
    collection: Pubkey,
    floor: u64,
) -> Result<()> {
    // ensure caller is an authorized attestor (for MVP we check authority)
    require!(
        ctx.accounts.attestor.key() == ctx.accounts.config.authority,
        VerityError::UnauthorizedAttestor
    );

    let clock = Clock::get()?;
    let attestor_state = &mut ctx.accounts.attestor_state;

    // nonce that will be used to derive the attestation PDA
    let nonce = attestor_state.last_nonce;
    // NOTE: we do NOT increment here *before* using nonce as the PDA seed.
    // We will increment AFTER the attestation is created so that the attestation's
    // nonce remains unique and monotonic.
    // Fill attestation data:
    let att = &mut ctx.accounts.attestation;
    att.attestor = ctx.accounts.attestor.key();
    att.collection = collection;
    att.floor = floor;
    att.ts = clock.unix_timestamp;
    att.nonce = nonce;
    att.used = false;

    // increment last_nonce so future attestations use the next nonce
    attestor_state.last_nonce = attestor_state.last_nonce.checked_add(1)
        .ok_or(VerityError::ArithmeticOverflow)?;

    msg!("Attestation created: collection={} floor={} nonce={}", collection, floor, nonce);
    Ok(())
}


/// Force-cancel a listing using an attestation.
/// This consumes (marks used) the attestation to prevent replays.
#[derive(Accounts)]
pub struct ForceCancelWithAttestation<'info> {
    #[account(
        mut,
        close = seller,
        seeds = [b"listing", listing.seller.as_ref(), listing.mint.as_ref()],
        bump = listing.bump,
        constraint = listing.state == STATE_OPEN @ VerityError::ListingNotOpen
    )]
    pub listing: Account<'info, Listing>,

    /// CHECK: PDA authority for vault (listing PDA)
    #[account(
        seeds = [b"listing", listing.seller.as_ref(), listing.mint.as_ref()],
        bump = listing.bump
    )]
    pub listing_pda: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = vault_ata.key() == listing.vault_ata,
        constraint = vault_ata.amount == 1
    )]
    pub vault_ata: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = seller.key() == listing.seller @ VerityError::UnauthorizedSeller
    )]
    pub seller: Signer<'info>,

    #[account(
        mut,
        constraint = seller_token_account.owner == seller.key(),
        constraint = seller_token_account.mint == listing.mint
    )]
    pub seller_token_account: Account<'info, TokenAccount>,

    /// Attestation must match the attestor and be unused
    #[account(
        mut,
        seeds = [
            b"attestation",
            attestation.attestor.as_ref(),
            &attestation.nonce.to_le_bytes()
        ],
        bump,
        constraint = attestation.attestor == config.authority @ VerityError::UnauthorizedAttestor,
        constraint = !attestation.used @ VerityError::AttestationUsed
    )]
    pub attestation: Account<'info, Attestation>,

    #[account(
        mut,
        seeds = [b"attestor_state", attestation.attestor.as_ref()],
        bump,
        constraint = attestor_state.attestor == attestation.attestor
    )]
    pub attestor_state: Account<'info, AttestorState>,

    #[account(
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, Config>,

    pub token_program: Program<'info, Token>,
}

pub fn force_cancel_handler(
    ctx: Context<ForceCancelWithAttestation>,
    collection: Pubkey,
) -> Result<()> {
    let attestation = &mut ctx.accounts.attestation;
    let clock = Clock::get()?;

    // Verify attestation is for the correct collection
    require!(
        attestation.collection == collection,
        VerityError::InvalidMetadata
    );

    // Check attestation is recent (TTL: 5 minutes)
    const ATTESTATION_TTL: i64 = 300;
    let age = clock.unix_timestamp.checked_sub(attestation.ts)
        .ok_or(VerityError::ArithmeticOverflow)?;
    require!(age <= ATTESTATION_TTL, VerityError::AttestationExpired);

    // Check floor condition
    let listing = &ctx.accounts.listing;
    require!(
        attestation.floor < listing.min_price,
        VerityError::FloorTooHigh
    );

    msg!("Force cancel triggered: floor={} < min_price={}", attestation.floor, listing.min_price);

    // mark attestation used to prevent replay
    attestation.used = true;

    // Transfer NFT from vault back to seller (CPI signed by listing PDA)
    let seeds = &[
        b"listing".as_ref(),
        listing.seller.as_ref(),
        listing.mint.as_ref(),
        &[listing.bump],
    ];
    let signer = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.vault_ata.to_account_info(),
        to: ctx.accounts.seller_token_account.to_account_info(),
        authority: ctx.accounts.listing_pda.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
    token::transfer(cpi_ctx, 1)?;

    // Close vault ATA
    let cpi_close_accounts = CloseAccount {
        account: ctx.accounts.vault_ata.to_account_info(),
        destination: ctx.accounts.seller.to_account_info(),
        authority: ctx.accounts.listing_pda.to_account_info(),
    };
    let cpi_ctx_close = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_close_accounts,
        signer,
    );
    token::close_account(cpi_ctx_close)?;

    Ok(())
}
