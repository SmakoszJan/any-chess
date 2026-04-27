use std::marker::PhantomData;

use bevy::{
    color::palettes::css::{self, RED},
    picking::hover::Hovered,
    prelude::*,
    ui_widgets::{Activate, Button, observe},
};
use chess_core::Kind;

use crate::{GoBack, board::ClientState};

#[derive(Component)]
struct Ui<T>(PhantomData<T>);

impl<T> Ui<T> {
    fn new() -> Self {
        Self(PhantomData)
    }
}

fn setup(assets: Res<AssetServer>, mut commands: Commands) {
    let pieces = [
        Kind::Queen,
        Kind::Rook,
        Kind::Knight,
        Kind::Bishop,
        Kind::King,
    ];

    commands
        .spawn((
            Node {
                padding: UiRect::all(px(2.0)),
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            Ui::<Promote>::new(),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            for piece in pieces {
                parent.spawn((
                    ImageNode::new(assets.load(format!("w_{piece:?}.png"))),
                    Node {
                        width: px(64.0),
                        height: px(64.0),
                        ..Default::default()
                    },
                    Button,
                    BackgroundColor(Color::from(css::AQUA)),
                    Hovered::default(),
                    observe(move |_ev: On<Activate>, mut commands: Commands| {
                        commands.trigger(Promote(piece))
                    }),
                    Promote(piece),
                ));
            }
        });

    let rotleft = assets.load("rotate.png");

    commands
        .spawn((Node::DEFAULT, Ui::<Rotate>::new(), Visibility::Hidden))
        .with_children(|parent| {
            // Right
            parent.spawn((
                Node {
                    top: px(-32),
                    left: px(16),
                    position_type: PositionType::Absolute,
                    ..Default::default()
                },
                ImageNode::new(rotleft.clone()),
                Button,
                observe(|_: On<Activate>, mut commands: Commands| {
                    commands.trigger(Rotate(chess_core::Direction::Right));
                }),
            ));
            // Left
            parent.spawn((
                Node {
                    top: px(-32),
                    left: px(-48),
                    position_type: PositionType::Absolute,
                    ..Default::default()
                },
                ImageNode::new(rotleft.clone()).with_flip_x(),
                Button,
                observe(|_: On<Activate>, mut commands: Commands| {
                    commands.trigger(Rotate(chess_core::Direction::Left));
                }),
            ));
        });
}

fn hover_color(
    buttons: Query<(&mut BackgroundColor, &Hovered), (With<Promote>, Changed<Hovered>)>,
) {
    for (mut col, hov) in buttons {
        if hov.0 {
            col.0 = Color::from(css::BLUE);
        } else {
            col.0 = Color::from(css::AQUA);
        }
    }
}

#[derive(Event)]
pub struct ShowUi<T> {
    at: Vec3,
    phantom: PhantomData<T>,
}

impl<T> ShowUi<T> {
    #[must_use]
    pub fn at(at: Vec3) -> Self {
        Self {
            at,
            phantom: PhantomData,
        }
    }
}

#[derive(Event)]
pub struct HideUi<T>(PhantomData<T>);

impl<T> HideUi<T> {
    #[must_use]
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

fn on_show_ui<T: Send + Sync + 'static>(
    ev: On<ShowUi<T>>,
    ui: Single<(&mut Node, &mut Visibility), With<Ui<T>>>,
    cam: Single<(&Camera, &GlobalTransform)>,
) {
    let (mut node, mut vis) = ui.into_inner();

    *vis = Visibility::Inherited;
    let pos = cam.0.world_to_viewport(cam.1, ev.at).unwrap();
    node.left = px(pos.x);
    node.top = px(pos.y);
}

fn on_hide_ui<T: Send + Sync + 'static>(
    _: On<HideUi<T>>,
    mut ui: Single<&mut Visibility, With<Ui<T>>>,
) {
    **ui = Visibility::Hidden;
}

#[derive(Event, Component)]
pub struct Promote(pub Kind);

#[derive(Event, Component)]
pub struct Rotate(pub chess_core::Direction);

#[derive(Component)]
struct GameInfo;

#[derive(Component)]
pub struct WhoseTurn;

#[derive(Component)]
pub struct Victory;

#[derive(Component)]
pub struct YourColor;

#[derive(Component)]
pub struct DisconnectedStatus;

fn spawn_info(mut commands: Commands) {
    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(3.0),
                ..Default::default()
            },
            GameInfo,
        ))
        .with_children(|parent| {
            parent.spawn((Text::new(""), YourColor));
            parent.spawn((Text::new(""), WhoseTurn));
            parent.spawn((Text::new(""), Victory));
            parent.spawn((
                Text::new("Go back"),
                Button,
                observe(|_: On<Activate>, mut ev: MessageWriter<GoBack>| {
                    ev.write(GoBack);
                }),
            ));
            parent.spawn((
                Text::new("Disconnected"),
                TextColor(RED.into()),
                Visibility::Hidden,
                DisconnectedStatus,
            ));
        });
}

fn despawn_info(mut commands: Commands, ui: Single<Entity, With<GameInfo>>) {
    commands.get_entity(ui.entity()).unwrap().despawn();
}

fn update_info(
    state: Res<ClientState>,
    mut your_color: Single<&mut Text, (With<YourColor>, Without<WhoseTurn>, Without<Victory>)>,
    mut whose_turn: Single<&mut Text, (With<WhoseTurn>, Without<YourColor>, Without<Victory>)>,
    mut victory: Single<&mut Text, (With<Victory>, Without<WhoseTurn>, Without<YourColor>)>,
    mut disconnected: Single<&mut Visibility, With<DisconnectedStatus>>,
) {
    if state.is_changed() {
        **your_color = Text(format!(
            "You play as {}",
            match state.color {
                chess_core::Color::White => "white",
                chess_core::Color::Black => "black",
            }
        ));
        **whose_turn = Text(format!(
            "It's {} turn",
            if state.color == state.board.turn {
                "your"
            } else {
                "your opponent's"
            }
        ));

        if let Some(color) = state.board.victory {
            **victory = Text(format!("{color:?} won!"));
        } else {
            **victory = Text::default();
        }

        if state.connected {
            **disconnected = Visibility::Hidden;
        } else {
            **disconnected = Visibility::Inherited;
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                hover_color,
                update_info.run_if(in_state(crate::State::Game)),
            ),
        )
        .add_systems(OnEnter(crate::State::Game), spawn_info)
        .add_systems(OnExit(crate::State::Game), despawn_info)
        .add_observer(on_show_ui::<Rotate>)
        .add_observer(on_hide_ui::<Rotate>)
        .add_observer(on_show_ui::<Promote>)
        .add_observer(on_hide_ui::<Promote>);
}
