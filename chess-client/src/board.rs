use std::f32::consts::PI;

use aeronet_io::{
    Session,
    connection::{Disconnect, Disconnected},
};
use aeronet_websocket::client::WebSocketClientPlugin;
use bevy::{color::palettes::css, ecs::query::QueryData, prelude::*};
use chess_core::{Board, ChessMove, Color as ChessColor, Kind, Move, Pos, net::ClientMessage};

use ui::Promote;

use crate::{
    board::ui::{HideUi, Rotate, ShowUi},
    net::{BoardPosition, RoomInfo, ServerBoard},
};

mod ui;

#[derive(Message)]
struct SyncBoard;

#[derive(Component)]
struct PieceMarker;

#[derive(Component)]
struct MoveMarker;

fn sync_board(
    mut state: ResMut<ClientState>,
    board: Res<ServerBoard>,
    mut writer: MessageWriter<SyncBoard>,
    mut commands: Commands,
) {
    if board.is_changed() {
        state.board.clone_from(&board);
        state.move_allowed = board.turn == state.color;
        commands.trigger(StageMove(None));
        writer.write(SyncBoard);
    }
}

fn sync_ui(
    mut pieces: Query<
        (&BoardPosition, &mut Sprite, &mut Visibility, &mut Transform),
        With<PieceMarker>,
    >,
    mut reader: MessageReader<SyncBoard>,
    state: Res<ClientState>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    if !reader.is_empty() {
        reader.clear();

        pieces
            .par_iter_mut()
            .for_each(|(&sq, mut sprite, mut vis, mut trans)| {
                if let Some(piece) = state.board[sq.0] {
                    *vis = Visibility::Inherited;
                    let color_letter = match piece.color {
                        chess_core::Color::White => 'w',
                        chess_core::Color::Black => 'b',
                    };
                    sprite.image = assets.load(format!("{color_letter}_{:?}.png", piece.kind));
                    let mut dir = piece.direction;
                    if state.color == chess_core::Color::Black {
                        dir = !dir;
                    }
                    if piece.color != state.color {
                        if dir == chess_core::Direction::Up {
                            dir = chess_core::Direction::Down;
                        } else if dir == chess_core::Direction::Down {
                            dir = chess_core::Direction::Up;
                        }
                    }
                    *trans = trans.with_rotation(Quat::from_rotation_z(
                        match dir {
                            chess_core::Direction::Up => 0.0,
                            chess_core::Direction::Left => 1.0,
                            chess_core::Direction::Down => 2.0,
                            chess_core::Direction::Right => 3.0,
                        } * PI
                            / 2.0,
                    ))
                } else {
                    *vis = Visibility::Hidden;
                }
            });

        commands.trigger(StageMove(None));
    }
}

#[derive(Debug, Default)]
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
    board: Board,
    connected: bool,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            room: 0,
            color: ChessColor::White,
            move_allowed: false,
            move_state: MoveState::None,
            board: Board::new(),
            connected: false,
        }
    }
}

#[derive(Message)]
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

