# Anchor Vault

A Solana SOL vault program built with Rust and Anchor.

The program allows a user to initialize a PDA-controlled vault, deposit SOL into it, withdraw SOL from it, and finally close the vault and recover its remaining lamports.

The program is built with Anchor and tested at the program level using LiteSVM.

---

# Overview

This project demonstrates the core mechanics of building a SOL vault on Solana using Program Derived Addresses (PDAs), Anchor account constraints, System Program CPIs, and PDA-signed transfers.

The vault separates two pieces of state:

```text
User
 │
 ├── Vault State PDA
 │
 └── Vault Account PDA
         │
         └── SOL / Lamports
```

The `VaultState` account stores the PDA bumps required by the program, while the `vault_account` is the PDA-controlled System Account that actually holds the SOL.

The user can:

1. initialize the vault;
2. deposit SOL;
3. withdraw SOL;
4. close the vault and recover the remaining balance.

---

# Protocol Architecture

The vault uses two PDAs.

### Vault State PDA

Derived from:

```text
["state", user]
```

This account stores the relationship between the user and the vault, together with the bumps required to sign for the vault account.

### Vault Account PDA

Derived from:

```text
["vault", vault_state]
```

This account holds the actual SOL.

The resulting architecture is:

```text
                     User
                      │
          ┌───────────┴───────────┐
          │                       │
          ▼                       ▼
   Vault State PDA          Vault Account PDA
   ["state", user]          ["vault", state]
          │                       │
          │ bumps                 │
          └──────────┐            │
                     │            │
                     └──────┬─────┘
                            │
                     PDA-signed CPI
                            │
                            ▼
                     System Program
```

The important distinction is that `VaultState` stores the vault metadata, while `VaultAccount` is where the lamports actually reside.

---

# Vault Lifecycle

The complete lifecycle is:

```text
                  initialize()
                       │
                       ▼
              ┌─────────────────┐
              │ Vault Initialized│
              └────────┬────────┘
                       │
                       ▼
                  deposit()
                       │
                       ▼
              ┌─────────────────┐
              │  SOL in Vault   │
              └────────┬────────┘
                       │
                  ┌────┴─────┐
                  │          │
              withdraw()   close()
                  │          │
                  ▼          ▼
             SOL returned  Vault closed
                  │          │
                  └────┬─────┘
                       ▼
                    Finished
```

---

# State Model

The `VaultState` account is imported from:

```rust
use crate::{constants::*, state::VaultState};
```

The supplied implementation uses the state account to store two PDA bumps:

```text
vault_bump
state_bump
```

These values are initialized during `initialize()` and later used to reconstruct the signer seeds for the vault account.

The test explicitly verifies both values:

```rust
assert_eq!(vault_state_data.vault_bump, vault_bump);
assert_eq!(vault_state_data.state_bump, state_bump);
```

This confirms that the on-chain state contains the bumps corresponding to the PDAs derived by the client.

---

# Initialize Flow

The `initialize` instruction creates the vault state and establishes the vault account.

The program entrypoint is:

```rust
pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
    ctx.accounts.initialize(ctx.bumps)
}
```

---

## Vault State PDA

The state account is initialized using:

```rust
#[account(
    init,
    payer = user,
    space = 8 + VaultState::INIT_SPACE,
    seeds = [STATE, user.key().as_ref()],
    bump
)]
pub vault_state: Account<'info, VaultState>,
```

The PDA is therefore derived from:

```text
STATE
+
user.public_key
```

This gives each user a separate vault state account.

Conceptually:

```text
User A → ["state", User A]
User B → ["state", User B]
User C → ["state", User C]
```

---

# Vault Account PDA

The actual vault is derived from the vault state:

```rust
#[account(
    mut,
    seeds = [VAULT_SEED, vault_state.key().as_ref()],
    bump
)]
pub vault_account: SystemAccount<'info>,
```

The derivation is therefore:

```text
VAULT_SEED
+
VaultState PDA
```

This creates a second level of PDA derivation:

```text
User
 │
 ▼
Vault State PDA
 │
 ▼
Vault Account PDA
```

The vault account is represented as:

```rust
SystemAccount<'info>
```

because it holds native SOL/lamports rather than SPL tokens.

---

# Initial Vault Funding

During initialization, the program calculates the amount of lamports required for the vault account's rent exemption:

```rust
let rent_exempt =
    Rent::get()?.minimum_balance(self.vault_account.data_len());
```

