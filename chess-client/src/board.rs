use aeronet_io::{
    Session,
    connection::{Disconnect, Disconnected},
};
use aeronet_websocket::client::{ClientConfig, WebSocketClient, WebSocketClientPlugin};
use bevy::{color::palettes::css, ecs::query::QueryData, prelude::*};
use chess_core::{
    Board, ChessMove, Color as ChessColor, Kind, Move, Piece,
    net::{ChessEvent, ChessMessage, ClientMessage},
};

use std::ops::{Deref, DerefMut, Index};

pub use ui::GoBack;
use ui::{HideUi, Promote, ShowUi};

mod ui;

// #[cfg(debug_assertions)]
// const WS_URL: &str = "ws://0.0.0.0:3000";
// #[cfg(not(debug_assertions))]
const WS_URL: &str = "wss://any-chess-smakoszjan2734-perdtvgt.leapcell.dev";

#[derive(Message)]
struct SyncBoard;

#[derive(Component, Clone, Copy)]
struct BoardPosition {
    rank: usize,
    file: usize,
}

#[derive(Resource)]
struct ChessBoard {
    board: Board,
}

impl Deref for ChessBoard {
    type Target = Board;

    fn deref(&self) -> &Self::Target {
        &self.board
    }
}

impl DerefMut for ChessBoard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.board
    }
}

impl Index<BoardPosition> for ChessBoard {
    type Output = Option<Piece>;

    fn index(&self, index: BoardPosition) -> &Self::Output {
        &self.board[(index.rank, index.file)]
    }
}

#[derive(Component)]
struct PieceMarker;

#[derive(Component)]
struct MoveMarker;

fn sync_board(
    mut pieces: Query<(&BoardPosition, &mut Sprite, &mut Visibility), With<PieceMarker>>,
    mut reader: MessageReader<SyncBoard>,
    board: Res<ChessBoard>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    if !reader.is_empty() {
        reader.clear();

        pieces
            .par_iter_mut()
            .for_each(|(&sq, mut sprite, mut vis)| {
                if let Some(piece) = board[sq] {
                    *vis = Visibility::Inherited;
                    let color_letter = match piece.color {
                        chess_core::Color::White => 'w',
                        chess_core::Color::Black => 'b',
                    };
                    sprite.image = assets.load(format!("{color_letter}_{:?}.png", piece.kind));
                } else {
                    *vis = Visibility::Hidden;
                }
            });

        commands.trigger(StageMove(None));
    }
}

#[derive(Default)]
enum MoveState {
    #[default]
    None,
    Start(Entity),
    Promote {
        from: Entity,
        to: Entity,
    },
}

#[derive(Resource)]
struct ClientState {
    room: i32,
    color: ChessColor,
    move_allowed: bool,
    move_state: MoveState,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            room: 0,
            color: ChessColor::White,
            move_allowed: false,
            move_state: MoveState::None,
        }
    }
}

#[derive(Event)]
struct MakeMove(ChessMove, bool);

#[derive(Event)]
struct SelectSquare(Option<Entity>);

#[derive(Resource)]
struct SelectedSquare(Option<Entity>);

#[derive(QueryData)]
#[query_data(mutable)]
struct Square {
    color: &'static mut MeshMaterial2d<ColorMaterial>,
    children: &'static Children,
    pos: &'static BoardPosition,
    t: &'static GlobalTransform,
}

fn on_square_selected(
    trigger: On<SelectSquare>,
    mut squares: Query<Square, Without<PieceMarker>>,
    mut current: ResMut<SelectedSquare>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Deselect
    if let Some(current) = current.0
        && let Ok(mut sq) = squares.get_mut(current)
    {
        sq.color.0 = materials.add(if (sq.pos.rank + sq.pos.file) % 2 == 0 {
            Color::from(css::BROWN)
        } else {
            Color::WHITE
        });
    }

    current.0 = trigger.0;

    if let Some(current) = current.0
        && let Ok(mut sq) = squares.get_mut(current)
    {
        sq.color.0 = materials.add(if (sq.pos.rank + sq.pos.file) % 2 == 0 {
            Color::from(css::ROSY_BROWN)
        } else {
            Color::from(css::GRAY)
        });
    }
}

#[derive(Event)]
struct StageMove(Option<Entity>);

