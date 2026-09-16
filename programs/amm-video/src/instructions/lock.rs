use anchor_lang::prelude::*;

use crate::{error::AmmError, state::Config};

#[derive(Accounts)]
pub struct Lock<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [b"config", config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,
}

impl<'info> Lock<'info> {
    pub fn lock(&mut self) -> Result<()> {
        require!(!self.config.locked, AmmError::AlreadyLocked);
        require!(
            self.config.authority.is_some(),
            AmmError::NoAuthoritySet
        );
        require!(
            self.config.authority.unwrap() == self.authority.key(),
            AmmError::InvalidAuthority
        );

        self.config.locked = true;
        Ok(())
    }
}