The user then funds the vault account through a System Program CPI:

```rust
let cpi_accounts = Transfer {
    from: self.user.to_account_info(),
    to: self.vault_account.to_account_info(),
};

let cpi_context =
    CpiContext::new(cpi_program, cpi_accounts);

transfer(cpi_context, rent_exempt)?;
```

The initialization therefore establishes the vault account with the lamports required for rent exemption.

---

# Storing PDA Bumps

After the vault has been initialized, the program stores both bumps:

```rust
self.vault_state.vault_bump = bumps.vault_account;
self.vault_state.state_bump = bumps.vault_state;
```

The `vault_bump` is particularly important because the program later needs it to sign for the vault account during withdrawals and closure.

---

# Deposit Flow

The `deposit` instruction allows the user to transfer SOL into the vault.

The program entrypoint is:

```rust
pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    ctx.accounts.deposit(amount)
}
```

The account constraints ensure that the supplied vault state and vault account correspond to the user's PDAs:

```rust
#[account(
    seeds = [STATE, user.key().as_ref()],
    bump
)]
pub vault_state: Account<'info, VaultState>,
```

and:

```rust
#[account(
    mut,
    seeds = [VAULT_SEED, vault_state.key().as_ref()],
    bump
)]
pub vault_account: SystemAccount<'info>,
```

---

## Deposit Validation

The program rejects zero-value deposits:

```rust
require!(
    amount > 0,
    ErrorCode::InvalidAmount
);
```

Therefore:

```text
amount == 0
    │
    ▼
InvalidAmount
```

while a positive amount proceeds to the System Program transfer.

---

# User → Vault Transfer

The actual SOL transfer is performed through the System Program:

```rust
let cpi_accounts = Transfer {
    from: self.user.to_account_info(),
    to: self.vault_account.to_account_info(),
};

let cpi_context =
    CpiContext::new(cpi_program, cpi_accounts);

transfer(cpi_context, amount)?;
```

The flow is:

```text
User
 │
 │ amount lamports
 ▼
Vault Account PDA
```

The user's signature authorizes the outgoing transfer.

---

# Withdraw Flow

The `withdraw` instruction allows the user to recover SOL from the vault.

The program entrypoint is:

```rust
pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
    ctx.accounts.withdraw(amount)
}
```

The same PDA relationships are enforced:

```text
["state", user]
        │
        ▼
Vault State PDA
        │
        ▼
["vault", vault_state]
        │
        ▼
Vault Account PDA
```

---

# Withdrawal Validation

The program first checks that the requested withdrawal is non-zero:

```rust
require!(
    amount > 0,
    ErrorCode::InvalidAmount
);
```

It then checks that the vault contains enough lamports:

```rust
require!(
    self.vault_account.lamports() >= amount,
    ErrorCode::InsufficientFunds
);
```

The withdrawal therefore cannot request more lamports than are currently held by the vault account.

---

# PDA-Signed Withdrawal

Unlike a deposit, the user is not the authority over the vault account.

The vault account is controlled by its PDA seeds:

```rust
let seeds = &[
    VAULT_SEED,
    self.vault_state.to_account_info().key.as_ref(),
    &[self.vault_state.vault_bump],
];

let signer_seeds = &[&seeds[..]];
```

The program then creates a System Program CPI using:

```rust
CpiContext::new_with_signer(
    cpi_program,
    cpi_accounts,
    signer_seeds
)
```

This allows the vault PDA to authorize the transfer.

The resulting flow is:

```text
             PDA signs
                 │
                 ▼
Vault Account ────────────> User
       │
       │ amount
       ▼
   System Program
```

This is one of the central mechanics demonstrated by the project:

> A Solana program can use PDA signer seeds to authorize a transfer from an account controlled by the PDA.

---

# Close Flow

The `close` instruction permanently closes the vault state and transfers the remaining vault balance to the user.

The program entrypoint is:

```rust
pub fn close(ctx: Context<Close>) -> Result<()> {
    ctx.accounts.close()
}
```

---

## Closing Vault State

The state account uses:

```rust
#[account(
    mut,
    seeds = [STATE, user.key().as_ref()],
    bump,
    close = user
)]
pub vault_state: Account<'info, VaultState>,
```

The:

```rust
close = user
```

constraint causes the `VaultState` account to be closed and its remaining lamports to be returned to the user.

---

# Draining the Vault

Before the vault account can be removed, its remaining lamports are transferred back to the user.

The program reconstructs the same PDA signer seeds used during withdrawal:

