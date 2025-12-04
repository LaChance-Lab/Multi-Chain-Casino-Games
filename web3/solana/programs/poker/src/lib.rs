use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("Poker11111111111111111111111111111111");

#[account]
pub struct PokerTournament {
    pub buy_in: u64,
    pub prize_pool: u64,
    pub players: Vec<Pubkey>,
    pub max_players: u8,
    pub status: u8, // 0=waiting, 1=active, 2=finished
    pub winner: Option<Pubkey>,
    pub bump: u8,
}

impl PokerTournament {
    pub const LEN: usize = 8 + 8 + 8 + 4 + 1 + 1 + 1 + 33 + 1 + 200; // Extra space for players
}

#[program]
pub mod poker {
    use super::*;

    pub fn create_tournament(ctx: Context<CreateTournament>, buy_in: u64, max_players: u8) -> Result<()> {
        let tournament = &mut ctx.accounts.tournament;
        tournament.buy_in = buy_in;
        tournament.prize_pool = 0;
        tournament.players = vec![];
        tournament.max_players = max_players;
        tournament.status = 0;
        tournament.winner = None;
        tournament.bump = ctx.bumps.tournament;
        Ok(())
    }

    pub fn join_tournament(ctx: Context<JoinTournament>) -> Result<()> {
        let tournament = &mut ctx.accounts.tournament;
        require!(tournament.status == 0, common::CasinoError::InvalidGameState);
        require!(tournament.players.len() < tournament.max_players as usize, common::CasinoError::InvalidGameState);

        // Transfer buy-in
        let cpi_accounts = Transfer {
            from: ctx.accounts.player_token_account.to_account_info(),
            to: ctx.accounts.tournament_token_account.to_account_info(),
            authority: ctx.accounts.player.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, tournament.buy_in)?;

        tournament.players.push(ctx.accounts.player.key());
        tournament.prize_pool = tournament.prize_pool
            .checked_add(tournament.buy_in)
            .ok_or(common::CasinoError::MathOverflow)?;

        Ok(())
    }

    pub fn start_tournament(ctx: Context<StartTournament>) -> Result<()> {
        let tournament = &mut ctx.accounts.tournament;
        require!(tournament.status == 0, common::CasinoError::InvalidGameState);
        require!(tournament.players.len() >= 2, common::CasinoError::InvalidGameState);
        tournament.status = 1;
        Ok(())
    }

    pub fn end_tournament(ctx: Context<EndTournament>, winner_index: u8) -> Result<()> {
        let tournament = &mut ctx.accounts.tournament;
        require!(tournament.status == 1, common::CasinoError::InvalidGameState);
        require!((winner_index as usize) < tournament.players.len(), common::CasinoError::InvalidBetAmount);

        let winner = tournament.players[winner_index as usize];
        tournament.winner = Some(winner);
        tournament.status = 2;

        // Transfer prize pool to winner
        let seeds = &[
            b"tournament",
            ctx.accounts.tournament.to_account_info().key.as_ref(),
            &[tournament.bump],
        ];
        let signer = &[&seeds[..]];
        
        let cpi_accounts = Transfer {
            from: ctx.accounts.tournament_token_account.to_account_info(),
            to: ctx.accounts.winner_token_account.to_account_info(),
            authority: ctx.accounts.tournament.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, tournament.prize_pool)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct CreateTournament<'info> {
    #[account(
        init,
        payer = authority,
        space = PokerTournament::LEN,
        seeds = [b"tournament", authority.key().as_ref(), &Clock::get()?.unix_timestamp.to_le_bytes()],
        bump
    )]
    pub tournament: Account<'info, PokerTournament>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub system_program: Program<'info, System>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct JoinTournament<'info> {
    #[account(mut)]
    pub tournament: Account<'info, PokerTournament>,
    
    #[account(mut)]
    pub player: Signer<'info>,
    
    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub tournament_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct StartTournament<'info> {
    #[account(mut)]
    pub tournament: Account<'info, PokerTournament>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct EndTournament<'info> {
    #[account(mut)]
    pub tournament: Account<'info, PokerTournament>,
    
    /// CHECK: Winner account
    pub winner: UncheckedAccount<'info>,
    
    #[account(mut)]
    pub tournament_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub winner_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
}

