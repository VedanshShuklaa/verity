# Verity

An escrowless NFT marketplace on Solana. Your NFTs stay in vaults you control.

## The Problem

Most NFT marketplaces work like this: you list an NFT, it gets transferred to an escrow account the marketplace controls. If that marketplace gets hacked, has a bug, or shuts down, your NFT could be gone.

```
Traditional Marketplace:

  You list NFT
       |
       v
  NFT transfers to marketplace escrow
       |
       v
  Marketplace controls your asset
       |
       v
  You trust them not to get hacked, 
  have bugs, or shut down
```

## How Verity Works

When you list an NFT on Verity, it goes into a vault derived from your wallet address. The marketplace program can facilitate sales, but it never has custody of your assets. If Verity disappeared tomorrow, you'd still own your vault and could withdraw your NFT.

```
Verity:

  You create a vault (one-time)
       |
       v
  NFT transfers to YOUR vault PDA
  (derived from your wallet + NFT mint)
       |
       v
  You create a listing (just rules, no transfer)
       |
       v
  Buyer purchases -> NFT goes from your vault to them
       |
  OR   v
  You cancel -> NFT already in your vault, nothing to do
```

The key difference: the vault PDA is derived from `[b"user_vault", your_pubkey, nft_mint]`. You control it, not us.

## User Flow

### Listing an NFT

```
Step 1: Create Vault
┌─────────────┐      ┌─────────────────────┐
│ Your Wallet │ ---> │ Your Vault PDA      │
│ (has NFT)   │      │ (you control this)  │
└─────────────┘      └─────────────────────┘
     NFT moves here once, stays here

Step 2: Create Listing  
┌─────────────────────┐      ┌─────────────────┐
│ Your Vault PDA      │ <--- │ Listing Account │
│ (still has NFT)     │      │ (just rules)    │
└─────────────────────┘      └─────────────────┘
     No transfer happens - listing just references the vault
```

### Buying an NFT

```
┌─────────────┐   SOL   ┌─────────────┐
│   Buyer     │ ------> │   Seller    │
└─────────────┘         └─────────────┘
       ^                       
       |  NFT                  
       |                       
┌─────────────────────┐        
│ Seller's Vault PDA  │        
└─────────────────────┘        

All happens atomically in one transaction.
Fees deducted, royalties paid, NFT transferred.
```

### Cancelling a Listing

```
┌─────────────────┐      ┌─────────────────────┐
│ Listing Account │      │ Your Vault PDA      │
│ (closes)        │      │ (NFT still here)    │
└─────────────────┘      └─────────────────────┘

Nothing to transfer. Your NFT never left your vault.
```

### Withdrawing from Vault

```
┌─────────────────────┐      ┌─────────────┐
│ Your Vault PDA      │ ---> │ Your Wallet │
│ (closes, rent back) │      │ (NFT back)  │
└─────────────────────┘      └─────────────┘

Only works when no active listing exists.
```

## Why This Matters

| Scenario | Traditional Marketplace | Verity |
|----------|------------------------|--------|
| Marketplace gets hacked | Your NFT at risk | Your vault unaffected |
| Marketplace shuts down | NFT stuck in escrow | You still own your vault |
| You want to cancel | Pay gas to transfer back | Instant, no transfer |
| Bug in marketplace code | Could lose NFT | Vault is separate from listings |

## Features

**What's implemented:**

- User-owned vault system
- Fixed price listings
- Linear price decay (Dutch auction style)
- Time-windowed listings (valid_from / valid_until)
- Listing cancellation without NFT transfer
- Marketplace fee collection

**What's not implemented yet:**

- Pyth oracle floor price validation (placeholder exists)
- Exponential price curves
- Collection-wide offers
- Reading royalties from metadata (hardcoded at 5%)

## Program Instructions

| Instruction | Description |
|-------------|-------------|
| `initialize_config` | One-time marketplace setup (fee %, recipient) |
| `initialize_user_vault` | Create vault and deposit NFT |
| `create_listing` | Create listing referencing your vault |
| `buy_now` | Purchase NFT at current price |
| `cancel_listing` | Cancel listing (NFT stays in vault) |
| `withdraw_from_vault` | Reclaim NFT when no active listing |

## Listing Options

When creating a listing, you can configure:

```rust
price_type: Fixed | LinearDecay
start_price: u64
min_price: u64            // For decay listings
duration: i64             // Seconds until min_price reached
valid_from: Option<i64>   // Optional start time
valid_until: Option<i64>  // Optional end time
```

**Fixed pricing:** NFT sells at `start_price` until cancelled or sold.

**Linear decay:** Price starts at `start_price` and decreases linearly to `min_price` over `duration` seconds. Good for price discovery.

## Usage

### Deploy

```bash
anchor build
anchor deploy --provider.cluster devnet
```

### Initialize marketplace (one-time)

```typescript
await program.methods
  .initializeConfig(250, feeRecipientPubkey) // 2.5% fee
  .accounts({ config: configPDA, authority: wallet.publicKey })
  .rpc();
```

### List an NFT

```typescript
// 1. Create vault (deposits NFT)
await program.methods.initializeUserVault()
  .accounts({ userVault, owner, mint, ownerTokenAccount, vaultAta })
  .rpc();

// 2. Create listing
await program.methods.createListing(
  { linearDecay: {} },
  new BN(2 * LAMPORTS_PER_SOL),  // start price
  new BN(1 * LAMPORTS_PER_SOL),  // min price
  new BN(Math.floor(Date.now() / 1000)),
  new BN(86400),  // 24 hours
  null, null, null  // optional conditions
)
.accounts({ listing, userVault, vaultAta, seller, mint })
.rpc();
```

### Buy an NFT

```typescript
await program.methods.buyNow()
  .accounts({ 
    listing, userVault, vaultPda, vaultAta, 
    buyer, buyerAta, seller, mint, config, feeRecipient 
  })
  .rpc();
```

### Cancel listing

```typescript
await program.methods.cancelListing()
  .accounts({ listing, userVault, seller })
  .rpc();
// NFT remains in your vault
```

### Withdraw from vault

```typescript
await program.methods.withdrawFromVault()
  .accounts({ userVault, vaultPda, vaultAta, owner, ownerTokenAccount })
  .rpc();
// NFT returns to your wallet, vault closes, rent refunded
```

## Project Structure

```
programs/verity/src/
├── lib.rs                    # Program entrypoint
├── state.rs                  # Account structures, price calculation
├── error.rs                  # Error definitions
└── instructions/
    ├── initialize_config.rs
    ├── initialize_user_vault.rs
    ├── create_listing.rs
    ├── buy_now.rs
    ├── cancel_listing.rs
    └── withdraw_from_vault.rs
```

## Frontend

A React frontend is included with:

- Marketplace browser
- NFT minting via Metaplex
- Vault management (list, unlist, withdraw)

See `/frontend` for setup.

## Fees

- Marketplace fee: Configurable on init (max 10%)
- Royalties: 5% hardcoded (should read from metadata - TODO)

## Known Limitations

1. Royalties hardcoded at 5%
2. Pyth floor validation stubbed out
3. No collection offers
4. No bid system
5. One listing per NFT per user
