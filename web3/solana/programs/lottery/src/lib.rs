use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("Lottery1111111111111111111111111111");

#[account]
pub struct LotteryTicket {
    pub player: Pubkey,
    pub numbers: [u8; 6],
    pub bet_amount: u64,
    pub draw_id: u64,
    pub timestamp: i64,
    pub bump: u8,
}

impl LotteryTicket {
    pub const LEN: usize = 8 + 32 + 6 + 8 + 8 + 8 + 1;
}

#[account]
pub struct LotteryDraw {
    pub draw_id: u64,
    pub winning_numbers: [u8; 6],
    pub prize_pool: u64,
    pub tickets: u64,
    pub drawn: bool,
    pub timestamp: i64,
    pub bump: u8,
}

impl LotteryDraw {
    pub const LEN: usize = 8 + 8 + 6 + 8 + 8 + 1 + 8 + 1;
}

#[program]
pub mod lottery {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, min_bet: u64, max_bet: u64) -> Result<()> {
        let config = &mut ctx.accounts.config;
        config.authority = ctx.accounts.authority.key();
        config.treasury = ctx.accounts.treasury.key();
        config.min_bet = min_bet;
        config.max_bet = max_bet;
        config.house_edge_bps = 0; // No house edge for lottery
        config.paused = false;
        config.bump = ctx.bumps.config;
        Ok(())
    }

    pub fn buy_ticket(ctx: Context<BuyTicket>, bet_amount: u64, numbers: [u8; 6], draw_id: u64) -> Result<()> {
        let config = &ctx.accounts.config;
        require!(!config.paused, common::CasinoError::GameNotActive);
        common::validate_bet(bet_amount, config.min_bet, config.max_bet)?;
        
        // Validate numbers (1-49)
        for &num in &numbers {
            require!(num >= 1 && num <= 49, common::CasinoError::InvalidBetAmount);
        }

        // Transfer bet to prize pool
        let cpi_accounts = Transfer {
            from: ctx.accounts.player_token_account.to_account_info(),
            to: ctx.accounts.pool_token_account.to_account_info(),
            authority: ctx.accounts.player.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, bet_amount)?;

        // Update draw pool
        let draw = &mut ctx.accounts.draw;
        draw.prize_pool = draw.prize_pool
            .checked_add(bet_amount)
            .ok_or(common::CasinoError::MathOverflow)?;
        draw.tickets = draw.tickets
            .checked_add(1)
            .ok_or(common::CasinoError::MathOverflow)?;

        // Create ticket
        let ticket = &mut ctx.accounts.ticket;
        ticket.player = ctx.accounts.player.key();
        ticket.numbers = numbers;
        ticket.bet_amount = bet_amount;
        ticket.draw_id = draw_id;
        ticket.timestamp = Clock::get()?.unix_timestamp;
        ticket.bump = ctx.bumps.ticket;

        Ok(())
    }

    pub fn draw_numbers(ctx: Context<DrawNumbers>) -> Result<()> {
        let draw = &mut ctx.accounts.draw;
        require!(!draw.drawn, common::CasinoError::GameAlreadySettled);

        // Generate 6 random numbers (1-49)
        let seed = Clock::get()?.unix_timestamp.to_le_bytes();
        let mut numbers = [0u8; 6];
        let mut used = [false; 50];
        
        for i in 0..6 {
            let mut num;
            loop {
                num = (common::generate_random_from_seed(&[seed[i]], 48) + 1) as u8;
                if !used[num as usize] {
                    used[num as usize] = true;
                    break;
                }
            }
            numbers[i] = num;
        }
        
        numbers.sort();
        draw.winning_numbers = numbers;
        draw.drawn = true;
        draw.timestamp = Clock::get()?.unix_timestamp;

        Ok(())
    }

    pub fn claim_prize(ctx: Context<ClaimPrize>, matching_numbers: u8) -> Result<()> {
        let ticket = &ctx.accounts.ticket;
        let draw = &ctx.accounts.draw;
        require!(draw.drawn, common::CasinoError::InvalidGameState);
        require!(ticket.player == ctx.accounts.player.key(), common::CasinoError::Unauthorized);

        // Calculate matches
        let mut matches = 0;
        for &num in &ticket.numbers {
            if draw.winning_numbers.contains(&num) {
                matches += 1;
            }
        }

        require!(matches == matching_numbers, common::CasinoError::InvalidBetAmount);

        // Calculate prize based on matches
        let prize = match matches {
            6 => draw.prize_pool, // Jackpot
            5 => draw.prize_pool / 10,
            4 => draw.prize_pool / 100,
            _ => 0,
        };

        if prize > 0 {
            let seeds = &[
                b"pool",
                ctx.accounts.draw.to_account_info().key.as_ref(),
                &[draw.bump],
            ];
            let signer = &[&seeds[..]];
            
            let cpi_accounts = Transfer {
                from: ctx.accounts.pool_token_account.to_account_info(),
                to: ctx.accounts.player_token_account.to_account_info(),
                authority: ctx.accounts.draw.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
            token::transfer(cpi_ctx, prize)?;
        }

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = common::GameConfig::LEN,
        seeds = [b"config", b"lottery"],
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
pub struct BuyTicket<'info> {
    #[account(
        seeds = [b"config", b"lottery"],
        bump = config.bump
    )]
    pub config: Account<'info, common::GameConfig>,
    
    #[account(mut)]
    pub player: Signer<'info>,
    
    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub pool_token_account: Account<'info, TokenAccount>,
    
    #[account(
        init_if_needed,
        payer = player,
        space = LotteryDraw::LEN,
        seeds = [b"draw", &draw_id.to_le_bytes()],
        bump
    )]
    pub draw: Account<'info, LotteryDraw>,
    
    #[account(
        init,
        payer = player,
        space = LotteryTicket::LEN,
        seeds = [b"ticket", player.key().as_ref(), &Clock::get()?.unix_timestamp.to_le_bytes()],
        bump
    )]
    pub ticket: Account<'info, LotteryTicket>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct DrawNumbers<'info> {
    #[account(mut)]
    pub draw: Account<'info, LotteryDraw>,
    pub authority: Signer<'info>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct ClaimPrize<'info> {
    #[account(mut)]
    pub ticket: Account<'info, LotteryTicket>,
    
    #[account(mut)]
    pub draw: Account<'info, LotteryDraw>,
    
    #[account(mut)]
    pub player: Signer<'info>,
    
    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub pool_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
}

