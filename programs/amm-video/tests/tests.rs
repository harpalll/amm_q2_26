use {
    anchor_lang::AccountDeserialize,
    anchor_spl::associated_token,
    litesvm::LiteSVM,
    litesvm_token::CreateMint,
    solana_keypair::Keypair,
    solana_message::{Instruction, Message, VersionedMessage},
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

mod ix_handlers;
use ix_handlers::*;

const SEED: u64 = 123;
const FEE_BPS: u16 = 30; // 0.30%
const GENESIS_LP: u64 = 100_000_000;
const GENESIS_X: u64 = 200_000_000;
const GENESIS_Y: u64 = 200_000_000;

fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> litesvm::types::TransactionResult {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
}

// Setup function to initialize LiteSVM and create a payer keypair.
// Returns (svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y).
fn setup(
    seed: u64,
) -> (
    LiteSVM,
    Keypair,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
) {
    let program_id = amm_video::id();
    let payer = Keypair::new();
    let treasury = Keypair::new().pubkey();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/amm_video.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    // Create two mints (Mint X and Mint Y) with 6 decimal places and the payer as the authority
    // This done using litesvm-token's CreateMint utility which creates the mint in the LiteSVM environment
    let mint_x = CreateMint::new(&mut svm, &payer)
        .decimals(6)
        .authority(&payer.pubkey())
        .send()
        .unwrap();

    let mint_y = CreateMint::new(&mut svm, &payer)
        .decimals(6)
        .authority(&payer.pubkey())
        .send()
        .unwrap();

    let config =
        Pubkey::find_program_address(&[b"config", &seed.to_le_bytes()], &amm_video::id()).0;
    let mint_lp = Pubkey::find_program_address(&[b"lp", config.as_ref()], &amm_video::id()).0;

    // Derive the PDA for the vault associated token account using the config PDA and Mint X / Mint Y
    let vault_x = associated_token::get_associated_token_address(&config, &mint_x);
    let vault_y = associated_token::get_associated_token_address(&config, &mint_y);

    (
        svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    )
}

/// Spin up a pool and perform the genesis deposit (100M LP for 200M X + 200M Y).
/// Returns the funded user ATAs for follow-up instructions.
fn setup_pool_with_liquidity() -> (
    LiteSVM,
    Keypair,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
) {
    let (mut svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y) =
        setup(SEED);

    let init_ix = create_initialise_ix(
        &mut svm,
        &payer,
        mint_x,
        mint_y,
        config,
        mint_lp,
        vault_x,
        vault_y,
        SEED,
        FEE_BPS,
        Some(payer.pubkey()),
        treasury,
    );

    let (user_x, user_y) = fund_user(&mut svm, &payer, &mint_x, &mint_y, 1_000_000_000, 1_000_000_000);
    let deposit_ix = create_deposit_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, user_x, user_y, GENESIS_LP,
        GENESIS_X, GENESIS_Y,
    );

    let res = send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]);
    assert!(res.is_ok(), "genesis setup failed: {:?}", res.err());

    (
        svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, user_x, user_y,
    )
}

// --- Account readers -------------------------------------------------------
// SPL Token Account layout: mint(32) | owner(32) | amount u64-le @ offset 64.
fn token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    let acc = svm.get_account(ata).unwrap();
    u64::from_le_bytes(acc.data[64..72].try_into().unwrap())
}

// SPL Mint layout: mint_authority(36) | supply u64-le @ offset 36.
fn mint_supply(svm: &LiteSVM, mint: &Pubkey) -> u64 {
    let acc = svm.get_account(mint).unwrap();
    u64::from_le_bytes(acc.data[36..44].try_into().unwrap())
}

fn read_config(svm: &LiteSVM, config: &Pubkey) -> amm_video::Config {
    let acc = svm.get_account(config).unwrap();
    amm_video::Config::try_deserialize(&mut &acc.data[..]).unwrap()
}

// --- initialize ------------------------------------------------------------

