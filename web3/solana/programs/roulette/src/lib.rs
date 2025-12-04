use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("Roulette111111111111111111111111111");

#[program]
pub mod roulette {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, min_bet: u64, max_bet: u64, house_edge_bps: u16) -> Result<()> {
        let config = &mut ctx.accounts.config;
        config.authority = ctx.accounts.authority.key();
        config.treasury = ctx.accounts.treasury.key();
        config.min_bet = min_bet;
        config.max_bet = max_bet;
        config.house_edge_bps = house_edge_bps;
        config.paused = false;
        config.bump = ctx.bumps.config;
        Ok(())
    }

    pub fn place_bet(ctx: Context<PlaceBet>, bet_amount: u64, bet_type: u8, bet_value: u8) -> Result<()> {
        let config = &ctx.accounts.config;
        require!(!config.paused, common::CasinoError::GameNotActive);
        common::validate_bet(bet_amount, config.min_bet, config.max_bet)?;

        // Transfer bet
        let cpi_accounts = Transfer {
            from: ctx.accounts.player_token_account.to_account_info(),
            to: ctx.accounts.treasury_token_account.to_account_info(),
            authority: ctx.accounts.player.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, bet_amount)?;

        // Spin wheel (0-36 for European, 0-37 for American with 00)
        let seed = Clock::get()?.unix_timestamp.to_le_bytes();
        let winning_number = common::generate_random_from_seed(&seed, 36);

        // Calculate payout based on bet type
        let multiplier_bps = match bet_type {
            0 => 360000, // Single number (35:1)
            1 => 20000,  // Red/Black (1:1)
            2 => 20000,  // Odd/Even (1:1)
            3 => 20000,  // High/Low (1:1)
            4 => 30000,  // Dozen (2:1)
            5 => 30000,  // Column (2:1)
            _ => return Err(common::CasinoError::InvalidBetAmount.into()),
        };

        let won = check_win(winning_number, bet_type, bet_value);
        let payout = if won {
            common::calculate_payout(bet_amount, multiplier_bps, config.house_edge_bps)?
        } else {
            0
        };

        let game_state = &mut ctx.accounts.game_state;
        game_state.player = ctx.accounts.player.key();
        game_state.bet_amount = bet_amount;
        game_state.game_id = Clock::get()?.unix_timestamp as u64;
        game_state.timestamp = Clock::get()?.unix_timestamp;
        game_state.settled = true;
        game_state.result = Some(winning_number as u64);
        game_state.payout = Some(payout);

        if payout > 0 {
            let seeds = &[
                b"treasury",
                config.to_account_info().key.as_ref(),
                &[config.bump],
            ];
            let signer = &[&seeds[..]];
            
            let cpi_accounts = Transfer {
                from: ctx.accounts.treasury_token_account.to_account_info(),
                to: ctx.accounts.player_token_account.to_account_info(),
                authority: config.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
            token::transfer(cpi_ctx, payout)?;
        }

        Ok(())
    }

    fn check_win(winning_number: u64, bet_type: u8, bet_value: u8) -> bool {
        match bet_type {
            0 => winning_number == bet_value as u64, // Single number
            1 => (winning_number % 2 == 1) == (bet_value == 1), // Red/Black
            2 => (winning_number % 2 == 0) == (bet_value == 1), // Odd/Even
            3 => (winning_number > 18) == (bet_value == 1), // High/Low
            4 => (winning_number / 12) == bet_value as u64, // Dozen
            5 => (winning_number % 3) == bet_value as u64, // Column
            _ => false,
        }
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = common::GameConfig::LEN,
        seeds = [b"config", b"roulette"],
        bump
    )]
    pub config: Account<'info, common::GameConfig>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    /// CHECK: Treasury account
    pub treasury: UncheckedAccount<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct PlaceBet<'info> {
    #[account(
        seeds = [b"config", b"roulette"],
        bump = config.bump
    )]
    pub config: Account<'info, common::GameConfig>,
    
    #[account(mut)]
    pub player: Signer<'info>,
    
    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub treasury_token_account: Account<'info, TokenAccount>,
    
    #[account(
        init,
        payer = player,
        space = common::GameState::LEN,
        seeds = [b"game", player.key().as_ref(), &Clock::get()?.unix_timestamp.to_le_bytes()],
        bump
    )]
    pub game_state: Account<'info, common::GameState>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub clock: Sysvar<'info, Clock>,
}