const WHITE: Color = Color::srgb_u8(0xf0, 0xd9, 0xb5);
const BLACK: Color = Color::srgb_u8(0xb5, 0x88, 0x63);

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
            BLACK
        } else {
            WHITE
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

#[derive(Event, Debug)]
struct StageMove(Option<Entity>);

fn on_stage_move(
    ev: On<StageMove>,
    mut make_move: MessageWriter<MakeMove>,
    squares: Query<(&Children, &BoardPosition, &GlobalTransform)>,
    mut pieces: Query<&mut Transform>,
    mut markers: Query<(&mut Sprite, &mut Visibility, &BoardPosition), With<MoveMarker>>,
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
    commands.trigger(HideUi::<Promote>::new());
    commands.trigger(HideUi::<Rotate>::new());

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
        let mut m = ChessMove::new(**from.1, **to.1);

        if let Some(piece) = state.board[from.1.0]
            && piece.kind == Kind::Pawn
            && (to.1.rank == 0 || to.1.rank == 7)
        {
            m.promotion = Some(Kind::Queen);
        }

        if m.check(&state.board).is_ok() {
            if m.promotion.is_some() {
                state.move_state = MoveState::Promote {
                    from: current,
                    to: picked,
                };
                commands.trigger(ShowUi::<Promote>::at(
                    to.2.translation() - Vec3::new(32.0, -128.0, 0.0),
                ));
            } else {
                make_move.write(MakeMove(m, true));
            }
            return;
        }
    }

    // Otherwise we stage a new one
    if state.move_allowed {
        state.move_state = ev.0.map_or(MoveState::None, MoveState::Start);
    }

    if let MoveState::Start(current) = state.move_state
        && let Ok(sq) = squares.get(current)
    {
        // Lift the piece
        if let Ok(mut t) = pieces.get_mut(sq.0[0]) {
            t.translation.y = 16.0;
        }

        // Show rotation
        if let Some(piece) = state.board[sq.1.0]
            && piece.kind == Kind::Pawn
        {
            commands.trigger(ShowUi::<Rotate>::at(sq.2.translation()));
        }

        // Put markers
        let current = *sq.1;

        for (mut sprite, mut vis, pos) in markers {
            let mut m = ChessMove {
                from: current.0,
                to: pos.0,
                promotion: None,
                direction: None,
            };
            if let Some(piece) = state.board[current.0]
                && piece.kind == Kind::Pawn
                && (pos.rank == 0 || pos.rank == 7)
            {
                m.promotion = Some(Kind::Queen);
            }

            if m.check(&state.board).is_ok() {
                *vis = Visibility::Inherited;
                if state.board[pos.0].is_some() {
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
    mut make_move: MessageWriter<MakeMove>,
    mut commands: Commands,
) {
    let MoveState::Promote { from, to } = state.move_state else {
        return;
    };
    let from = pieces.get(from).unwrap();
    let to = pieces.get(to).unwrap();

    make_move.write(MakeMove(
        ChessMove {
            from: from.0,
            to: to.0,
            promotion: Some(ev.0),
            direction: None,
        },
        true,
    ));
    commands.trigger(StageMove(None));
}

fn on_rotate(
    ev: On<Rotate>,
    squares: Query<&BoardPosition>,
    state: Res<ClientState>,
    mut make_move: MessageWriter<MakeMove>,
    mut commands: Commands,
) {
    let MoveState::Start(sq) = state.move_state else {
        return;
    };
    let pos = squares.get(sq).unwrap().0;
    let dir = state.board[pos].unwrap().direction;
    let dir = match ev.0 {
        chess_core::Direction::Left => dir.left(),
        chess_core::Direction::Right => dir.right(),
        _ => unreachable!(),
    };

    make_move.write(MakeMove(
        ChessMove {
            from: pos,
            to: pos,
            promotion: None,
            direction: Some(dir),
        },
        true,
    ));
    commands.trigger(StageMove(None));
}

fn on_make_move(
    mut m: ResMut<Messages<MakeMove>>,
    mut writer: MessageWriter<SyncBoard>,
    mut session: Single<&mut Session>,
    mut state: ResMut<ClientState>,
) {
    for m in m.drain() {
        m.0.exec(&mut state.board);
        writer.write(SyncBoard);
        if m.1 {
            session.send.push(
                serde_json::to_vec(&ClientMessage::Move(m.0))
                    .unwrap()
                    .into(),
            );
        }

        state.move_state = MoveState::None;
        state.move_allowed = state.board.turn == state.color;
    }
}

fn on_square_clicked(click: On<Pointer<Click>>, mut commands: Commands) {
    commands.trigger(SelectSquare(Some(click.entity)));
    commands.trigger(StageMove(Some(click.entity)));
}

fn spawn_board(
    room: Res<RoomInfo>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut writer: MessageWriter<SyncBoard>,
    mut client: ResMut<ClientState>,
    server: Res<ServerBoard>,
) {
    client.color = room.color;
    client.room = room.room;
    client.move_allowed = false;
    client.move_state = MoveState::None;
    client.board = server.clone();
    client.move_allowed = server.turn == room.color;
    let square = meshes.add(Rectangle::new(64.0, 64.0));
    let white = materials.add(WHITE);
    let black = materials.add(BLACK);

    let mut x = if room.color.is_white() { -224.0 } else { 224.0 };
    let mut y = if room.color.is_white() { -224.0 } else { 224.0 };
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
                    BoardPosition(Pos { rank, file }),
                ))
                .observe(on_square_clicked)
                .with_children(|children| {
                    children.spawn((
                        Sprite::default(),
                        Transform::from_scale(Vec3::splat(0.5)),
                        BoardPosition(Pos { rank, file }),
                        PieceMarker,
                    ));
                    children.spawn((
                        Sprite::default(),
                        BoardPosition(Pos { rank, file }),
                        MoveMarker,
                        Visibility::Hidden,
                    ));
                });
            if room.color.is_white() {
                x += 64.0;
            } else {
                x -= 64.0;
            }
        }

        if room.color.is_white() {
            y += 64.0;
            x = -224.0;
        } else {
            y -= 64.0;
            x = 224.0;
        }
    }

    writer.write(SyncBoard);
}

fn despawn_board(tiles: Query<Entity, With<Pickable>>, mut commands: Commands) {
    for tile in tiles {
        commands.get_entity(tile).unwrap().despawn();
    }
}

#[derive(Message)]
pub struct GameEnded(pub i32);

fn disconnect(session: Single<Entity, With<Session>>, mut commands: Commands) {
    commands.trigger(Disconnect::new(session.entity(), "client disconnected"));
}

fn on_disconnect(ev: On<Disconnected>, mut client: ResMut<ClientState>) {
    client.connected = false;
    tracing::info!("Disconnected: {:?}", ev.reason);
}

fn on_connect(_: On<Add, Session>, mut client: ResMut<ClientState>) {
    client.connected = true;
    tracing::info!("Connected");
}

pub fn plugin(app: &mut App) {
    app.add_plugins((MeshPickingPlugin, WebSocketClientPlugin, ui::plugin))
        .add_systems(
            Update,
            (sync_ui.after(sync_board), sync_board, on_make_move),
        )
        .add_systems(OnEnter(super::State::Game), spawn_board)
        .add_systems(OnExit(super::State::Game), (disconnect, despawn_board))
        .insert_resource(SelectedSquare(None))
        .init_resource::<ClientState>()
        // .add_observer(on_play)
        .add_observer(on_square_selected)
        .add_observer(on_stage_move)
        .add_observer(on_promote)
        .add_observer(on_rotate)
        .add_observer(on_disconnect)
        .add_observer(on_connect)
        .add_message::<GameEnded>()
        .add_message::<MakeMove>()
        .add_message::<SyncBoard>();
}
