use {
    anchor_lang::{
        prelude::{msg, Pubkey}, solana_program::{instruction::Instruction, system_program}, system_program::ID as SYSTEM_PROGRAM_ID, AccountDeserialize, InstructionData, ToAccountMetas
    }, anchor_vault::{instruction, withdraw}, litesvm::LiteSVM, solana_keypair::Keypair, solana_message::{Message, VersionedMessage}, solana_signer::Signer, solana_transaction::{versioned::VersionedTransaction, Transaction}
};

// the purpose of this function is to setup the LiteSVM and return the program id and payer keypair for testing 
fn setup() -> (LiteSVM, Keypair) {
    let program_id = anchor_vault::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/anchor_vault.so"
    ));
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    (svm, payer)
}

#[test]
fn test_initialize_deposit_withdraw_close() { 
    let (mut svm, payer) = setup();
    let user = payer.pubkey();

    let (vault_state_pda, state_bump) = Pubkey::find_program_address(
        &[b"state", user.as_ref()], 
        &anchor_vault::id()
    );
    let (vault_account_pda, vault_bump) = Pubkey::find_program_address(
        &[b"vault", vault_state_pda.as_ref()],
         &anchor_vault::id()
    );

    // Initialize
    let init_ix = Instruction {
        program_id: anchor_vault::id(),
        accounts: anchor_vault::accounts::Initialize {
            user,
            vault_state: vault_state_pda,
            vault_account: vault_account_pda,
            system_program: SYSTEM_PROGRAM_ID,
        }.to_account_metas(None), 
        data: anchor_vault::instruction::Initialize {}.data(),
    };

    let message = Message::new(&[init_ix], Some(&payer.pubkey()));
    let recent_blockhash = svm.latest_blockhash();
    let transaction = Transaction::new(&[&payer], message, recent_blockhash);
    let tx1 = svm.send_transaction(transaction).unwrap();

    msg!("Vault initialized successfully");
    msg!("Transaction signature: {:?}", tx1.signature);

    let vault_state_account = svm.get_account(&vault_state_pda).unwrap();
    let vault_state_data = anchor_vault::state::VaultState::try_deserialize(
        &mut vault_state_account.data.as_ref()
    ).unwrap();

    assert_eq!(vault_state_data.vault_bump, vault_bump);
    assert_eq!(vault_state_data.state_bump, state_bump);

    // Deposit 1 Sol
    let deposit_amount: u64 = 1_000_000_000; // 1 SOL in lamports

    let vault_balance_before_deposit = svm.get_balance(&vault_account_pda).unwrap();  // ← moved here, before sending

    let deposit_ix = Instruction {
        program_id: anchor_vault::id(),
        accounts: anchor_vault::accounts::Deposit {
            user,
            vault_state: vault_state_pda,
            vault_account: vault_account_pda,
            system_program: SYSTEM_PROGRAM_ID,
        }.to_account_metas(None), 
        data: anchor_vault::instruction::Deposit { amount: deposit_amount }.data(),
    };
    let message = Message::new(&[deposit_ix], Some(&payer.pubkey()));
    let recent_blockhash = svm.latest_blockhash();
    let transaction2 = Transaction::new(&[&payer], message, recent_blockhash);
    let tx2 = svm.send_transaction(transaction2).unwrap();

    msg!("Deposit successful");
    msg!("Transaction signature: {:?}", tx2.signature);

    let balance_after_deposit: u64 = svm.get_balance(&vault_account_pda).unwrap();  // this one stays where it was
    assert_eq!(
        balance_after_deposit - vault_balance_before_deposit,
        deposit_amount,
        "vault should grow by exactly the deposited amount"
    );

    // Withdraw 0.5 Sol
    let vault_balance_before_withdraw = svm.get_balance(&vault_account_pda).unwrap();
    let withdraw_amount: u64 = 500_000_000; // 0.5 SOL in lamports

    let withdraw_ix = Instruction {
        program_id: anchor_vault::id(),
        accounts: anchor_vault::accounts::Withdraw {
            user,
            vault_state: vault_state_pda,
            vault_account: vault_account_pda,
            system_program: SYSTEM_PROGRAM_ID,
        }.to_account_metas(None), 
        data: anchor_vault::instruction::Withdraw { amount: withdraw_amount }
        .data(),
    };

    let message = Message::new(&[withdraw_ix], Some(&payer.pubkey()));
    let recent_blockhash = svm.latest_blockhash();
    let transaction3 = Transaction::new(&[&payer], message, recent_blockhash);
    let tx3 = svm.send_transaction(transaction3).unwrap();

    msg!("O.5 Sol withdrawn successfully from Vaul");
    msg!("Transaction signature: {}", tx3.signature);

    let balance_after_withdraw: u64 = svm.get_balance(&vault_account_pda).unwrap();
    assert_eq!(
        vault_balance_before_withdraw - balance_after_withdraw,
        withdraw_amount,
        "vault should shrink by exactly the withdrawn amount"
    );
    msg!("Vault balance after withdraw: {}", balance_after_withdraw);

    // Close
    let close_amount: u64 = svm.get_balance(&vault_account_pda).unwrap();

    let close_ix = Instruction {
        program_id: anchor_vault::id(),
        accounts: anchor_vault::accounts::Close {
            user,
            vault_state: vault_state_pda,
            vault_account: vault_account_pda,
            system_program: SYSTEM_PROGRAM_ID,
        }.to_account_metas(None), 
        data: anchor_vault::instruction::Close {}
        .data(),
    };

    let message = Message::new(&[close_ix], Some(&payer.pubkey()));
    let recent_blockhash = svm.latest_blockhash();
    let transaction4 = Transaction::new(&[&payer], message, recent_blockhash);
    let tx4 = svm.send_transaction(transaction4).unwrap();

    msg!("Vault closed successfully");
    msg!("Transaction signature: {}", tx4.signature);

    assert!(svm.get_account(&vault_account_pda).is_none());
    assert!(svm.get_account(&vault_state_pda).is_none());

    let user_balance_after_close: u64 = svm.get_balance(&user).unwrap();
    assert!(user_balance_after_close > close_amount);
    msg!("User balance after close: {}", user_balance_after_close);
    
}
