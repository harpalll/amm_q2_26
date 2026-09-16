use {
    anchor_lang::{
        solana_program::instruction::Instruction, InstructionData, ToAccountMetas,
    },
    solana_keypair::Keypair,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
};

pub fn create_lock_ix(authority: &Keypair, config: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        amm_video::id(),
        &amm_video::instruction::Lock {}.data(),
        amm_video::accounts::Lock {
            authority: authority.pubkey(),
            config,
        }
        .to_account_metas(None),
    )
}