```rust
let seeds = &[
    VAULT_SEED,
    self.vault_state.to_account_info().key.as_ref(),
    &[self.vault_state.vault_bump],
];

let signer_seeds = &[&seeds[..]];
```

It then performs a PDA-signed System Program transfer:

```rust
let cpi_accounts = Transfer {
    from: self.vault_account.to_account_info(),
    to: self.user.to_account_info(),
};

let cpi_context =
    CpiContext::new_with_signer(
        cpi_program,
        cpi_accounts,
        signer_seeds
    );

transfer(
    cpi_context,
    self.vault_account.lamports()
)?;
```

The entire remaining balance is transferred.

So the close flow is:

```text
Vault Account PDA
       │
       │ all remaining lamports
       ▼
     User
       │
       │
       ▼
Vault State PDA
       │
       │ close = user
       ▼
     Closed
```

---

# Account Model

The program uses three important account relationships.

```text
┌──────────────────────────────┐
│            User              │
│                              │
│  signs transactions          │
│  pays initialization costs   │
│  deposits SOL                │
│  receives withdrawals        │
└──────────────┬───────────────┘
               │
               │ seeds = ["state", user]
               ▼
┌──────────────────────────────┐
│       Vault State PDA        │
│                              │
│  vault_bump                  │
│  state_bump                  │
└──────────────┬───────────────┘
               │
               │ seeds = ["vault", state]
               ▼
┌──────────────────────────────┐
│       Vault Account PDA      │
│                              │
│  holds SOL / lamports        │
└──────────────────────────────┘
```

The state PDA provides the persistent metadata required to interact with the vault, while the vault PDA holds the actual funds.

---

# Security Properties

## PDA-Based Account Isolation

Each user's vault state is derived from:

```text
["state", user]
```

This prevents the user from simply supplying an arbitrary vault state account.

The vault account is then derived from the corresponding vault state:

```text
["vault", vault_state]
```

This establishes a deterministic relationship between the two PDAs.

---

## User-Bound Vault State

The state PDA includes the user's public key in its seeds:

```rust
seeds = [STATE, user.key().as_ref()]
```

Therefore the user's vault state is tied to that user's public key.

---

## User-Bound Vault Access

The deposit, withdrawal, and close instructions all require:

```rust
pub user: Signer<'info>
```

The user therefore has to authorize the instruction.

---

## Positive Amount Validation

Both deposit and withdrawal reject zero-value operations:

```rust
require!(
    amount > 0,
    ErrorCode::InvalidAmount
);
```

This prevents meaningless zero-value transfers from proceeding through the instruction logic.

---

## Insufficient Balance Protection

Withdrawals explicitly verify:

```rust
self.vault_account.lamports() >= amount
```

before attempting the transfer.

This gives the instruction an explicit application-level insufficient-funds check.

---

## PDA-Signed Vault Transfers

The vault's outgoing transfers are not authorized by the user directly.

Instead, the program reconstructs:

```text
VAULT_SEED
+
VaultState PDA
+
vault bump
```

and uses those seeds with `new_with_signer`.

This establishes the vault PDA as the authority for outgoing SOL transfers.

---

# Testing with LiteSVM

The project uses LiteSVM to execute the compiled Anchor program in an SVM test environment.

The test setup loads the compiled program:

```rust
let program_id = anchor_vault::id();

let mut svm = LiteSVM::new();

let bytes = include_bytes!(concat!(
    env!("CARGO_TARGET_TMPDIR"),
    "/../deploy/anchor_vault.so"
));

svm.add_program(program_id, bytes).unwrap();
```

A test payer is then created and funded:

```rust
let payer = Keypair::new();

svm.airdrop(
    &payer.pubkey(),
    10_000_000_000
).unwrap();
```

The tests therefore execute against the actual compiled program rather than mocking the instruction handlers.

---

# Integration Test

The supplied test:

```text
test_initialize_deposit_withdraw_close
```

covers the complete vault lifecycle:

```text
Initialize
    ↓
Deposit 1 SOL
    ↓
Withdraw 0.5 SOL
    ↓
Close Vault
```

---

## PDA Derivation in the Test

The test independently derives the two PDAs:

```rust
let (vault_state_pda, state_bump) =
    Pubkey::find_program_address(
        &[b"state", user.as_ref()],
        &anchor_vault::id()
    );
```

and:

