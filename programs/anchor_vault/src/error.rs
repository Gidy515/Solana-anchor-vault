use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Only the counter authority can update this counter")]
    Unauthorized,
    #[msg("Counter has reached the maximum value")]
    CounterOverflow,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Insufficient funds in the vault")]
    InsufficientFunds,
}
