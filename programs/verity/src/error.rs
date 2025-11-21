use anchor_lang::prelude::*;

#[error_code]
pub enum VerityError {
    #[msg("Listing is not active")]
    ListingNotActive,
    
    #[msg("Listing has not started yet")]
    ListingNotYetValid,
    
    #[msg("Only the vault owner can perform this action")]
    UnauthorizedVaultOwner,
    
    #[msg("Only the seller can perform this action")]
    UnauthorizedSeller,
    
    #[msg("Invalid price configuration")]
    InvalidPrice,
    
    #[msg("Duration must be positive for decay pricing")]
    InvalidDuration,
    
    #[msg("Token account does not contain exactly 1 NFT")]
    InvalidTokenAmount,
    
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
    
    #[msg("Unsupported mint type (must be standard SPL token NFT)")]
    UnsupportedMint,
    
    #[msg("Listing has expired")]
    ListingExpired,
    
    #[msg("User vault already exists for this mint")]
    VaultAlreadyExists,
    
    #[msg("User vault does not match listing")]
    VaultMismatch,
    
    #[msg("Floor price below minimum threshold")]
    FloorTooLow,
    
    #[msg("Invalid time window configuration")]
    InvalidTimeWindow,
    
    #[msg("Vault is currently locked by an active listing")]
    VaultLocked,
    
    #[msg("NFT not in user vault")]
    NftNotInVault,
}