fn on_stage_move(
    ev: On<StageMove>,
    squares: Query<(&Children, &BoardPosition, &GlobalTransform)>,
    mut pieces: Query<&mut Transform>,
    mut markers: Query<(&mut Sprite, &mut Visibility, &BoardPosition), With<MoveMarker>>,
    board: Res<ChessBoard>,
    mut commands: Commands,
    mut state: ResMut<ClientState>,
    assets: Res<AssetServer>,
) {
    // No matter what, we deselect the current move
    if let MoveState::Start(current) = state.move_state
        && let Ok(sq) = squares.get(current)
        && let Ok(mut t) = pieces.get_mut(sq.0[0])
    {
        t.translation.y = 0.0;
    }
    commands.trigger(HideUi);

    // Hide all markers
    for mut marker in &mut markers {
        *marker.1 = Visibility::Hidden;
    }

    // Now if there's a move already staged, we try to finish it
    if let MoveState::Start(current) = state.move_state
        && let Ok(from) = squares.get(current)
        && let Some(picked) = ev.0
        && let Ok(to) = squares.get(picked)
    {
        let mut m = ChessMove {
            from: (from.1.rank, from.1.file),
            to: (to.1.rank, to.1.file),
            promotion: None,
        };

        if let Some(piece) = board[*from.1]
            && piece.kind == Kind::Pawn
            && (to.1.rank == 0 || to.1.rank == 7)
        {
            m.promotion = Some(Kind::Queen);
        }

        if m.check(&board).is_ok() {
            if m.promotion.is_some() {
                state.move_state = MoveState::Promote {
                    from: current,
                    to: picked,
                };
                commands.trigger(ShowUi {
                    at: to.2.translation() - Vec3::new(32.0, -128.0, 0.0),
                });
            } else {
                commands.trigger(MakeMove(m, true));
            }
            return;
        }
    }

    // Otherwise we stage a new one
    if state.move_allowed {
        state.move_state = ev.0.map_or(MoveState::None, MoveState::Start);
    }

    // Lift the piece
    if let MoveState::Start(current) = state.move_state
        && let Ok(sq) = squares.get(current)
    {
        if let Ok(mut t) = pieces.get_mut(sq.0[0]) {
            t.translation.y = 16.0;
        }

        // Put markers
        let current = *sq.1;

        for (mut sprite, mut vis, pos) in markers {
            let mut m = ChessMove {
                from: (current.rank, current.file),
                to: (pos.rank, pos.file),
                promotion: None,
            };
            if let Some(piece) = board[current]
                && piece.kind == Kind::Pawn
                && (pos.rank == 0 || pos.rank == 7)
            {
                m.promotion = Some(Kind::Queen);
            }

            if m.check(&board).is_ok() {
                *vis = Visibility::Inherited;
                if board[*pos].is_some() {
                    sprite.image = assets.load("take.png");
                } else {
                    sprite.image = assets.load("move.png");
                }
            }
        }
    }
}

fn on_promote(
    ev: On<Promote>,
    pieces: Query<&BoardPosition>,
    state: Res<ClientState>,
    mut commands: Commands,
) {
    let MoveState::Promote { from, to } = state.move_state else {
        return;
    };
    let from = pieces.get(from).unwrap();
    let to = pieces.get(to).unwrap();

    commands.trigger(MakeMove(
        ChessMove {
            from: (from.rank, from.file),
            to: (to.rank, to.file),
            promotion: Some(ev.0),
        },
        true,
    ));
    commands.trigger(StageMove(None));
}

fn on_make_move(
    m: On<MakeMove>,
    mut board: ResMut<ChessBoard>,
    mut writer: MessageWriter<SyncBoard>,
    mut session: Single<&mut Session>,
    mut state: ResMut<ClientState>,
) {
    m.0.exec(&mut board);
    writer.write(SyncBoard);
    if m.1 {
        session.send.push(
            serde_json::to_vec(&ClientMessage::Move(m.0))
                .unwrap()
                .into(),
        );
    }

    state.move_state = MoveState::None;
    state.move_allowed = board.turn == state.color;
}

fn on_square_clicked(click: On<Pointer<Click>>, mut commands: Commands) {
    commands.trigger(SelectSquare(Some(click.entity)));
    commands.trigger(StageMove(Some(click.entity)));
}

#[derive(Event)]
pub struct Play {
    pub token: String,
    pub is_white: bool,
    pub room: i32,
}

