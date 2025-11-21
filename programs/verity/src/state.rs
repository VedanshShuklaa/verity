use anchor_lang::prelude::*;

/// User-owned vault PDA (escrowless architecture)
/// Seeds: [b"user_vault", seller, mint]
/// The vault belongs to the USER, not the marketplace
#[account]
pub struct UserVault {
    pub owner: Pubkey,           // User who owns this vault
    pub mint: Pubkey,            // NFT mint stored in this vault
    pub vault_ata: Pubkey,       // ATA holding the NFT
    pub bump: u8,
}

impl UserVault {
    pub const LEN: usize = 8 +   // discriminator
        32 +                      // owner
        32 +                      // mint
        32 +                      // vault_ata
        1;                        // bump
}

/// Listing references the user vault, doesn't custody the NFT
#[account]
pub struct Listing {
    pub seller: Pubkey,
    pub mint: Pubkey,
    pub user_vault: Pubkey,      // Reference to UserVault, not direct custody
    pub price_config: PriceConfig,
    pub conditions: ListingConditions,
    pub state: u8,               // 0 = Active, 1 = Cancelled, 2 = Sold
    pub bump: u8,
}

impl Listing {
    pub const LEN: usize = 8 +   // discriminator
        32 +                      // seller
        32 +                      // mint
        32 +                      // user_vault
        PriceConfig::LEN +
        ListingConditions::LEN +
        1 +                       // state
        1;                        // bump
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct PriceConfig {
    pub price_type: PriceType,
    pub start_price: u64,
    pub min_price: u64,
    pub start_ts: i64,
    pub duration: i64,
}

impl PriceConfig {
    pub const LEN: usize = 1 +   // price_type
        8 +                       // start_price
        8 +                       // min_price
        8 +                       // start_ts
        8;                        // duration
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PriceType {
    Fixed,                        // Constant price
    LinearDecay,                  // start_price → min_price linearly
    Exponential,                  // Exponential decay (future)
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct ListingConditions {
    pub min_floor: Option<u64>,   // Pyth oracle floor validation
    pub valid_from: Option<i64>,  // Time window start
    pub valid_until: Option<i64>, // Time window end
}

impl ListingConditions {
    pub const LEN: usize = 9 +   // min_floor (1 + 8)
        9 +                       // valid_from (1 + 8)
        9;                        // valid_until (1 + 8)
}

#[account]
pub struct Config {
    pub authority: Pubkey,
    pub fee_bps: u16,
    pub fee_recipient: Pubkey,
}

impl Config {
    pub const LEN: usize = 8 + 32 + 2 + 32;
}

// Listing state constants
pub const STATE_ACTIVE: u8 = 0;
pub const STATE_CANCELLED: u8 = 1;
pub const STATE_SOLD: u8 = 2;

/// Calculate current price based on price configuration
pub fn calculate_price(config: &PriceConfig, current_ts: i64) -> u64 {
    match config.price_type {
        PriceType::Fixed => config.start_price,
        
        PriceType::LinearDecay => {
            if current_ts <= config.start_ts {
                return config.start_price;
            }
            
            let elapsed = current_ts.saturating_sub(config.start_ts);
            if elapsed >= config.duration {
                return config.min_price;
            }
            
            // Linear: price = start - ((start - min) * elapsed / duration)
            let diff = config.start_price.saturating_sub(config.min_price);
            let drop = (diff as u128)
                .saturating_mul(elapsed as u128)
                .saturating_div(config.duration as u128) as u64;
            
            config.start_price.saturating_sub(drop).max(config.min_price)
        }
        
        PriceType::Exponential => {
            // Future: exponential decay implementation
            config.start_price
        }
    }
}

/// Validate listing conditions (floor price, time window)
pub fn validate_conditions(
    conditions: &ListingConditions,
    current_ts: i64,
    _pyth_price: Option<u64>, // Future: Pyth integration
) -> Result<()> {
    // Time window validation
    if let Some(valid_from) = conditions.valid_from {
        require!(
            current_ts >= valid_from,
            crate::error::VerityError::ListingNotYetValid
        );
    }
    
    if let Some(valid_until) = conditions.valid_until {
        require!(
            current_ts <= valid_until,
            crate::error::VerityError::ListingExpired
        );
    }
    
    // Floor price validation (Pyth integration placeholder)
    if let Some(_min_floor) = conditions.min_floor {
        // Future: validate against Pyth oracle
        // require!(pyth_price.unwrap_or(0) >= min_floor, FloorTooLow);
    }
    
    Ok(())
}