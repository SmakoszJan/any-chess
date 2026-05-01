use std::{
    collections::HashSet,
    fmt::Display,
    ops::{Add, Index, IndexMut, Mul, Not},
    str::FromStr,
    sync::OnceLock,
};

use generational_arena::{Arena, Index as ArenaIndex};
use serde::{Deserialize, Serialize};

pub const VERSION: &str = "0.8";

pub mod net;

#[cfg(test)]
pub mod tests {
    use std::str::FromStr;

    use crate::{Board, ChessMove, Color, Kind, Move};

    #[test]
    fn empty_squares_do_not_move() {
        let board = Board::empty();

        assert!(ChessMove::new((0, 0), (0, 0)).check(&board).is_err());
    }

    #[test]
    fn pieces_cant_move_out_of_bounds() {
        let board = Board::from_str("P").unwrap();

        assert!(ChessMove::new((7, 0), (8, 0)).check(&board).is_err());
    }

    #[test]
    fn cant_take_your_color() {
        let mut board = Board::from_str("pP/pP").unwrap();
        board.rules.move_order = false;

        assert!(ChessMove::new((7, 0), (6, 0)).check(&board).is_err());
        assert!(ChessMove::new((6, 1), (7, 1)).check(&board).is_err());
    }

    #[test]
    fn move_order() {
        let mut board = Board::new();

        assert!(ChessMove::new((6, 0), (5, 0)).check(&board).is_err());
        ChessMove::new((1, 0), (2, 0)).exec(&mut board);
        assert!(ChessMove::new((1, 1), (2, 1)).check(&board).is_err());
    }

    #[test]
    fn pawn_moves_only_up() {
        let mut board = Board::from_str("///Pp").unwrap();
        board.rules.move_order = false;
        board.rules.move_after_win = true;

        ChessMove::new((4, 0), (5, 0)).check(&board).unwrap();
        ChessMove::new((4, 0), (6, 0)).check(&board).unwrap();
        ChessMove::new((4, 1), (3, 1)).check(&board).unwrap();
        ChessMove::new((4, 0), (5, 0)).exec(&mut board);
        assert!(ChessMove::new((5, 0), (7, 0)).check(&board).is_err());
        assert!(ChessMove::new((5, 0), (6, 1)).check(&board).is_err());
    }

    #[test]
    fn pawn_takes_only_diagonally() {
        let board = Board::from_str("pp/P").unwrap();

        assert!(ChessMove::new((6, 0), (7, 0)).check(&board).is_err());
        ChessMove::new((6, 0), (7, 1))
            .promote(Kind::Queen)
            .check(&board)
            .unwrap();
    }

    #[test]
    fn knight_makes_an_l() {
        let board = Board::from_str("//4N").unwrap();

        assert!(ChessMove::new((5, 4), (7, 4)).check(&board).is_err());
        ChessMove::new((5, 4), (7, 3)).check(&board).unwrap();
    }

    #[test]
    fn bishop_moves_in_a_cross() {
        let board = Board::from_str("//4B").unwrap();

        assert!(ChessMove::new((5, 4), (7, 4)).check(&board).is_err());
        ChessMove::new((5, 4), (7, 2)).check(&board).unwrap();
    }

    #[test]
    fn rook_moves_in_a_plus() {
        let board = Board::from_str("//4R").unwrap();

        assert!(ChessMove::new((5, 4), (7, 2)).check(&board).is_err());
        ChessMove::new((5, 4), (7, 4)).check(&board).unwrap();
    }

    #[test]
    fn queen_moves_anywhere() {
        let board = Board::from_str("//4Q").unwrap();

        assert!(ChessMove::new((5, 4), (7, 3)).check(&board).is_err());
        ChessMove::new((5, 4), (7, 4)).check(&board).unwrap();
        ChessMove::new((5, 4), (7, 2)).check(&board).unwrap();
    }

