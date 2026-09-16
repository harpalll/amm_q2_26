use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub seed: u64,                 // Seed to be able to create different pools / configs
    pub authority: Option<Pubkey>, // Admin that can lock the pool (None = immutable)
    pub mint_x: Pubkey,            // Token X
    pub mint_y: Pubkey,            // Token Y
    pub fee: u16,                  // Total swap fee in basis points (10_000 = 100%)
    pub treasury: Pubkey,          // Protocol fee recipient (owner of treasury_x / treasury_y ATAs)
    pub locked: bool,              // If true, deposit / withdraw / swap are disabled
    pub config_bump: u8,           // Bump seed for the config account
    pub lp_bump: u8,               // Bump seed for the LP token
}
