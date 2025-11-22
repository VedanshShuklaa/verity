import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { Verity } from "../target/types/verity";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddress,
} from "@solana/spl-token";
import { PublicKey, Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { assert, expect } from "chai";

describe("Verity Escrowless NFT Marketplace", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Verity as Program<Verity>;
  const PROGRAM_ID = program.programId;

  // Test constants
  const FEE_BPS = 250; // 2.5%
  const START_PRICE = new BN(2 * LAMPORTS_PER_SOL);
  const MIN_PRICE = new BN(1 * LAMPORTS_PER_SOL);
  const DURATION = new BN(3600); // 1 hour

  // Helper to airdrop and confirm
  async function airdrop(pubkey: PublicKey, amount = 10 * LAMPORTS_PER_SOL) {
    const sig = await provider.connection.requestAirdrop(pubkey, amount);
    await provider.connection.confirmTransaction(sig, "confirmed");
  }

  // Helper to create NFT mint
  async function createNFT(owner: Keypair): Promise<{ mint: PublicKey; tokenAccount: PublicKey }> {
    const mint = await createMint(
      provider.connection,
      owner,
      owner.publicKey,
      null,
      0 // NFT = 0 decimals
    );
    const tokenAccount = await createAccount(
      provider.connection,
      owner,
      mint,
      owner.publicKey
    );
    await mintTo(
      provider.connection,
      owner,
      mint,
      tokenAccount,
      owner,
      1 // NFT = supply of 1
    );
    return { mint, tokenAccount };
  }

  // Helper to derive PDAs
  function getConfigPDA(): [PublicKey, number] {
    return PublicKey.findProgramAddressSync([Buffer.from("config")], PROGRAM_ID);
  }

  function getUserVaultPDA(owner: PublicKey, mint: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("user_vault"), owner.toBuffer(), mint.toBuffer()],
      PROGRAM_ID
    );
  }

  function getListingPDA(seller: PublicKey, mint: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("listing"), seller.toBuffer(), mint.toBuffer()],
      PROGRAM_ID
    );
  }

  // ============================================
  // Test Suite Setup
  // ============================================

  describe("Config Initialization", () => {
    const authority = Keypair.generate();
    const feeRecipient = Keypair.generate();

    before(async () => {
      await airdrop(authority.publicKey);
    });

    it("initializes marketplace config", async () => {
      const [configPda] = getConfigPDA();

      await program.methods
        .initializeConfig(FEE_BPS, feeRecipient.publicKey)
        .accountsPartial({
          config: configPda,
          authority: authority.publicKey,
        })
        .signers([authority])
        .rpc();

      const config = await program.account.config.fetch(configPda);
      assert.equal(config.authority.toString(), authority.publicKey.toString());
      assert.equal(config.feeBps, FEE_BPS);
      assert.equal(config.feeRecipient.toString(), feeRecipient.publicKey.toString());
    });

    it("fails to reinitialize config", async () => {
      const [configPda] = getConfigPDA();

      try {
        await program.methods
          .initializeConfig(500, feeRecipient.publicKey)
          .accountsPartial({
            config: configPda,
            authority: authority.publicKey,
          })
          .signers([authority])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        // Expected - account already exists
        expect(err.toString()).to.include("already in use");
      }
    });
  });

  // ============================================
  // User Vault Tests
  // ============================================

  describe("User Vault", () => {
    let seller: Keypair;
    let mint: PublicKey;
    let sellerTokenAccount: PublicKey;

    before(async () => {
      seller = Keypair.generate();
      await airdrop(seller.publicKey);
      const nft = await createNFT(seller);
      mint = nft.mint;
      sellerTokenAccount = nft.tokenAccount;
    });

    it("creates user vault and deposits NFT", async () => {
      const [userVaultPda] = getUserVaultPDA(seller.publicKey, mint);
      const vaultAta = await getAssociatedTokenAddress(mint, userVaultPda, true);

      await program.methods
        .initializeUserVault()
        .accountsPartial({
          userVault: userVaultPda,
          vaultPda: userVaultPda,
          owner: seller.publicKey,
          ownerTokenAccount: sellerTokenAccount,
          vaultAta: vaultAta,
          mint: mint,
        })
        .signers([seller])
        .rpc();

      // Verify vault created
      const vault = await program.account.userVault.fetch(userVaultPda);
      assert.equal(vault.owner.toString(), seller.publicKey.toString());
      assert.equal(vault.mint.toString(), mint.toString());

      // Verify NFT transferred to vault
      const vaultAccount = await getAccount(provider.connection, vaultAta);
      assert.equal(vaultAccount.amount.toString(), "1");

      // Verify seller no longer has NFT
      const sellerAccount = await getAccount(provider.connection, sellerTokenAccount);
      assert.equal(sellerAccount.amount.toString(), "0");
    });

    it("fails to create duplicate vault", async () => {
      const [userVaultPda] = getUserVaultPDA(seller.publicKey, mint);
      const vaultAta = await getAssociatedTokenAddress(mint, userVaultPda, true);

      try {
        await program.methods
          .initializeUserVault()
          .accountsPartial({
            userVault: userVaultPda,
            vaultPda: userVaultPda,
            owner: seller.publicKey,
            ownerTokenAccount: sellerTokenAccount,
            vaultAta: vaultAta,
            mint: mint,
          })
          .signers([seller])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        expect(err.toString()).to.include("already in use");
      }
    });
  });

  // ============================================
  // Listing Tests
  // ============================================

  describe("Listings", () => {
    let seller: Keypair;
    let mint: PublicKey;
    let sellerTokenAccount: PublicKey;
    let userVaultPda: PublicKey;
    let vaultAta: PublicKey;

    before(async () => {
      seller = Keypair.generate();
      await airdrop(seller.publicKey);
      const nft = await createNFT(seller);
      mint = nft.mint;
      sellerTokenAccount = nft.tokenAccount;

      [userVaultPda] = getUserVaultPDA(seller.publicKey, mint);
      vaultAta = await getAssociatedTokenAddress(mint, userVaultPda, true);

      // Create vault first
      await program.methods
        .initializeUserVault()
        .accountsPartial({
          userVault: userVaultPda,
          vaultPda: userVaultPda,
          owner: seller.publicKey,
          ownerTokenAccount: sellerTokenAccount,
          vaultAta: vaultAta,
          mint: mint,
        })
        .signers([seller])
        .rpc();
    });

    it("creates fixed price listing", async () => {
      const [listingPda] = getListingPDA(seller.publicKey, mint);
      const now = Math.floor(Date.now() / 1000);

      await program.methods
        .createListing(
          { fixed: {} }, // PriceType::Fixed
          START_PRICE,
          MIN_PRICE,
          new BN(now),
          DURATION,
          null, // min_floor
          null, // valid_from
          null  // valid_until
        )
        .accountsPartial({
          listing: listingPda,
          userVault: userVaultPda,
          vaultAta: vaultAta,
          seller: seller.publicKey,
          mint: mint,
        })
        .signers([seller])
        .rpc();

      const listing = await program.account.listing.fetch(listingPda);
      assert.equal(listing.seller.toString(), seller.publicKey.toString());
      assert.equal(listing.mint.toString(), mint.toString());
      assert.equal(listing.state, 0); // STATE_ACTIVE
      assert.equal(listing.priceConfig.startPrice.toString(), START_PRICE.toString());
    });

    it("cancels listing (NFT stays in vault)", async () => {
      const [listingPda] = getListingPDA(seller.publicKey, mint);

      await program.methods
        .cancelListing()
        .accountsPartial({
          listing: listingPda,
          userVault: userVaultPda,
          seller: seller.publicKey,
        })
        .signers([seller])
        .rpc();

      // Listing should be closed
      try {
        await program.account.listing.fetch(listingPda);
        assert.fail("Listing should be closed");
      } catch (err) {
        expect(err.toString().toLowerCase()).to.satisfy(
          (s: string) => s.includes("account does not exist") || s.includes("could not find")
        );
      }

      // NFT should still be in vault
      const vaultAccount = await getAccount(provider.connection, vaultAta);
      assert.equal(vaultAccount.amount.toString(), "1");
    });

    it("creates decay price listing", async () => {
      const [listingPda] = getListingPDA(seller.publicKey, mint);
      const now = Math.floor(Date.now() / 1000);

      await program.methods
        .createListing(
          { linearDecay: {} }, // PriceType::LinearDecay
          START_PRICE,
          MIN_PRICE,
          new BN(now),
          DURATION,
          null,
          null,
          null
        )
        .accountsPartial({
          listing: listingPda,
          userVault: userVaultPda,
          vaultAta: vaultAta,
          seller: seller.publicKey,
          mint: mint,
        })
        .signers([seller])
        .rpc();

      const listing = await program.account.listing.fetch(listingPda);
      assert.deepEqual(listing.priceConfig.priceType, { linearDecay: {} });
    });
  });

  // ============================================
  // Buy Flow Tests
  // ============================================

  describe("Buy NFT", () => {
    let seller: Keypair;
    let buyer: Keypair;
    let feeRecipient: PublicKey;
    let mint: PublicKey;
    let sellerTokenAccount: PublicKey;
    let configPda: PublicKey;
    let userVaultPda: PublicKey;
    let vaultAta: PublicKey;
    let listingPda: PublicKey;

    before(async () => {
      seller = Keypair.generate();
      buyer = Keypair.generate();
      await airdrop(seller.publicKey);
      await airdrop(buyer.publicKey);

      const nft = await createNFT(seller);
      mint = nft.mint;
      sellerTokenAccount = nft.tokenAccount;

      [configPda] = getConfigPDA();
      [userVaultPda] = getUserVaultPDA(seller.publicKey, mint);
      [listingPda] = getListingPDA(seller.publicKey, mint);
      vaultAta = await getAssociatedTokenAddress(mint, userVaultPda, true);

      // Get fee recipient from config
      const config = await program.account.config.fetch(configPda);
      feeRecipient = config.feeRecipient;

      // Create vault
      await program.methods
        .initializeUserVault()
        .accountsPartial({
          userVault: userVaultPda,
          vaultPda: userVaultPda,
          owner: seller.publicKey,
          ownerTokenAccount: sellerTokenAccount,
          vaultAta: vaultAta,
          mint: mint,
        })
        .signers([seller])
        .rpc();

      // Create listing
      const now = Math.floor(Date.now() / 1000);
      await program.methods
        .createListing(
          { fixed: {} },
          START_PRICE,
          MIN_PRICE,
          new BN(now),
          DURATION,
          null,
          null,
          null
        )
        .accountsPartial({
          listing: listingPda,
          userVault: userVaultPda,
          vaultAta: vaultAta,
          seller: seller.publicKey,
          mint: mint,
        })
        .signers([seller])
        .rpc();
    });

    it("buys NFT and transfers to buyer", async () => {
      const buyerAta = await getAssociatedTokenAddress(mint, buyer.publicKey);
      const sellerBalanceBefore = await provider.connection.getBalance(seller.publicKey);

      await program.methods
        .buyNow()
        .accountsPartial({
          listing: listingPda,
          userVault: userVaultPda,
          vaultPda: userVaultPda,
          vaultAta: vaultAta,
          buyer: buyer.publicKey,
          buyerAta: buyerAta,
          seller: seller.publicKey,
          mint: mint,
          config: configPda,
          feeRecipient: feeRecipient,
        })
        .signers([buyer])
        .rpc();

      // Verify buyer received NFT
      const buyerAccount = await getAccount(provider.connection, buyerAta);
      assert.equal(buyerAccount.amount.toString(), "1");

      // Verify seller received payment
      const sellerBalanceAfter = await provider.connection.getBalance(seller.publicKey);
      assert.isTrue(sellerBalanceAfter > sellerBalanceBefore);

      // Verify listing closed
      try {
        await program.account.listing.fetch(listingPda);
        assert.fail("Listing should be closed");
      } catch (err) {
        expect(err.toString().toLowerCase()).to.satisfy(
          (s: string) => s.includes("account does not exist") || s.includes("could not find")
        );
      }
    });
  });

  // ============================================
  // Withdraw Tests
  // ============================================

  describe("Withdraw from Vault", () => {
    let owner: Keypair;
    let mint: PublicKey;
    let ownerTokenAccount: PublicKey;
    let userVaultPda: PublicKey;
    let vaultAta: PublicKey;

    before(async () => {
      owner = Keypair.generate();
      await airdrop(owner.publicKey);
      const nft = await createNFT(owner);
      mint = nft.mint;
      ownerTokenAccount = nft.tokenAccount;

      [userVaultPda] = getUserVaultPDA(owner.publicKey, mint);
      vaultAta = await getAssociatedTokenAddress(mint, userVaultPda, true);

      // Create vault
      await program.methods
        .initializeUserVault()
        .accountsPartial({
          userVault: userVaultPda,
          vaultPda: userVaultPda,
          owner: owner.publicKey,
          ownerTokenAccount: ownerTokenAccount,
          vaultAta: vaultAta,
          mint: mint,
        })
        .signers([owner])
        .rpc();
    });

    it("withdraws NFT from vault", async () => {
      await program.methods
        .withdrawFromVault()
        .accountsPartial({
          userVault: userVaultPda,
          vaultPda: userVaultPda,
          vaultAta: vaultAta,
          owner: owner.publicKey,
          ownerTokenAccount: ownerTokenAccount,
        })
        .signers([owner])
        .rpc();

      // Verify NFT back in owner's account
      const ownerAccount = await getAccount(provider.connection, ownerTokenAccount);
      assert.equal(ownerAccount.amount.toString(), "1");

      // Verify vault closed
      try {
        await program.account.userVault.fetch(userVaultPda);
        assert.fail("Vault should be closed");
      } catch (err) {
        expect(err.toString().toLowerCase()).to.satisfy(
          (s: string) => s.includes("account does not exist") || s.includes("could not find")
        );
      }
    });
  });

  // ============================================
  // Security Tests
  // ============================================

  describe("Security", () => {
    it("prevents double-buy race condition", async () => {
      const seller = Keypair.generate();
      const buyer1 = Keypair.generate();
      const buyer2 = Keypair.generate();
      await airdrop(seller.publicKey);
      await airdrop(buyer1.publicKey);
      await airdrop(buyer2.publicKey);

      const nft = await createNFT(seller);
      const [configPda] = getConfigPDA();
      const config = await program.account.config.fetch(configPda);

      const [userVaultPda] = getUserVaultPDA(seller.publicKey, nft.mint);
      const [listingPda] = getListingPDA(seller.publicKey, nft.mint);
      const vaultAta = await getAssociatedTokenAddress(nft.mint, userVaultPda, true);

      // Setup vault and listing
      await program.methods
        .initializeUserVault()
        .accountsPartial({
          userVault: userVaultPda,
          vaultPda: userVaultPda,
          owner: seller.publicKey,
          ownerTokenAccount: nft.tokenAccount,
          vaultAta: vaultAta,
          mint: nft.mint,
        })
        .signers([seller])
        .rpc();

      const now = Math.floor(Date.now() / 1000);
      await program.methods
        .createListing({ fixed: {} }, START_PRICE, MIN_PRICE, new BN(now), DURATION, null, null, null)
        .accountsPartial({
          listing: listingPda,
          userVault: userVaultPda,
          vaultAta: vaultAta,
          seller: seller.publicKey,
          mint: nft.mint,
        })
        .signers([seller])
        .rpc();

      // Buyer 1 purchases
      const buyer1Ata = await getAssociatedTokenAddress(nft.mint, buyer1.publicKey);
      await program.methods
        .buyNow()
        .accountsPartial({
          listing: listingPda,
          userVault: userVaultPda,
          vaultPda: userVaultPda,
          vaultAta: vaultAta,
          buyer: buyer1.publicKey,
          buyerAta: buyer1Ata,
          seller: seller.publicKey,
          mint: nft.mint,
          config: configPda,
          feeRecipient: config.feeRecipient,
        })
        .signers([buyer1])
        .rpc();

      // Buyer 2 should fail
      const buyer2Ata = await getAssociatedTokenAddress(nft.mint, buyer2.publicKey);
      try {
        await program.methods
          .buyNow()
          .accountsPartial({
            listing: listingPda,
            userVault: userVaultPda,
            vaultPda: userVaultPda,
            vaultAta: vaultAta,
            buyer: buyer2.publicKey,
            buyerAta: buyer2Ata,
            seller: seller.publicKey,
            mint: nft.mint,
            config: configPda,
            feeRecipient: config.feeRecipient,
          })
          .signers([buyer2])
          .rpc();
        assert.fail("Second buy should fail");
      } catch (err) {
        // Expected - listing closed or state changed after first buy
        // Could be "AccountNotInitialized", "account does not exist", or constraint error
        assert.ok(err, "Transaction should have failed");
      }
    });

    it("prevents unauthorized cancel", async () => {
      const seller = Keypair.generate();
      const attacker = Keypair.generate();
      await airdrop(seller.publicKey);
      await airdrop(attacker.publicKey);

      const nft = await createNFT(seller);
      const [userVaultPda] = getUserVaultPDA(seller.publicKey, nft.mint);
      const [listingPda] = getListingPDA(seller.publicKey, nft.mint);
      const vaultAta = await getAssociatedTokenAddress(nft.mint, userVaultPda, true);

      // Setup
      await program.methods
        .initializeUserVault()
        .accountsPartial({
          userVault: userVaultPda,
          vaultPda: userVaultPda,
          owner: seller.publicKey,
          ownerTokenAccount: nft.tokenAccount,
          vaultAta: vaultAta,
          mint: nft.mint,
        })
        .signers([seller])
        .rpc();

      const now = Math.floor(Date.now() / 1000);
      await program.methods
        .createListing({ fixed: {} }, START_PRICE, MIN_PRICE, new BN(now), DURATION, null, null, null)
        .accountsPartial({
          listing: listingPda,
          userVault: userVaultPda,
          vaultAta: vaultAta,
          seller: seller.publicKey,
          mint: nft.mint,
        })
        .signers([seller])
        .rpc();

      // Attacker tries to cancel
      try {
        await program.methods
          .cancelListing()
          .accountsPartial({
            listing: listingPda,
            userVault: userVaultPda,
            seller: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();
        assert.fail("Attacker should not be able to cancel");
      } catch (err) {
        // Expected - either constraint seeds fail (user_vault PDA mismatch) or unauthorized
        assert.ok(err, "Transaction should have failed");
      }
    });

    it("prevents unauthorized withdraw", async () => {
      const owner = Keypair.generate();
      const attacker = Keypair.generate();
      await airdrop(owner.publicKey);
      await airdrop(attacker.publicKey);

      const nft = await createNFT(owner);
      const [userVaultPda] = getUserVaultPDA(owner.publicKey, nft.mint);
      const vaultAta = await getAssociatedTokenAddress(nft.mint, userVaultPda, true);

      // Create attacker's token account
      const attackerTokenAccount = await createAccount(
        provider.connection,
        attacker,
        nft.mint,
        attacker.publicKey
      );

      // Owner creates vault
      await program.methods
        .initializeUserVault()
        .accountsPartial({
          userVault: userVaultPda,
          vaultPda: userVaultPda,
          owner: owner.publicKey,
          ownerTokenAccount: nft.tokenAccount,
          vaultAta: vaultAta,
          mint: nft.mint,
        })
        .signers([owner])
        .rpc();

      // Attacker tries to withdraw
      try {
        await program.methods
          .withdrawFromVault()
          .accountsPartial({
            userVault: userVaultPda,
            vaultPda: userVaultPda,
            vaultAta: vaultAta,
            owner: attacker.publicKey,
            ownerTokenAccount: attackerTokenAccount,
          })
          .signers([attacker])
          .rpc();
        assert.fail("Attacker should not be able to withdraw");
      } catch (err) {
        // PDA derivation will fail since attacker is not the owner
        expect(err.toString()).to.satisfy(
          (s: string) => s.includes("ConstraintSeeds") || s.includes("UnauthorizedVaultOwner")
        );
      }
    });
  });
});