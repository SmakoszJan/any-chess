use std::{
    cmp::Ordering,
    fmt::Display,
    ops::{Index, IndexMut, Not},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

pub mod net;

#[cfg(test)]
pub mod tests {
    use std::str::FromStr;

    use crate::{Board, ChessError, ChessMove, Color, Kind, Move};

    #[test]
    fn empty_squares_do_not_move() {
        let board = Board::empty();

        assert_eq!(
            ChessMove::new((0, 0), (0, 0)).check(&board),
            Err(ChessError::Empty)
        );
    }

    #[test]
    fn pieces_cant_move_out_of_bounds() {
        let board = Board::from_str("P").unwrap();

        assert_eq!(
            ChessMove::new((7, 0), (8, 0)).check(&board),
            Err(ChessError::OutOfBounds)
        );
    }

    #[test]
    fn cant_take_your_color() {
        let mut board = Board::from_str("pP/pP").unwrap();
        board.set_ordered(false);

        assert_eq!(
            ChessMove::new((7, 0), (6, 0)).check(&board),
            Err(ChessError::TakeOwn)
        );
        assert_eq!(
            ChessMove::new((6, 1), (7, 1)).check(&board),
            Err(ChessError::TakeOwn)
        );
    }

    #[test]
    fn move_order() {
        let mut board = Board::new();

        assert_eq!(
            ChessMove::new((6, 0), (5, 0)).check(&board),
            Err(ChessError::OnlyMoveYourColor)
        );
        ChessMove::new((1, 0), (2, 0)).exec(&mut board);
        assert_eq!(
            ChessMove::new((1, 1), (2, 1)).check(&board),
            Err(ChessError::OnlyMoveYourColor)
        );
    }

    #[test]
    fn pawn_moves_only_up() {
        let mut board = Board::from_str("//Pp").unwrap();
        board.set_ordered(false);

        ChessMove::new((5, 0), (6, 0)).check(&board).unwrap();
        ChessMove::new((5, 1), (4, 1)).check(&board).unwrap();
        assert_eq!(
            ChessMove::new((5, 0), (7, 0)).check(&board),
            Err(ChessError::MovePattern)
        );
        assert_eq!(
            ChessMove::new((5, 0), (6, 1)).check(&board),
            Err(ChessError::MovePattern)
        );
    }

    #[test]
    fn pawn_takes_only_diagonally() {
        let board = Board::from_str("pp/P").unwrap();

        assert_eq!(
            ChessMove::new((6, 0), (7, 0)).check(&board),
            Err(ChessError::MovePattern)
        );
        ChessMove::new((6, 0), (7, 1))
            .promote(Kind::Queen)
            .check(&board)
            .unwrap();
    }

    #[test]
    fn knight_makes_an_l() {
        let board = Board::from_str("//4N").unwrap();

        assert_eq!(
            ChessMove::new((5, 4), (7, 4)).check(&board),
            Err(ChessError::MovePattern)
        );
        ChessMove::new((5, 4), (7, 3)).check(&board).unwrap();
    }

    #[test]
    fn bishop_moves_in_a_cross() {
        let board = Board::from_str("//4B").unwrap();

        assert_eq!(
            ChessMove::new((5, 4), (7, 4)).check(&board),
            Err(ChessError::MovePattern)
        );
        ChessMove::new((5, 4), (7, 2)).check(&board).unwrap();
    }

    #[test]
    fn rook_moves_in_a_plus() {
        let board = Board::from_str("//4R").unwrap();

        assert_eq!(
            ChessMove::new((5, 4), (7, 2)).check(&board),
            Err(ChessError::MovePattern)
        );
        ChessMove::new((5, 4), (7, 4)).check(&board).unwrap();
    }

    #[test]
    fn queen_moves_anywhere() {
        let board = Board::from_str("//4Q").unwrap();

        assert_eq!(
            ChessMove::new((5, 4), (7, 3)).check(&board),
            Err(ChessError::MovePattern)
        );
        ChessMove::new((5, 4), (7, 4)).check(&board).unwrap();
        ChessMove::new((5, 4), (7, 2)).check(&board).unwrap();
    }

    #[test]
    fn king_moves_anywhere_close() {
        let board = Board::from_str("//4K").unwrap();

        assert_eq!(
            ChessMove::new((5, 4), (7, 2)).check(&board),
            Err(ChessError::MovePattern)
        );
        ChessMove::new((5, 4), (6, 4)).check(&board).unwrap();
        ChessMove::new((5, 4), (6, 3)).check(&board).unwrap();
    }

    #[test]
    fn en_passant() {
        let mut board = Board::from_str("////p//1P").unwrap();

        ChessMove::new((1, 1), (3, 1)).exec(&mut board);
        let m = ChessMove::new((3, 0), (2, 1));
        m.check(&board).unwrap();
        m.exec(&mut board);
        assert!(board[(3, 1)].is_none());

        let mut board = Board::from_str("////pP").unwrap();
        board.turn = Color::Black;
        assert_eq!(
            ChessMove::new((3, 0), (2, 1)).check(&board),
            Err(ChessError::MovePattern)
        );
    }

    #[test]
    fn cant_move_after_victory() {
        let mut board = Board::from_str("kRp").unwrap();

        let m = ChessMove::new((7, 1), (7, 0));
        m.exec(&mut board);

        assert_eq!(
            ChessMove::new((7, 2), (6, 2)).check(&board),
            Err(ChessError::AlreadyWon)
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Pawn,
    Rook,
    Knight,
    Bishop,
    King,
    Queen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

impl Not for Color {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub kind: Kind,
    pub color: Color,
}

impl Display for Piece {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = match self.kind {
            Kind::Pawn => 'p',
            Kind::Rook => 'r',
            Kind::Knight => 'n',
            Kind::Bishop => 'b',
            Kind::Queen => 'q',
            Kind::King => 'k',
        };

        if self.color == Color::White {
            write!(f, "{}", c.to_ascii_uppercase())
        } else {
            write!(f, "{c}")
        }
    }
}

pub struct Board {
    state: [Option<Piece>; 64],
    pub turn: Color,
    ordered: bool,
    pub victory: Option<Color>,
    en_passant: Option<(usize, usize)>,
}

impl Board {
    #[must_use]
    pub fn new() -> Self {
        Self::from_str("rnbqkbnr/pppppppp/////PPPPPPPP/RNBQKBNR").expect("invalid fen")
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            state: [None; 64],
            turn: Color::White,
            ordered: true,
            victory: None,
            en_passant: None,
        }
    }

    pub fn set_ordered(&mut self, v: bool) {
        self.ordered = v;
    }
}

impl Display for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for rank in 0..8 {
            for file in 0..7 {
                match self[(7 - rank, file)] {
                    Some(p) => write!(f, "{p}"),
                    None => write!(f, "."),
                }?;
            }

            writeln!(f)?;
        }

        Ok(())
    }
}