fn on_play(
    play: On<Play>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut writer: MessageWriter<SyncBoard>,
    mut board: ResMut<ChessBoard>,
    mut client: ResMut<ClientState>,
) {
    client.color = if play.is_white {
        ChessColor::White
    } else {
        ChessColor::Black
    };
    client.room = play.room;
    client.move_allowed = false;
    client.move_state = MoveState::None;
    board.board = Board::new();
    let square = meshes.add(Rectangle::new(64.0, 64.0));
    let white = materials.add(Color::WHITE);
    let black = materials.add(Color::from(css::BROWN));

    let mut x = -224.0;
    let mut y = if play.is_white { -224.0 } else { 224.0 };
    for rank in 0..8 {
        for file in 0..8 {
            commands
                .spawn((
                    Mesh2d(square.clone()),
                    MeshMaterial2d(if (rank + file) % 2 == 0 {
                        black.clone()
                    } else {
                        white.clone()
                    }),
                    Transform::from_xyz(x, y, 0.0),
                    Pickable::default(),
                    BoardPosition { rank, file },
                ))
                .observe(on_square_clicked)
                .with_children(|children| {
                    children.spawn((
                        Sprite::default(),
                        Transform::from_scale(Vec3::splat(0.5)),
                        BoardPosition { rank, file },
                        PieceMarker,
                    ));
                    children.spawn((
                        Sprite::default(),
                        BoardPosition { rank, file },
                        MoveMarker,
                        Visibility::Hidden,
                    ));
                });
            x += 64.0;
        }

        if play.is_white {
            y += 64.0;
        } else {
            y -= 64.0;
        }
        x = -224.0;
    }

    writer.write(SyncBoard);

    // Connect to client
    commands.spawn_empty().queue(WebSocketClient::connect(
        ClientConfig::default(),
        format!("{WS_URL}/connect?token={}", play.token),
    ));
}

fn despawn_board(tiles: Query<Entity, With<Pickable>>, mut commands: Commands) {
    for tile in tiles {
        commands.get_entity(tile).unwrap().despawn();
    }
}

#[derive(Message)]
pub struct GameEnded(pub i32);

fn process_event(
    ev: &ChessEvent,
    state: &mut ClientState,
    board: &Board,
    commands: &mut Commands,
    ended: &mut MessageWriter<GameEnded>,
) {
    match ev {
        ChessEvent::Move(mv) => {
            // This is kind of a hack
            if board[mv.from].is_none() {
                return;
            }

            commands.trigger(MakeMove(*mv, false));
            state.move_allowed = board.turn == state.color;
        }
        ChessEvent::GameEnded => {
            ended.write(GameEnded(state.room));
        }
    }
}

fn process_msgs(
    mut session: Single<&mut Session>,
    mut commands: Commands,
    mut state: ResMut<ClientState>,
    board: Res<ChessBoard>,
    mut ended: MessageWriter<GameEnded>,
) {
    for msg in session.recv.drain(..) {
        let msg: ChessMessage = serde_json::from_slice(msg.payload.as_ref()).unwrap();
        tracing::info!("Received {msg:?}");

        match msg {
            ChessMessage::Sync(events) => {
                for ev in events.as_ref() {
                    process_event(ev, &mut state, &board.board, &mut commands, &mut ended);
                }

                state.move_allowed = board.turn == state.color;
            }
            ChessMessage::Event(ev) => {
                process_event(&ev, &mut state, &board.board, &mut commands, &mut ended);
            }
            ChessMessage::MoveError => panic!("something must have gone terribly wrong"),
        }
    }
}

fn disconnect(session: Single<Entity, With<Session>>, mut commands: Commands) {
    commands.trigger(Disconnect::new(session.entity(), "client disconnected"));
}

fn on_disconnect(ev: On<Disconnected>) {
    tracing::info!("Disconnected: {:?}", ev.reason);
}

fn on_connect(_: On<Add, Session>) {
    tracing::info!("Connected");
}

pub fn plugin(app: &mut App) {
    app.add_plugins((MeshPickingPlugin, WebSocketClientPlugin, ui::plugin))
        .add_systems(Update, (sync_board, process_msgs))
        .add_systems(OnExit(super::State::Game), (disconnect, despawn_board))
        .insert_resource(ChessBoard {
            board: Board::new(),
        })
        .insert_resource(SelectedSquare(None))
        .init_resource::<ClientState>()
        .add_observer(on_play)
        .add_observer(on_square_selected)
        .add_observer(on_make_move)
        .add_observer(on_stage_move)
        .add_observer(on_promote)
        .add_observer(on_disconnect)
        .add_observer(on_connect)
        .add_message::<GameEnded>()
        .add_message::<SyncBoard>();
}
