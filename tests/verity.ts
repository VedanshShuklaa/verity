import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Verity } from "../target/types/verity";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { assert } from "chai";

describe("verity (escrowless) - tests", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Verity as Program<Verity>;
  // fallback: explicit program id you gave
  const PROGRAM_ID = new anchor.web3.PublicKey("2pw3GT51qRHmrobvzmBzso2iHCBv5rN8MTNTNMxEyG2P");

  let seller: anchor.web3.Keypair;
  let buyer: anchor.web3.Keypair;
  let mint: anchor.web3.PublicKey;
  let sellerTokenAccount: anchor.web3.PublicKey;
  let configPda: anchor.web3.PublicKey;
  const START_PRICE = new anchor.BN(2_000_000_000);
  const MIN_PRICE = new anchor.BN(1_000_000_000);
  const DURATION = new anchor.BN(3600);

  before(async () => {
    seller = anchor.web3.Keypair.generate();
    buyer = anchor.web3.Keypair.generate();

    // Airdrop
    await provider.connection.requestAirdrop(seller.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.requestAirdrop(buyer.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await new Promise((r) => setTimeout(r, 2000));

    // Create NFT mint and mint to seller
    mint = await createMint(provider.connection, seller, seller.publicKey, null, 0);
    sellerTokenAccount = await createAccount(provider.connection, seller, mint, seller.publicKey);
    await mintTo(provider.connection, seller, mint, sellerTokenAccount, seller, 1);

    // config PDA (global)
    [configPda] = anchor.web3.PublicKey.findProgramAddressSync([Buffer.from("config")], PROGRAM_ID);

    // initialize config (use seller as authority/fee recipient for tests)
    await program.methods
      .initializeConfig(250, seller.publicKey) // 2.5% fee -> fee recipient = seller for test simplicity
      .accounts({
        config: configPda,
        authority: seller.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([seller])
      .rpc();
  });

  it("initializes user vault and creates a listing", async () => {
    // Build PDAs
    const [userVaultPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("user_vault"), seller.publicKey.toBuffer(), mint.toBuffer()],
      PROGRAM_ID
    );
    const vaultPda = userVaultPda;
    const vaultAta = await anchor.utils.token.associatedAddress({ mint, owner: vaultPda });

    // initialize user vault (deposits NFT into vault_ata)
    await program.methods
      .initializeUserVault()
      .accounts({
        userVault: userVaultPda,
        vaultPda,
        owner: seller.publicKey,
        ownerTokenAccount: sellerTokenAccount,
        vaultAta,
        mint,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .signers([seller])
      .rpc();

    // Vault ATA should hold the NFT
    const vaultAccount = await getAccount(provider.connection, vaultAta);
    assert.equal(vaultAccount.amount.toString(), "1");

    // create listing PDA
    const [listingPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("listing"), seller.publicKey.toBuffer(), mint.toBuffer()],
      PROGRAM_ID
    );

    // create listing (priceType: Fixed = 0)
    await program.methods
      .createListing(0, START_PRICE, MIN_PRICE, new anchor.BN(Math.floor(Date.now() / 1000)), DURATION, null, null, null)
      .accounts({
        listing: listingPda,
        userVault: userVaultPda,
        vaultAta,
        seller: seller.publicKey,
        mint,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([seller])
      .rpc();

    const listingAccount = await program.account.listing.fetch(listingPda);
    assert.equal(listingAccount.seller.toString(), seller.publicKey.toString());
    // state is a u8/number; check equals STATE_ACTIVE (assumed 0)
    assert.equal(Number(listingAccount.state), 0);
    assert.equal(listingAccount.priceConfig.startPrice.toString(), START_PRICE.toString());
  });

  it("buys the NFT at current price", async () => {
    const [listingPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("listing"), seller.publicKey.toBuffer(), mint.toBuffer()],
      PROGRAM_ID
    );
    const [userVaultPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("user_vault"), seller.publicKey.toBuffer(), mint.toBuffer()],
      PROGRAM_ID
    );
    const vaultPda = userVaultPda;
    const vaultAta = await anchor.utils.token.associatedAddress({ mint, owner: vaultPda });
    const buyerAta = await anchor.utils.token.associatedAddress({ mint, owner: buyer.publicKey });

    const sellerBalanceBefore = await provider.connection.getBalance(seller.publicKey);

    await program.methods
      .buyNow()
      .accounts({
        listing: listingPda,
        userVault: userVaultPda,
        vaultPda,
        vaultAta,
        buyer: buyer.publicKey,
        buyerAta,
        seller: seller.publicKey,
        mint,
        config: configPda,
        feeRecipient: seller.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .signers([buyer])
      .rpc();

    const buyerTokenAccount = await getAccount(provider.connection, buyerAta);
    assert.equal(buyerTokenAccount.amount.toString(), "1");

    const sellerBalanceAfter = await provider.connection.getBalance(seller.publicKey);
    assert(sellerBalanceAfter > sellerBalanceBefore);

    // listing should be closed -> fetching should fail
    try {
      await program.account.listing.fetch(listingPda);
      assert.fail("listing should be closed");
    } catch (err) {
      assert(err.toString().toLowerCase().includes("account does not exist") || err.toString().includes("Could not find account"));
    }
  });

  it("prevents double-buy / race condition", async () => {
    // create new seller + mint
    const newSeller = anchor.web3.Keypair.generate();
    await provider.connection.requestAirdrop(newSeller.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await new Promise((r) => setTimeout(r, 2000));

    const newMint = await createMint(provider.connection, newSeller, newSeller.publicKey, null, 0);
    const newSellerTokenAccount = await createAccount(provider.connection, newSeller, newMint, newSeller.publicKey);
    await mintTo(provider.connection, newSeller, newMint, newSellerTokenAccount, newSeller, 1);

    const [newUserVaultPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("user_vault"), newSeller.publicKey.toBuffer(), newMint.toBuffer()],
      PROGRAM_ID
    );
    const newVaultPda = newUserVaultPda;
    const newVaultAta = await anchor.utils.token.associatedAddress({ mint: newMint, owner: newVaultPda });

    // init vault and deposit
    await program.methods
      .initializeUserVault()
      .accounts({
        userVault: newUserVaultPda,
        vaultPda: newVaultPda,
        owner: newSeller.publicKey,
        ownerTokenAccount: newSellerTokenAccount,
        vaultAta: newVaultAta,
        mint: newMint,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .signers([newSeller])
      .rpc();

    const [newListingPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("listing"), newSeller.publicKey.toBuffer(), newMint.toBuffer()],
      PROGRAM_ID
    );

    await program.methods
      .createListing(0, START_PRICE, MIN_PRICE, new anchor.BN(Math.floor(Date.now() / 1000)), DURATION, null, null, null)
      .accounts({
        listing: newListingPda,
        userVault: newUserVaultPda,
        vaultAta: newVaultAta,
        seller: newSeller.publicKey,
        mint: newMint,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([newSeller])
      .rpc();

    // buyer1 purchases
    const buyer1 = anchor.web3.Keypair.generate();
    await provider.connection.requestAirdrop(buyer1.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await new Promise((r) => setTimeout(r, 2000));
    const buyer1Ata = await anchor.utils.token.associatedAddress({ mint: newMint, owner: buyer1.publicKey });

    await program.methods
      .buyNow()
      .accounts({
        listing: newListingPda,
        userVault: newUserVaultPda,
        vaultPda: newVaultPda,
        vaultAta: newVaultAta,
        buyer: buyer1.publicKey,
        buyerAta: buyer1Ata,
        seller: newSeller.publicKey,
        mint: newMint,
        config: configPda,
        feeRecipient: newSeller.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .signers([buyer1])
      .rpc();

    // buyer2 attempt should fail because listing closed / gone
    const buyer2 = anchor.web3.Keypair.generate();
    await provider.connection.requestAirdrop(buyer2.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await new Promise((r) => setTimeout(r, 2000));
    const buyer2Ata = await anchor.utils.token.associatedAddress({ mint: newMint, owner: buyer2.publicKey });

    try {
      await program.methods
        .buyNow()
        .accounts({
          listing: newListingPda,
          userVault: newUserVaultPda,
          vaultPda: newVaultPda,
          vaultAta: newVaultAta,
          buyer: buyer2.publicKey,
          buyerAta: buyer2Ata,
          seller: newSeller.publicKey,
          mint: newMint,
          config: configPda,
          feeRecipient: newSeller.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        })
        .signers([buyer2])
        .rpc();
      assert.fail("Second buy should fail");
    } catch (err) {
      assert(err.toString().toLowerCase().includes("account does not exist") || err.toString().includes("Could not find account"));
    }
  });

  it("seller can cancel listing (NFT remains in vault)", async () => {
    // create seller + mint
    const s = anchor.web3.Keypair.generate();
    await provider.connection.requestAirdrop(s.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await new Promise((r) => setTimeout(r, 2000));
    const m = await createMint(provider.connection, s, s.publicKey, null, 0);
    const sToken = await createAccount(provider.connection, s, m, s.publicKey);
    await mintTo(provider.connection, s, m, sToken, s, 1);

    const [uv] = anchor.web3.PublicKey.findProgramAddressSync([Buffer.from("user_vault"), s.publicKey.toBuffer(), m.toBuffer()], PROGRAM_ID);
    const vaultPda = uv;
    const vaultAta = await anchor.utils.token.associatedAddress({ mint: m, owner: vaultPda });

    await program.methods
      .initializeUserVault()
      .accounts({
        userVault: uv,
        vaultPda,
        owner: s.publicKey,
        ownerTokenAccount: sToken,
        vaultAta,
        mint: m,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .signers([s])
      .rpc();

    const [listing] = anchor.web3.PublicKey.findProgramAddressSync([Buffer.from("listing"), s.publicKey.toBuffer(), m.toBuffer()], PROGRAM_ID);

    await program.methods
      .createListing(0, START_PRICE, MIN_PRICE, new anchor.BN(Math.floor(Date.now()/1000)), DURATION, null, null, null)
      .accounts({
        listing,
        userVault: uv,
        vaultAta,
        seller: s.publicKey,
        mint: m,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([s])
      .rpc();

    // Cancel the listing
    await program.methods
      .cancelListing()
      .accounts({
        listing,
        userVault: uv,
        seller: s.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([s])
      .rpc();

    // Listing should be closed
    try {
      await program.account.listing.fetch(listing);
      assert.fail("Listing should be closed");
    } catch (err) {
      assert(err.toString().toLowerCase().includes("account does not exist") || err.toString().includes("Could not find account"));
    }

    // NFT still in vault ATA
    const vaultAcc = await getAccount(provider.connection, vaultAta);
    assert.equal(vaultAcc.amount.toString(), "1");
  });
});