impl Index<(usize, usize)> for Board {
    type Output = Option<Piece>;

    fn index(&self, index: (usize, usize)) -> &Self::Output {
        &self.state[index.0 * 8 + index.1]
    }
}

impl IndexMut<(usize, usize)> for Board {
    fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
        &mut self.state[index.0 * 8 + index.1]
    }
}

impl FromStr for Board {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut state = [None; 64];
        let mut rank: usize = 7;
        let mut file: usize = 0;

        for byte in s.bytes() {
            if byte == b' ' {
                break;
            }

            let piece = match byte {
                b'/' => {
                    file = 0;
                    rank -= 1;
                    continue;
                }
                b'p' | b'P' => Kind::Pawn,
                b'r' | b'R' => Kind::Rook,
                b'n' | b'N' => Kind::Knight,
                b'b' | b'B' => Kind::Bishop,
                b'q' | b'Q' => Kind::Queen,
                b'k' | b'K' => Kind::King,
                _ => {
                    if (b'1'..=b'8').contains(&byte) {
                        file += usize::from(byte - b'0');
                        continue;
                    } else {
                        return Err(());
                    }
                }
            };

            let color = if byte.is_ascii_uppercase() {
                Color::White
            } else {
                Color::Black
            };

            state[rank * 8 + file] = Some(Piece { kind: piece, color });
            file += 1;
        }

