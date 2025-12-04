use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("Jackpot11111111111111111111111111111");

#[account]
pub struct JackpotPool {
    pub total_pool: u64,
    pub total_bets: u64,
    pub rake_bps: u16, // 500 = 5%
    pub bump: u8,
}

impl JackpotPool {
    pub const LEN: usize = 8 + 8 + 8 + 2 + 1;
}

#[program]
pub mod jackpot {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, rake_bps: u16) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.total_pool = 0;
        pool.total_bets = 0;
        pool.rake_bps = rake_bps;
        pool.bump = ctx.bumps.pool;
        Ok(())
    }

    pub fn place_bet(ctx: Context<PlaceBet>, bet_amount: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        
        // Transfer bet amount from player to pool
        let cpi_accounts = Transfer {
            from: ctx.accounts.player_token_account.to_account_info(),
            to: ctx.accounts.pool_token_account.to_account_info(),
            authority: ctx.accounts.player.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, bet_amount)?;

        // Calculate rake (5% goes to house)
        let rake = bet_amount
            .checked_mul(pool.rake_bps as u64)
            .and_then(|x| x.checked_div(10000))
            .ok_or(common::CasinoError::MathOverflow)?;
        
        let pool_contribution = bet_amount
            .checked_sub(rake)
            .ok_or(common::CasinoError::MathOverflow)?;

        pool.total_pool = pool.total_pool
            .checked_add(pool_contribution)
            .ok_or(common::CasinoError::MathOverflow)?;
        pool.total_bets = pool.total_bets
            .checked_add(1)
            .ok_or(common::CasinoError::MathOverflow)?;

        Ok(())
    }

    pub fn draw_winner(ctx: Context<DrawWinner>) -> Result<()> {
        let pool = &ctx.accounts.pool;
        require!(pool.total_pool > 0, common::CasinoError::InsufficientFunds);

        // Generate random winner from recent bets
        let seed = Clock::get()?.unix_timestamp.to_le_bytes();
        let winner_index = common::generate_random_from_seed(&seed, pool.total_bets - 1);

        // Transfer entire pool to winner
        let seeds = &[
            b"pool",
            ctx.accounts.pool.to_account_info().key.as_ref(),
            &[pool.bump],
        ];
        let signer = &[&seeds[..]];
        
        let cpi_accounts = Transfer {
            from: ctx.accounts.pool_token_account.to_account_info(),
            to: ctx.accounts.winner_token_account.to_account_info(),
            authority: ctx.accounts.pool.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, pool.total_pool)?;

        // Reset pool
        let pool = &mut ctx.accounts.pool;
        pool.total_pool = 0;
        pool.total_bets = 0;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = JackpotPool::LEN,
        seeds = [b"pool", b"jackpot"],
        bump
    )]
    pub pool: Account<'info, JackpotPool>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct PlaceBet<'info> {
    #[account(mut)]
    pub pool: Account<'info, JackpotPool>,
    
    #[account(mut)]
    pub player: Signer<'info>,
    
    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = pool_token_account.owner == pool.key()
    )]
    pub pool_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct DrawWinner<'info> {
    #[account(mut)]
    pub pool: Account<'info, JackpotPool>,
    
    /// CHECK: Winner account
    pub winner: UncheckedAccount<'info>,
    
    #[account(mut)]
    pub pool_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub winner_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