#[test]
fn test_initialize() {
    let (mut svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y) =
        setup(SEED);

    let instruction = create_initialise_ix(
        &mut svm,
        &payer,
        mint_x,
        mint_y,
        config,
        mint_lp,
        vault_x,
        vault_y,
        SEED,
        FEE_BPS,
        Some(payer.pubkey()),
        treasury,
    );
    let res = send(&mut svm, &[instruction], &payer, &[&payer]);
    assert!(res.is_ok(), "initialize failed: {:?}", res.err());

    // Config state is stored correctly, including fee + treasury.
    let state = read_config(&svm, &config);
    assert_eq!(state.seed, SEED);
    assert_eq!(state.fee, FEE_BPS);
    assert_eq!(state.treasury, treasury);
    assert_eq!(state.mint_x, mint_x);
    assert_eq!(state.mint_y, mint_y);
    assert_eq!(state.authority, Some(payer.pubkey()));
    assert!(!state.locked);

    // Vaults exist and are empty; LP supply is zero.
    assert_eq!(token_balance(&svm, &vault_x), 0);
    assert_eq!(token_balance(&svm, &vault_y), 0);
    assert_eq!(mint_supply(&svm, &mint_lp), 0);
}

#[test]
fn test_initialize_rejects_invalid_fee() {
    let (mut svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y) =
        setup(SEED);

    // 10_001 bps > 100% is rejected.
    let instruction = create_initialise_ix(
        &mut svm,
        &payer,
        mint_x,
        mint_y,
        config,
        mint_lp,
        vault_x,
        vault_y,
        SEED,
        10_001,
        Some(payer.pubkey()),
        treasury,
    );
    let res = send(&mut svm, &[instruction], &payer, &[&payer]);
    assert!(res.is_err(), "fee above 10_000 bps must fail");
}

// --- deposit ---------------------------------------------------------------

#[test]
pub fn test_deposit() {
    let (mut svm, payer, _treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y) =
        setup(SEED);
    let init_ix = create_initialise_ix(
        &mut svm,
        &payer,
        mint_x,
        mint_y,
        config,
        mint_lp,
        vault_x,
        vault_y,
        SEED,
        FEE_BPS,
        Some(payer.pubkey()),
        Keypair::new().pubkey(),
    );

    let (user_x, user_y) =
        fund_user(&mut svm, &payer, &mint_x, &mint_y, 1_000_000_000, 1_000_000_000);
    let deposit_ix = create_deposit_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, user_x, user_y, GENESIS_LP,
        GENESIS_X, GENESIS_Y,
    );

    let res = send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]);
    assert!(res.is_ok(), "deposit failed: {:?}", res.err());

    // Genesis deposit fills both vaults and mints the requested LP amount.
    assert_eq!(token_balance(&svm, &vault_x), GENESIS_X);
    assert_eq!(token_balance(&svm, &vault_y), GENESIS_Y);
    assert_eq!(mint_supply(&svm, &mint_lp), GENESIS_LP);
    let user_lp = associated_token::get_associated_token_address(&payer.pubkey(), &mint_lp);
    assert_eq!(token_balance(&svm, &user_lp), GENESIS_LP);
    // User ATAs were debited.
    assert_eq!(token_balance(&svm, &user_x), 1_000_000_000 - GENESIS_X);
    assert_eq!(token_balance(&svm, &user_y), 1_000_000_000 - GENESIS_Y);
}

#[test]
fn test_deposit_rejects_zero_amount() {
    let (svm, payer, _treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, user_x, user_y) =
        setup_pool_with_liquidity();
    let mut svm = svm;

    let bad_ix = create_deposit_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, user_x, user_y, 0, GENESIS_X,
        GENESIS_Y,
    );
    let res = send(&mut svm, &[bad_ix], &payer, &[&payer]);
    assert!(res.is_err(), "zero-amount deposit must fail");
}