    #[test]
    fn king_moves_anywhere_close() {
        let board = Board::from_str("//4K").unwrap();

        assert!(ChessMove::new((5, 4), (7, 2)).check(&board).is_err());
        ChessMove::new((5, 4), (6, 4)).check(&board).unwrap();
        ChessMove::new((5, 4), (6, 3)).check(&board).unwrap();
    }

    #[test]
    fn en_passant_forced() {
        let mut board = Board::from_str("R////p//1P").unwrap();
        board.rules.move_after_win = true;

        assert!(ChessMove::new((7, 0), (7, 2)).check(&board).is_err());
        ChessMove::new((1, 1), (3, 1)).exec(&mut board);
        assert!(ChessMove::new((3, 0), (2, 0)).check(&board).is_err());
    }

    #[test]
    fn en_passant() {
        let mut board = Board::from_str("R////p//1P").unwrap();
        board.rules.move_after_win = true;

        ChessMove::new((1, 1), (3, 1)).exec(&mut board);
        let m = ChessMove::new((3, 0), (2, 1));
        m.check(&board).unwrap();
        m.exec(&mut board);
        assert!(board[(3, 1).into()].is_none());

        let mut board = Board::from_str("////pP").unwrap();
        board.turn = Color::Black;
        assert!(ChessMove::new((3, 0), (2, 1)).check(&board).is_err());
    }

