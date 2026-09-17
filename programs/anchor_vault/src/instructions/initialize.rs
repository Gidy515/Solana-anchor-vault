use anchor_lang::{prelude::*, system_program::{transfer, Transfer}};

use crate::{constants::*, state::VaultState};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        init,
        payer = user,
        space = 8 + VaultState::INIT_SPACE,
        seeds = [STATE, user.key().as_ref()],
        bump
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        seeds = [VAULT, vault_state.key().as_ref()],
        bump
    )]
    pub vault_account: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> Initialize <'info> {
    pub fn initialize(&mut self, bumps: InitializeBumps) -> Result<()> {
        //get and pass in the required amount of sol needed for rent exemption for the vault account
        let rent_exempt = Rent::get()?.minimum_balance(self.vault_account.data_len());

        let cpi_program = self.system_program.key();

        let cpi_accounts = Transfer {
            from: self.user.to_account_info(),
            to: self.vault_account.to_account_info(),
        };

        let cpi_context = CpiContext::new(cpi_program, cpi_accounts);

        transfer(cpi_context, rent_exempt)?;

        // save data to vault_state onchain
        self.vault_state.vault_bump = bumps.vault_state;
        self.vault_state.state_bump = bumps.vault_account;
        Ok(())
    }
}