#[test]
fn test_deposit_rejects_slippage() {
    let (svm, payer, _treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, user_x, user_y) =
        setup_pool_with_liquidity();
    let mut svm = svm;

    // 50M LP needs 100M X + 100M Y on top of genesis; max_x of 1_000 exceeds slippage.
    let bad_ix = create_deposit_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, user_x, user_y, 50_000_000,
        1_000, 100_000_000,
    );
    let res = send(&mut svm, &[bad_ix], &payer, &[&payer]);
    assert!(res.is_err(), "deposit breaching max_x must fail");
}

// --- withdraw --------------------------------------------------------------

#[test]
pub fn test_withdraw() {
    let (svm, payer, _treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, user_x, user_y) =
        setup_pool_with_liquidity();
    let mut svm = svm;

    // Burning 10% of LP supply returns 10% of each vault (20M X + 20M Y).
    let withdraw_ix = create_withdraw_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, 10_000_000, 19_000_000,
        19_000_000,
    );
    let res = send(&mut svm, &[withdraw_ix], &payer, &[&payer]);
    assert!(res.is_ok(), "withdraw failed: {:?}", res.err());

    assert_eq!(token_balance(&svm, &vault_x), GENESIS_X - 20_000_000);
    assert_eq!(token_balance(&svm, &vault_y), GENESIS_Y - 20_000_000);
    assert_eq!(mint_supply(&svm, &mint_lp), GENESIS_LP - 10_000_000);
    assert_eq!(
        token_balance(&svm, &user_x),
        1_000_000_000 - GENESIS_X + 20_000_000
    );
    assert_eq!(
        token_balance(&svm, &user_y),
        1_000_000_000 - GENESIS_Y + 20_000_000
    );
}

#[test]
fn test_withdraw_rejects_slippage() {
    let (svm, payer, _treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, _user_x, _user_y) =
        setup_pool_with_liquidity();
    let mut svm = svm;

    // 10M LP only yields 20M per side; demanding 100M must fail.
    let bad_ix = create_withdraw_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, 10_000_000, 100_000_000,
        100_000_000,
    );
    let res = send(&mut svm, &[bad_ix], &payer, &[&payer]);
    assert!(res.is_err(), "withdraw breaching min amounts must fail");
}

// --- swap (fees + treasury) -------------------------------------------------

#[test]
pub fn test_swap_x_for_y() {
    let (svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, user_x, user_y) =
        setup_pool_with_liquidity();
    let mut svm = svm;

    let treasury_x = associated_token::get_associated_token_address(&treasury, &mint_x);
    let amount_in = 10_000_000u64;

    let swap_ix = create_swap_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, treasury, true, amount_in, 1,
    );
    let res = send(&mut svm, &[swap_ix], &payer, &[&payer]);
    assert!(res.is_ok(), "swap X->Y failed: {:?}", res.err());

    // Fee = 10M * 30 / 10_000 = 30_000; half (15_000) goes to the treasury,
    // the rest of the input lands in the vault.
    let expected_treasury_fee = (amount_in * FEE_BPS as u64 / 10_000) / 2;
    assert_eq!(expected_treasury_fee, 15_000);
    assert_eq!(token_balance(&svm, &treasury_x), expected_treasury_fee);
    assert_eq!(
        token_balance(&svm, &vault_x),
        GENESIS_X + amount_in - expected_treasury_fee
    );
    // Taker received Y and spent exactly `amount_in` of X.
    assert!(token_balance(&svm, &user_y) > 1_000_000_000 - GENESIS_Y);
    assert_eq!(
        token_balance(&svm, &user_x),
        1_000_000_000 - GENESIS_X - amount_in
    );
}

#[test]
pub fn test_swap_y_for_x() {
    let (svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, user_x, user_y) =
        setup_pool_with_liquidity();
    let mut svm = svm;

    let treasury_y = associated_token::get_associated_token_address(&treasury, &mint_y);
    let amount_in = 10_000_000u64;

    let swap_ix = create_swap_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, treasury, false, amount_in, 1,
    );
    let res = send(&mut svm, &[swap_ix], &payer, &[&payer]);
    assert!(res.is_ok(), "swap Y->X failed: {:?}", res.err());

    let expected_treasury_fee = (amount_in * FEE_BPS as u64 / 10_000) / 2;
    assert_eq!(token_balance(&svm, &treasury_y), expected_treasury_fee);
    assert_eq!(
        token_balance(&svm, &vault_y),
        GENESIS_Y + amount_in - expected_treasury_fee
    );
    assert!(token_balance(&svm, &user_x) > 1_000_000_000 - GENESIS_X);
    assert_eq!(
        token_balance(&svm, &user_y),
        1_000_000_000 - GENESIS_Y - amount_in
    );
}