        Ok(Board {
            state,
            turn: Color::White,
            ordered: true,
            victory: None,
            en_passant: None,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[must_use]
pub struct ChessMove {
    pub from: (usize, usize),
    pub to: (usize, usize),
    pub promotion: Option<Kind>,
}

impl ChessMove {
    pub const fn new(from: (usize, usize), to: (usize, usize)) -> Self {
        Self {
            from,
            to,
            promotion: None,
        }
    }

    pub const fn promote(mut self, kind: Kind) -> Self {
        self.promotion = Some(kind);
        self
    }
}

pub trait Move: for<'de> Deserialize<'de> {
    type State;
    type Err;

    fn request(state: &Self::State) -> impl Serialize;

    fn check(&self, state: &Self::State) -> Result<(), Self::Err>;

    fn exec(self, state: &mut Self::State);
}

pub trait Player {
    fn send(&mut self, data: &str) -> impl Future<Output = ()>;

    fn query<M: Move>(&self, state: &M::State) -> impl Future<Output = M>;
}

pub trait Table {
    type Player: Player;

    fn players(&self) -> &[Self::Player];

    fn state(&self) -> &mut Board;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChessError {
    Empty,
    MovePattern,
    OutOfBounds,
    TakeOwn,
    Collision,
    OnlyMoveYourColor,
    Promotion,
    AlreadyWon,
}

impl Move for ChessMove {
    type State = Board;
    type Err = ChessError;

    fn request(_state: &Self::State) -> impl Serialize {
        ""
    }

    fn check(&self, state: &Self::State) -> Result<(), ChessError> {
        if state.victory.is_some() {
            return Err(ChessError::AlreadyWon);
        }

        if self.from.0 > 7 || self.from.1 > 7 || self.to.0 > 7 || self.to.1 > 7 {
            return Err(ChessError::OutOfBounds);
        }

        let Some(piece) = state[self.from] else {
            return Err(ChessError::Empty);
        };

        // can only move your
        if state.ordered && piece.color != state.turn {
            return Err(ChessError::OnlyMoveYourColor);
        }

        // cant take own
        if let Some(target) = state[self.to]
            && target.color == piece.color
        {
            return Err(ChessError::TakeOwn);
        }

        // Verify pattern
        // TODO: Make it `SmallVec`
        let mut trace = Vec::new();
        let pattern = match piece.kind {
            Kind::Pawn => {
                let is_attacking = state[self.to].is_some() || state.en_passant == Some(self.to);
                (match piece.color {
                    Color::White => {
                        let ret = self.to.0 == self.from.0 + 1;

                        if !ret && self.from.0 == 1 && self.to.0 == 3 {
                            trace.push((2, self.from.1));
                            true
                        } else {
                            ret
                        }
                    }
                    Color::Black => {
                        let ret = self.to.0 == self.from.0 - 1;

                        if !ret && self.from.0 == 6 && self.to.0 == 4 {
                            trace.push((5, self.from.1));
                            true
                        } else {
                            ret
                        }
                    }
                }) && self.from.1.abs_diff(self.to.1) == if is_attacking { 1 } else { 0 }
                    && !(self.from.1.abs_diff(self.to.1) == 1
                        && self.from.0.abs_diff(self.to.0) == 2)
            }
            Kind::Knight => {
                let y = self.from.0.abs_diff(self.to.0);
                let x = self.from.1.abs_diff(self.to.1);

                x == 1 && y == 2 || x == 2 && y == 1
            }
            Kind::Bishop => {
                let y = self.from.0.abs_diff(self.to.0);
                let x = self.from.1.abs_diff(self.to.1);

                let mut pos = self.from;

                loop {
                    // step
                    match pos.0.cmp(&self.to.0) {
                        Ordering::Less => pos.0 += 1,
                        Ordering::Equal => (),
                        Ordering::Greater => pos.0 -= 1,
                    }
                    match pos.1.cmp(&self.to.1) {
                        Ordering::Less => pos.1 += 1,
                        Ordering::Equal => (),
                        Ordering::Greater => pos.1 -= 1,
                    }

                    if pos == self.to {
                        break;
                    } else {
                        trace.push(pos);
                    }
                }

                x == y && x != 0
            }
            Kind::Rook => {
                let y = self.from.0.abs_diff(self.to.0);
                let x = self.from.1.abs_diff(self.to.1);

                let mut pos = self.from;

                loop {
                    // step
                    match pos.0.cmp(&self.to.0) {
                        Ordering::Less => pos.0 += 1,
                        Ordering::Equal => (),
                        Ordering::Greater => pos.0 -= 1,
                    }
                    match pos.1.cmp(&self.to.1) {
                        Ordering::Less => pos.1 += 1,
                        Ordering::Equal => (),
                        Ordering::Greater => pos.1 -= 1,
                    }

                    if pos == self.to {
                        break;
                    } else {
                        trace.push(pos);
                    }
                }

                x == 0 && y != 0 || y == 0 && x != 0
            }
            Kind::Queen => {
                let y = self.from.0.abs_diff(self.to.0);
                let x = self.from.1.abs_diff(self.to.1);

                let mut pos = self.from;

                loop {
                    // step
                    match pos.0.cmp(&self.to.0) {
                        Ordering::Less => pos.0 += 1,
                        Ordering::Equal => (),
                        Ordering::Greater => pos.0 -= 1,
                    }
                    match pos.1.cmp(&self.to.1) {
                        Ordering::Less => pos.1 += 1,
                        Ordering::Equal => (),
                        Ordering::Greater => pos.1 -= 1,
                    }

                    if pos == self.to {
                        break;
                    } else {
                        trace.push(pos);
                    }
                }

                x == 0 && y != 0 || y == 0 && x != 0 || (x == y && x != 0)
            }
            Kind::King => {
                self.from.0.abs_diff(self.to.0) <= 1 && self.from.1.abs_diff(self.to.1) <= 1
            }
        };

        if !pattern {
            return Err(ChessError::MovePattern);
        }

        // Verify promotion
        if piece.kind == Kind::Pawn && (self.to.0 == 0 || self.to.0 == 7) {
            let Some(promotion) = self.promotion else {
                return Err(ChessError::Promotion);
            };

            if matches!(promotion, Kind::Pawn | Kind::King) {
                return Err(ChessError::Promotion);
            }
        } else if self.promotion.is_some() {
            return Err(ChessError::Promotion);
        }

        // Verify trace
        if trace.into_iter().any(|v| state[v].is_some()) {
            return Err(ChessError::Collision);
        }

        Ok(())
    }

    fn exec(self, state: &mut Self::State) {
        let target = state[self.to];
        assert!(state[self.from].is_some());
        state[self.to] = state[self.from].take();
        state.turn = !state.turn;

        if let Some(target) = target
            && target.kind == Kind::King
        {
            state.victory = Some(!target.color);
        }

        if let Some(promotion) = self.promotion {
            state[self.to].as_mut().unwrap().kind = promotion;
        }

        let piece = state[self.to].unwrap();

        // detect en passant
        if piece.kind == Kind::Pawn && Some(self.to) == state.en_passant {
            match piece.color {
                Color::White => state[(self.to.0 - 1, self.to.1)] = None,
                Color::Black => state[(self.to.0 + 1, self.to.1)] = None,
            }
        }

        state.en_passant = None;
        if piece.kind == Kind::Pawn && self.to.0.abs_diff(self.from.0) == 2 {
            state.en_passant = Some(((self.to.0 + self.from.0) / 2, self.to.1));
        }
    }
}

pub async fn logic(table: impl Table) {
    let [w, b] = table.players() else {
        panic!("illegal player count")
    };
    let mut state = table.state();

    loop {
        w.query::<ChessMove>(&state).await.exec(&mut state);
        b.query::<ChessMove>(&state).await.exec(&mut state);
    }
}