    #[test]
    fn cant_move_after_victory() {
        let mut board = Board::from_str("kRp").unwrap();

        let m = ChessMove::new((7, 1), (7, 0));
        m.exec(&mut board);

        assert!(ChessMove::new((7, 2), (6, 2)).check(&board).is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Kind {
    Pawn,
    Rook,
    Knight,
    Bishop,
    King,
    Queen,
    Knook,
}

impl Kind {
    /// Returns `true` if the kind is [`Pawn`].
    ///
    /// [`Pawn`]: Kind::Pawn
    #[must_use]
    pub fn is_pawn(&self) -> bool {
        matches!(self, Self::Pawn)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    White,
    Black,
}

impl Color {
    /// Returns `true` if the color is [`White`].
    ///
    /// [`White`]: Color::White
    #[must_use]
    pub fn is_white(&self) -> bool {
        matches!(self, Self::White)
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Direction {
    Up,
    Left,
    Right,
    Down,
}

impl Direction {
    #[must_use]
    pub fn left(self) -> Direction {
        match self {
            Self::Up => Self::Left,
            Self::Left => Self::Down,
            Self::Right => Self::Up,
            Self::Down => Self::Right,
        }
    }

    #[must_use]
    pub fn right(self) -> Direction {
        match self {
            Self::Up => Self::Right,
            Self::Left => Self::Up,
            Self::Right => Self::Down,
            Self::Down => Self::Left,
        }
    }
}

impl Not for Direction {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Self::Up => Self::Down,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Down => Self::Up,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LadderState {
    None,
    Building(ArenaIndex),
    Spent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub kind: Kind,
    pub color: Color,
    pub direction: Direction,
    pub moved: bool,
    pub ladder: LadderState,
    pub ladder_countdown: u8,
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
            Kind::Knook => 'o',
        };

        if self.color == Color::White {
            write!(f, "{}", c.to_ascii_uppercase())
        } else {
            write!(f, "{c}")
        }
    }
}

#[derive(Clone)]
struct Rules {
    move_after_win: bool,
    move_order: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            move_after_win: false,
            move_order: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Pos {
    pub rank: i32,
    pub file: i32,
}

impl From<(i32, i32)> for Pos {
    fn from(value: (i32, i32)) -> Self {
        Self {
            rank: value.0,
            file: value.1,
        }
    }
}

impl Add<Pos> for Pos {
    type Output = Pos;

    fn add(self, rhs: Pos) -> Self::Output {
        Pos {
            rank: self.rank + rhs.rank,
            file: self.file + rhs.file,
        }
    }
}

impl Mul<Direction> for Pos {
    type Output = Pos;

    fn mul(self, rhs: Direction) -> Self::Output {
        match rhs {
            Direction::Up => self,
            Direction::Left => Pos {
                rank: self.file,
                file: -self.rank,
            },
            Direction::Down => Pos {
                rank: -self.rank,
                file: -self.file,
            },
            Direction::Right => Pos {
                rank: -self.file,
                file: self.rank,
            },
        }
    }
}

#[derive(PartialEq, Eq)]
enum Tile {
    Some(Piece),
    EnPassant,
    Empty,
    Void,
}

impl Tile {
    #[must_use]
    fn is_empty_or(self, f: impl FnOnce(Piece) -> bool) -> bool {
        match self {
            Self::Some(v) => f(v),
            Self::Empty => true,
            Self::Void => false,
            Self::EnPassant => true,
        }
    }

    #[must_use]
    fn is_empty(self) -> bool {
        matches!(self, Self::Empty)
    }

    #[must_use]
    fn is_pawn_takeable(self, color: Color) -> bool {
        match self {
            Self::Some(v) => v.color != color,
            Self::EnPassant => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct MoveSet {
    moves: HashSet<ChessMove>,
    forced: HashSet<ChessMove>,
    /// Is it possible?
    en_passant: bool,
}

#[derive(Clone, Copy)]
pub struct Ladder {
    pub start: Pos,
    pub end: Pos,
    pub built: bool,
}

#[derive(Clone)]
pub struct Board {
    rules: Rules,
    state: [Option<Piece>; 64],
    pub turn: Color,
    pub victory: Option<Color>,
    en_passant: Option<Pos>,
    moves: OnceLock<MoveSet>,
    pub ladders: Arena<Ladder>,
}

impl Board {
    #[must_use]
    pub fn new() -> Self {
        Self::from_str("rnbqkbnr/pppppppp/////PPPPPPPP/RNBQKBNR").expect("invalid fen")
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            rules: Rules::default(),
            state: [None; 64],
            turn: Color::White,
            victory: None,
            en_passant: None,
            moves: OnceLock::new(),
            ladders: Arena::new(),
        }
    }

    fn get(&self, pos: Pos) -> Tile {
        if Some(pos) == self.en_passant {
            Tile::EnPassant
        } else if 0 <= pos.rank && pos.rank < 8 && 0 <= pos.file && pos.file < 8 {
            self.state[(pos.rank * 8 + pos.file) as usize].map_or(Tile::Empty, Tile::Some)
        } else {
            Tile::Void
        }
    }

    fn gen_patterns(
        &self,
        piece: Piece,
        pos: Pos,
        patterns: impl IntoIterator<Item = (i32, i32)>,
        moves: &mut HashSet<ChessMove>,
    ) {
        for (dx, dy) in patterns {
            let target = Pos {
                rank: pos.rank + dy,
                file: pos.file + dx,
            };

            if self.get(target).is_empty_or(|v| v.color != piece.color) {
                moves.insert(ChessMove::new(pos, target));
            }
        }
    }

    fn gen_directions(
        &self,
        piece: Piece,
        pos: Pos,
        dirs: impl IntoIterator<Item = (i32, i32)>,
        moves: &mut HashSet<ChessMove>,
    ) {
        for (dx, dy) in dirs {
            let mut target = Pos {
                rank: pos.rank + dy,
                file: pos.file + dx,
            };

            loop {
                if self.get(target).is_empty_or(|v| v.color != piece.color) {
                    moves.insert(ChessMove::new(pos, target));
                } else {
                    break;
                }

                target.rank += dy;
                target.file += dx;
            }
        }
    }

    fn get_moves(&self, deep: bool) -> &MoveSet {
        self.moves.get_or_init(|| {
            if !self.rules.move_after_win && self.victory.is_some() {
                return MoveSet::default();
            }

            let mut moves = HashSet::new();
            let mut forced = HashSet::new();
            let mut en_passant = false;

            for (i, piece) in self
                .state
                .iter()
                .copied()
                .enumerate()
                .flat_map(|v| Some((v.0, v.1?)))
            {
                let pos = Pos {
                    rank: (i / 8) as i32,
                    file: (i % 8) as i32,
                };

                if self.rules.move_order && piece.color != self.turn {
                    continue;
                }

                for (_, ladder) in &self.ladders {
                    if !ladder.built {
                        continue;
                    }

                    let target = if ladder.start == pos {
                        ladder.end
                    } else if ladder.end == pos {
                        ladder.start
                    } else {
                        continue;
                    };

                    if self.get(target) == Tile::Empty {
                        moves.insert(ChessMove::new(pos, target));
                    }
                }

                match piece.kind {
                    Kind::King => {
                        self.gen_patterns(
                            piece,
                            pos,
                            [
                                (-1, -1),
                                (-1, 0),
                                (-1, 1),
                                (0, -1),
                                (0, 1),
                                (1, -1),
                                (1, 0),
                                (1, 1),
                            ],
                            &mut moves,
                        );
                    }
                    Kind::Bishop => {
                        self.gen_directions(
                            piece,
                            pos,
                            [(1, 1), (1, -1), (-1, -1), (-1, 1)],
                            &mut moves,
                        );
                    }
                    Kind::Rook => {
                        self.gen_directions(
                            piece,
                            pos,
                            [(1, 0), (-1, 0), (0, -1), (0, 1)],
                            &mut moves,
                        );
                    }
                    Kind::Queen => {
                        self.gen_directions(
                            piece,
                            pos,
                            [
                                (-1, -1),
                                (-1, 0),
                                (-1, 1),
                                (0, -1),
                                (0, 1),
                                (1, -1),
                                (1, 0),
                                (1, 1),
                            ],
                            &mut moves,
                        );
                    }
                    Kind::Knight => {
                        let patterns = [
                            (-1, -2),
                            (-1, 2),
                            (1, -2),
                            (1, 2),
                            (2, -1),
                            (2, 1),
                            (-2, -1),
                            (-2, 1),
                        ];
                        for (dx, dy) in patterns {
                            let target = Pos {
                                rank: pos.rank + dy,
                                file: pos.file + dx,
                            };

                            if self
                                .get(target)
                                .is_empty_or(|v| v.color != piece.color || v.kind == Kind::Rook)
                            {
                                moves.insert(ChessMove::new(pos, target));
                            }
                        }

                        if piece.ladder == LadderState::None {
                            moves.insert(ChessMove::ladder(pos));
                        }
                    }
                    Kind::Knook => {
                        self.gen_directions(
                            piece,
                            pos,
                            [(1, 0), (-1, 0), (0, -1), (0, 1)],
                            &mut moves,
                        );
                        self.gen_patterns(
                            piece,
                            pos,
                            [
                                (-1, -2),
                                (-1, 2),
                                (1, -2),
                                (1, 2),
                                (2, -1),
                                (2, 1),
                                (-2, -1),
                                (-2, 1),
                            ],
                            &mut moves,
                        );
                    }
                    Kind::Pawn => {
                        let mut add = |target: Pos| {
                            let last = match piece.color {
                                Color::White => 7,
                                Color::Black => 0,
                            };

                            let mut candidates = HashSet::new();

                            if target.rank == last {
                                for kind in [
                                    Kind::Rook,
                                    Kind::Knight,
                                    Kind::Bishop,
                                    Kind::Queen,
                                    Kind::King,
                                ] {
                                    candidates.insert(ChessMove::new(pos, target).promote(kind));
                                }
                            } else {
                                candidates.insert(ChessMove {
                                    from: pos,
                                    to: target,
                                    promotion: None,
                                    direction: None,
                                    use_ladder: false,
                                });
                            }

                            for mv in candidates {
                                moves.insert(mv);

                                // En passant is forced
                                if self.get(mv.to) == Tile::EnPassant {
                                    forced.insert(mv);
                                    en_passant = true;
                                }

                                // Offerring en passant is forced
                                let mut b2 = self.clone();
                                mv.exec(&mut b2);
                                if deep && b2.get_moves(false).en_passant {
                                    forced.insert(mv);
                                }
                            }
                        };

                        // Forward 1
                        let target = pos + Pos { rank: 1, file: 0 } * piece.direction;
                        let can1 = if self.get(target).is_empty() {
                            add(target);
                            true
                        } else {
                            false
                        };

                        // Forward 2
                        if can1 && !piece.moved {
                            let target = pos + Pos { rank: 2, file: 0 } * piece.direction;
                            if self.get(target).is_empty() {
                                add(target);
                            }
                        }

                        // Capture left
                        let target = pos + Pos { rank: 1, file: -1 } * piece.direction;
                        if self.get(target).is_pawn_takeable(piece.color) {
                            add(target);
                        }

                        // Capture right
                        let target = pos + Pos { rank: 1, file: 1 } * piece.direction;
                        if self.get(target).is_pawn_takeable(piece.color) {
                            add(target);
                        }

                        // We can also just rotate
                        moves.insert(ChessMove::rotate(pos, piece.direction.left()));
                        moves.insert(ChessMove::rotate(pos, piece.direction.right()));
                    }
                }
            }

            MoveSet {
                moves,
                forced,
                en_passant,
            }
        })
    }
}

impl Display for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for rank in 0..8 {
            for file in 0..8 {
                match self[(7 - rank, file).into()] {
                    Some(p) => write!(f, "{p}"),
                    None => write!(f, "."),
                }?;
            }

            writeln!(f)?;
        }

        Ok(())
    }
}

impl Index<Pos> for Board {
    type Output = Option<Piece>;

    fn index(&self, index: Pos) -> &Self::Output {
        &self.state[(index.rank * 8 + index.file) as usize]
    }
}

impl IndexMut<Pos> for Board {
    fn index_mut(&mut self, index: Pos) -> &mut Self::Output {
        &mut self.state[(index.rank * 8 + index.file) as usize]
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

            state[rank * 8 + file] = Some(Piece {
                kind: piece,
                color,
                direction: match color {
                    Color::White => Direction::Up,
                    Color::Black => Direction::Down,
                },
                moved: false,
                ladder: LadderState::None,
                ladder_countdown: 0,
            });
            file += 1;
        }

        Ok(Board {
            state,
            rules: Rules::default(),
            turn: Color::White,
            victory: None,
            en_passant: None,
            moves: OnceLock::default(),
            ladders: Arena::new(),
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub struct ChessMove {
    pub from: Pos,
    pub to: Pos,
    pub direction: Option<Direction>,
    pub promotion: Option<Kind>,
    pub use_ladder: bool,
}

impl ChessMove {
    pub fn new(from: impl Into<Pos>, to: impl Into<Pos>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            promotion: None,
            direction: None,
            use_ladder: false,
        }
    }

    pub const fn promote(mut self, kind: Kind) -> Self {
        self.promotion = Some(kind);
        self
    }

    pub fn ladder(at: impl Into<Pos>) -> Self {
        let at = at.into();
        Self {
            from: at,
            to: at,
            direction: None,
            promotion: None,
            use_ladder: true,
        }
    }

    pub fn rotate(at: impl Into<Pos>, dir: Direction) -> Self {
        let at = at.into();
        Self {
            from: at,
            to: at,
            direction: Some(dir),
            promotion: None,
            use_ladder: false,
        }
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
    Invalid,
}

impl Move for ChessMove {
    type State = Board;
    type Err = ChessError;

    fn request(_state: &Self::State) -> impl Serialize {
        ""
    }

    fn check(&self, state: &Self::State) -> Result<(), ChessError> {
        let moves = state.get_moves(true);
        if moves.moves.contains(self) && (moves.forced.is_empty() || moves.forced.contains(self)) {
            Ok(())
        } else {
            Err(ChessError::Invalid)
        }
    }

    fn exec(self, state: &mut Self::State) {
        assert!(state[self.from].is_some());

        let was_friendly_rook = if let Some(piece) = state[self.to] {
            // Destroy ladder built by taken piece
            if let LadderState::Building(ladder) = piece.ladder {
                state.ladders.remove(ladder);
            }

            piece.kind == Kind::Rook && piece.color == state.turn
        } else {
            false
        };

        state[self.to] = state[self.from].take();
        state.moves.take();

        if self.from != self.to {
            state[self.to].as_mut().unwrap().moved = true;
        }

        if let Some(promotion) = self.promotion {
            state[self.to].as_mut().unwrap().kind = promotion;
        }

        if let Some(direction) = self.direction {
            state[self.to].as_mut().unwrap().direction = direction;
        }

        // We turn the knight into a knook before ladders
        if was_friendly_rook
            && let Some(piece) = state[self.to]
            && piece.kind == Kind::Knight
        {
            // Drop ladder
            if let LadderState::Building(ladder) = piece.ladder {
                state.ladders.remove(ladder);
            }

            state[self.to] = Some(Piece {
                kind: Kind::Knook,
                color: piece.color,
                direction: piece.direction,
                moved: true,
                ladder: LadderState::None,
                ladder_countdown: 0,
            });
        }

        // Drop ladder counters
        for (i, piece) in state
            .state
            .iter_mut()
            .enumerate()
            .map(|(i, p)| Some((i, p.as_mut()?)))
            .flatten()
        {
            // We already changed color
            if piece.color != state.turn {
                continue;
            }

            if let LadderState::Building(ladder) = piece.ladder {
                let l = state.ladders.get_mut(ladder).unwrap();
                l.end = Pos {
                    rank: (i as i32) / 8,
                    file: (i as i32) % 8,
                };

                if piece.ladder_countdown == 1 {
                    if l.start == l.end {
                        state.ladders.remove(ladder);
                    } else {
                        l.built = true;
                    }
                    piece.ladder = LadderState::Spent;
                }
            }

            if piece.ladder_countdown > 0 {
                piece.ladder_countdown -= 1;
            }
        }

        // Use ladder
        if self.use_ladder {
            let ladder = state.ladders.insert(Ladder {
                start: self.to,
                end: self.to,
                built: false,
            });
            let me = state[self.to].as_mut().unwrap();
            me.ladder_countdown = 3;
            me.ladder = LadderState::Building(ladder);
        }

        let piece = state[self.to].unwrap();

        // detect en passant
        if piece.kind == Kind::Pawn && Some(self.to) == state.en_passant {
            match piece.color {
                Color::White => state[(self.to.rank - 1, self.to.file).into()] = None,
                Color::Black => state[(self.to.rank + 1, self.to.file).into()] = None,
            }
        }

        state.turn = !state.turn;
        state.en_passant = None;
        if piece.kind == Kind::Pawn
            && (self.to.rank.abs_diff(self.from.rank) == 2
                || self.to.file.abs_diff(self.from.file) == 2)
        {
            state.en_passant = Some(Pos {
                rank: (self.to.rank + self.from.rank) / 2,
                file: (self.to.file + self.from.file) / 2,
            });
        }

        if state.victory.is_none() {
            // Check king count
            let mut white_count = 0;
            let mut black_count = 0;
            for piece in state.state.iter().copied().flatten() {
                if piece.kind == Kind::King {
                    match piece.color {
                        Color::White => white_count += 1,
                        Color::Black => black_count += 1,
                    }
                }
            }

            if white_count == 0 {
                state.victory = Some(Color::Black);
            } else if black_count == 0 {
                state.victory = Some(Color::White);
            }
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
