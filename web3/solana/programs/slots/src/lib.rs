use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("Slots11111111111111111111111111111111");

#[program]
pub mod slots {
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

    pub fn spin(ctx: Context<Spin>, bet_amount: u64) -> Result<()> {
        let config = &ctx.accounts.config;
        require!(!config.paused, common::CasinoError::GameNotActive);
        
        common::validate_bet(bet_amount, config.min_bet, config.max_bet)?;

        // Transfer bet amount from player to treasury
        let cpi_accounts = Transfer {
            from: ctx.accounts.player_token_account.to_account_info(),
            to: ctx.accounts.treasury_token_account.to_account_info(),
            authority: ctx.accounts.player.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, bet_amount)?;

        // Generate 3 random symbols (0-6: Cherry, Lemon, Orange, Plum, Bell, Bar, Seven)
        let seed = Clock::get()?.unix_timestamp.to_le_bytes();
        let reel1 = common::generate_random_from_seed(&seed, 6);
        let reel2 = common::generate_random_from_seed(&[seed[0].wrapping_add(1)], 6);
        let reel3 = common::generate_random_from_seed(&[seed[0].wrapping_add(2)], 6);

        // Calculate payout based on symbol combinations
        let multiplier_bps = calculate_payout_multiplier(reel1, reel2, reel3);

        let game_state = &mut ctx.accounts.game_state;
        game_state.player = ctx.accounts.player.key();
        game_state.bet_amount = bet_amount;
        game_state.game_id = Clock::get()?.unix_timestamp as u64;
        game_state.timestamp = Clock::get()?.unix_timestamp;
        game_state.settled = false;
        game_state.result = Some((reel1 << 16) | (reel2 << 8) | reel3); // Pack reels into u64
        game_state.bump = ctx.bumps.game_state;

        let payout = common::calculate_payout(bet_amount, multiplier_bps, config.house_edge_bps)?;
        game_state.payout = Some(payout);
        
        if payout > 0 {
            // Transfer payout to player
            let seeds = &[
                b"treasury",
                ctx.accounts.config.to_account_info().key.as_ref(),
                &[ctx.accounts.config.bump],
            ];
            let signer = &[&seeds[..]];
            
            let cpi_accounts = Transfer {
                from: ctx.accounts.treasury_token_account.to_account_info(),
                to: ctx.accounts.player_token_account.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
            token::transfer(cpi_ctx, payout)?;
        }

        game_state.settled = true;
        Ok(())
    }

    fn calculate_payout_multiplier(reel1: u64, reel2: u64, reel3: u64) -> u64 {
        // Three of a kind
        if reel1 == reel2 && reel2 == reel3 {
            match reel1 {
                6 => 250000, // Three Sevens = 25x
                5 => 100000, // Three Bars = 10x
                4 => 50000,  // Three Bells = 5x
                _ => 20000,  // Three others = 2x
            }
        }
        // Two of a kind
        else if reel1 == reel2 || reel2 == reel3 || reel1 == reel3 {
            10000 // 1x
        }
        // No match
        else {
            0
        }
    }

    pub fn pause(ctx: Context<Pause>) -> Result<()> {
        let config = &mut ctx.accounts.config;
        require!(
            ctx.accounts.authority.key() == config.authority,
            common::CasinoError::Unauthorized
        );
        config.paused = true;
        Ok(())
    }

    pub fn unpause(ctx: Context<Pause>) -> Result<()> {
        let config = &mut ctx.accounts.config;
        require!(
            ctx.accounts.authority.key() == config.authority,
            common::CasinoError::Unauthorized
        );
        config.paused = false;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = common::GameConfig::LEN,
        seeds = [b"config", b"slots"],
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
pub struct Spin<'info> {
    #[account(
        seeds = [b"config", b"slots"],
        bump = config.bump
    )]
    pub config: Account<'info, common::GameConfig>,
    
    #[account(mut)]
    pub player: Signer<'info>,
    
    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub treasury_token_account: Account<'info, TokenAccount>,
    
    /// CHECK: Treasury PDA
    #[account(
        seeds = [b"treasury", config.key().as_ref()],
        bump
    )]
    pub treasury: UncheckedAccount<'info>,
    
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

#[derive(Accounts)]
pub struct Pause<'info> {
    #[account(
        mut,
        seeds = [b"config", b"slots"],
        bump = config.bump
    )]
    pub config: Account<'info, common::GameConfig>,
    pub authority: Signer<'info>,
}