```rust
let (vault_account_pda, vault_bump) =
    Pubkey::find_program_address(
        &[b"vault", vault_state_pda.as_ref()],
        &anchor_vault::id()
    );
```

This mirrors the seeds used by the program.

---

# Test: Initialize

The test constructs the `Initialize` instruction and sends it through LiteSVM.

After successful execution, it reads the vault state account:

```rust
let vault_state_account =
    svm.get_account(&vault_state_pda).unwrap();
```

and deserializes it:

```rust
let vault_state_data =
    anchor_vault::state::VaultState::try_deserialize(
        &mut vault_state_account.data.as_ref()
    ).unwrap();
```

The stored bumps are then verified:

```rust
assert_eq!(
    vault_state_data.vault_bump,
    vault_bump
);

assert_eq!(
    vault_state_data.state_bump,
    state_bump
);
```

This confirms that initialization correctly persisted the PDA bump values.

---

# Test: Deposit

The test deposits:

```text
1 SOL
```

or:

```rust
1_000_000_000
```

lamports.

Before the transaction, it records:

```rust
let vault_balance_before_deposit =
    svm.get_balance(&vault_account_pda).unwrap();
```

After the deposit:

```rust
let balance_after_deposit =
    svm.get_balance(&vault_account_pda).unwrap();
```

The test verifies the exact balance increase:

```rust
assert_eq!(
    balance_after_deposit - vault_balance_before_deposit,
    deposit_amount,
    "vault should grow by exactly the deposited amount"
);
```

This verifies that the vault receives exactly the requested amount.

---

# Test: Withdraw

The test then withdraws:

```text
0.5 SOL
```

or:

```rust
500_000_000
```

lamports.

The vault balance is captured before the withdrawal:

```rust
let vault_balance_before_withdraw =
    svm.get_balance(&vault_account_pda).unwrap();
```

After the transaction:

```rust
let balance_after_withdraw =
    svm.get_balance(&vault_account_pda).unwrap();
```

The test verifies:

```rust
assert_eq!(
    vault_balance_before_withdraw - balance_after_withdraw,
    withdraw_amount,
    "vault should shrink by exactly the withdrawn amount"
);
```

This demonstrates that the PDA-signed withdrawal successfully transfers the requested amount from the vault.

---

# Test: Close

Finally, the test captures the remaining vault balance:

```rust
let close_amount =
    svm.get_balance(&vault_account_pda).unwrap();
```

It sends the `Close` instruction.

The test then verifies that both accounts no longer exist:

```rust
assert!(
    svm.get_account(&vault_account_pda).is_none()
);

assert!(
    svm.get_account(&vault_state_pda).is_none()
);
```

The user's balance is also checked:

```rust
let user_balance_after_close =
    svm.get_balance(&user).unwrap();

assert!(
    user_balance_after_close > close_amount
);
```

This confirms that the vault was closed and the remaining balance was returned to the user.

---

# Complete Tested Lifecycle

The current LiteSVM test therefore demonstrates:

```text
                    Initialize
                        │
                        ▼
              ┌─────────────────┐
              │ Vault State PDA │
              │ Vault Account   │
              └────────┬────────┘
                       │
                       ▼
                 Deposit 1 SOL
                       │
                       ▼
              ┌─────────────────┐
              │   Vault +1 SOL  │
              └────────┬────────┘
                       │
                       ▼
               Withdraw 0.5 SOL
                       │
                       ▼
              ┌─────────────────┐
              │ Vault -0.5 SOL  │
              └────────┬────────┘
                       │
                       ▼
                    Close
                       │
              ┌────────┴────────┐
              ▼                 ▼
       Vault account      State account
          closed              closed
              │
              └───────┬────────┘
                      ▼
              Remaining SOL
                 → User
```

---

# Testing Status

The supplied test suite currently covers the complete happy-path lifecycle:

* LiteSVM program initialization;
* program loading;
* user funding;
* Vault State PDA derivation;
* Vault Account PDA derivation;
* vault initialization;
* PDA bump persistence;
* SOL deposit;
* exact deposit balance increase;
* SOL withdrawal;
* exact withdrawal balance decrease;
* vault closure;
* vault state closure;
* return of remaining vault balance to the user.

The supplied tests do **not** currently demonstrate separate failure-path tests for:

* zero-value deposits;
* zero-value withdrawals;
* insufficient funds;
* invalid vault state PDA;
* invalid vault account PDA;
* unauthorized user access.

Those validations exist in the instruction implementation, but they are not independently exercised by the supplied integration