#[test]
fn test_swap_rejects_slippage() {
    let (svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, _user_x, _user_y) =
        setup_pool_with_liquidity();
    let mut svm = svm;

    let bad_ix = create_swap_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, treasury, true, 10_000_000,
        u64::MAX,
    );
    let res = send(&mut svm, &[bad_ix], &payer, &[&payer]);
    assert!(res.is_err(), "swap breaching min_amount_out must fail");
}

#[test]
fn test_swap_rejects_zero_amount() {
    let (svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, _user_x, _user_y) =
        setup_pool_with_liquidity();
    let mut svm = svm;

    let bad_ix = create_swap_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, treasury, true, 0, 0,
    );
    let res = send(&mut svm, &[bad_ix], &payer, &[&payer]);
    assert!(res.is_err(), "zero-amount swap must fail");
}

// --- lock -------------------------------------------------------------------

#[test]
fn test_lock_rejects_non_authority() {
    let (svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, _user_x, _user_y) =
        setup_pool_with_liquidity();
    let mut svm = svm;
    let _ = (treasury, mint_x, mint_y, mint_lp, vault_x, vault_y);

    let intruder = Keypair::new();
    let bad_ix = create_lock_ix(&intruder, config);
    let res = send(&mut svm, &[bad_ix], &payer, &[&payer, &intruder]);
    assert!(res.is_err(), "lock by non-authority must fail");
    assert!(!read_config(&svm, &config).locked);
}

#[test]
fn test_lock_disables_pool() {
    let (svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y, user_x, user_y) =
        setup_pool_with_liquidity();
    let mut svm = svm;

    let lock_ix = create_lock_ix(&payer, config);
    let res = send(&mut svm, &[lock_ix], &payer, &[&payer]);
    assert!(res.is_ok(), "lock failed: {:?}", res.err());
    assert!(read_config(&svm, &config).locked);

    // Locking twice is rejected.
    let res = send(&mut svm, &[create_lock_ix(&payer, config)], &payer, &[&payer]);
    assert!(res.is_err(), "double lock must fail");

    // Deposit, withdraw and swap are all disabled once locked.
    let deposit_ix = create_deposit_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, user_x, user_y, 1_000_000,
        10_000_000, 10_000_000,
    );
    assert!(send(&mut svm, &[deposit_ix], &payer, &[&payer]).is_err());

    let withdraw_ix = create_withdraw_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, 1_000_000, 0, 0,
    );
    assert!(send(&mut svm, &[withdraw_ix], &payer, &[&payer]).is_err());

    let swap_ix = create_swap_ix(
        &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, treasury, true, 1_000_000, 0,
    );
    assert!(send(&mut svm, &[swap_ix], &payer, &[&payer]).is_err());
}

#[test]
fn test_lock_rejects_missing_authority() {
    // A pool initialized with `authority: None` can never be locked.
    let (mut svm, payer, treasury, mint_x, mint_y, config, mint_lp, vault_x, vault_y) =
        setup(SEED + 1);
    let init_ix = create_initialise_ix(
        &mut svm,
        &payer,
        mint_x,
        mint_y,
        config,
        mint_lp,
        vault_x,
        vault_y,
        SEED + 1,
        FEE_BPS,
        None,
        treasury,
    );
    let res = send(&mut svm, &[init_ix], &payer, &[&payer]);
    assert!(res.is_ok(), "initialize failed: {:?}", res.err());

    let res = send(&mut svm, &[create_lock_ix(&payer, config)], &payer, &[&payer]);
    assert!(res.is_err(), "lock without an authority must fail");
}
