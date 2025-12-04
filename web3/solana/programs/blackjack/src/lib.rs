use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("Blackjack1111111111111111111111111");

#[account]
pub struct BlackjackGame {
    pub player: Pubkey,
    pub bet_amount: u64,
    pub player_cards: Vec<u8>, // Card values (1-13, suit encoded in upper bits)
    pub dealer_cards: Vec<u8>,
    pub player_score: u8,
    pub dealer_score: u8,
    pub game_state: u8, // 0=betting, 1=playing, 2=settled
    pub timestamp: i64,
    pub bump: u8,
}

impl BlackjackGame {
    pub const LEN: usize = 8 + 32 + 8 + 4 + 4 + 1 + 1 + 1 + 8 + 1 + 200; // Extra space for vectors
}

#[program]
pub mod blackjack {
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

    pub fn place_bet(ctx: Context<PlaceBet>, bet_amount: u64) -> Result<()> {
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

        // Deal initial cards
        let seed = Clock::get()?.unix_timestamp.to_le_bytes();
        let player_card1 = (common::generate_random_from_seed(&seed, 12) + 1) as u8;
        let player_card2 = (common::generate_random_from_seed(&[seed[0].wrapping_add(1)], 12) + 1) as u8;
        let dealer_card1 = (common::generate_random_from_seed(&[seed[0].wrapping_add(2)], 12) + 1) as u8;

        let mut game = &mut ctx.accounts.game;
        game.player = ctx.accounts.player.key();
        game.bet_amount = bet_amount;
        game.player_cards = vec![player_card1, player_card2];
        game.dealer_cards = vec![dealer_card1];
        game.player_score = calculate_score(&game.player_cards);
        game.dealer_score = calculate_score(&game.dealer_cards);
        game.game_state = 1; // Playing
        game.timestamp = Clock::get()?.unix_timestamp;
        game.bump = ctx.bumps.game;

        // Check for blackjack
        if game.player_score == 21 {
            game.game_state = 2; // Settled
            let payout = bet_amount
                .checked_mul(3)
                .and_then(|x| x.checked_div(2))
                .ok_or(common::CasinoError::MathOverflow)?; // 3:2 payout
            settle_game(ctx, payout)?;
        }

        Ok(())
    }

    pub fn hit(ctx: Context<Hit>) -> Result<()> {
        let game = &mut ctx.accounts.game;
        require!(game.game_state == 1, common::CasinoError::InvalidGameState);
        require!(game.player == ctx.accounts.player.key(), common::CasinoError::Unauthorized);

        let seed = Clock::get()?.unix_timestamp.to_le_bytes();
        let new_card = (common::generate_random_from_seed(&seed, 12) + 1) as u8;
        game.player_cards.push(new_card);
        game.player_score = calculate_score(&game.player_cards);

        if game.player_score > 21 {
            game.game_state = 2; // Bust
            settle_game(ctx, 0)?;
        }

        Ok(())
    }

    pub fn stand(ctx: Context<Stand>) -> Result<()> {
        let game = &mut ctx.accounts.game;
        require!(game.game_state == 1, common::CasinoError::InvalidGameState);
        require!(game.player == ctx.accounts.player.key(), common::CasinoError::Unauthorized);

        // Dealer draws until 17+
        let seed = Clock::get()?.unix_timestamp.to_le_bytes();
        while game.dealer_score < 17 {
            let new_card = (common::generate_random_from_seed(&seed, 12) + 1) as u8;
            game.dealer_cards.push(new_card);
            game.dealer_score = calculate_score(&game.dealer_cards);
        }

        game.game_state = 2;

        // Determine winner
        let payout = if game.dealer_score > 21 || game.player_score > game.dealer_score {
            game.bet_amount * 2 // 1:1 payout
        } else if game.player_score == game.dealer_score {
            game.bet_amount // Push
        } else {
            0 // Dealer wins
        };

        settle_game(ctx, payout)
    }

    fn settle_game(ctx: Context<Hit>, payout: u64) -> Result<()> {
        let game = &ctx.accounts.game;
        let config = &ctx.accounts.config;

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

    fn calculate_score(cards: &[u8]) -> u8 {
        let mut score = 0;
        let mut aces = 0;
        
        for &card in cards {
            let value = card % 13;
            if value == 0 {
                aces += 1;
                score += 11;
            } else if value >= 10 {
                score += 10;
            } else {
                score += value + 1;
            }
        }
        
        while score > 21 && aces > 0 {
            score -= 10;
            aces -= 1;
        }
        
        score
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = common::GameConfig::LEN,
        seeds = [b"config", b"blackjack"],
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
        seeds = [b"config", b"blackjack"],
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
        space = BlackjackGame::LEN,
        seeds = [b"game", player.key().as_ref(), &Clock::get()?.unix_timestamp.to_le_bytes()],
        bump
    )]
    pub game: Account<'info, BlackjackGame>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct Hit<'info> {
    #[account(
        seeds = [b"config", b"blackjack"],
        bump = config.bump
    )]
    pub config: Account<'info, common::GameConfig>,
    
    #[account(mut)]
    pub player: Signer<'info>,
    
    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub treasury_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub game: Account<'info, BlackjackGame>,
    
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct Stand<'info> {
    #[account(
        seeds = [b"config", b"blackjack"],
        bump = config.bump
    )]
    pub config: Account<'info, common::GameConfig>,
    
    #[account(mut)]
    pub player: Signer<'info>,
    
    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub treasury_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub game: Account<'info, BlackjackGame>,
    
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

