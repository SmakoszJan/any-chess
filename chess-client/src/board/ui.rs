use bevy::{
    color::palettes::css::{self, RED},
    picking::hover::Hovered,
    prelude::*,
    ui_widgets::{Activate, Button, observe},
};
use chess_core::Kind;

use crate::{GoBack, board::ClientState};

#[derive(Component)]
struct PromotionUi;

fn setup(assets: Res<AssetServer>, mut commands: Commands) {
    let pieces = [Kind::Queen, Kind::Rook, Kind::Knight, Kind::Bishop];

    commands
        .spawn((
            Node {
                padding: UiRect::all(px(2.0)),
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            PromotionUi,
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
pub struct ShowUi {
    pub at: Vec3,
}

#[derive(Event)]
pub struct HideUi;

fn on_show_ui(
    ev: On<ShowUi>,
    ui: Single<(&mut Node, &mut Visibility), With<PromotionUi>>,
    cam: Single<(&Camera, &GlobalTransform)>,
) {
    let (mut node, mut vis) = ui.into_inner();

    *vis = Visibility::Inherited;
    let pos = cam.0.world_to_viewport(cam.1, ev.at).unwrap();
    node.left = px(pos.x);
    node.top = px(pos.y);
}

fn on_hide_ui(_: On<HideUi>, mut ui: Single<&mut Visibility, With<PromotionUi>>) {
    **ui = Visibility::Hidden;
}

#[derive(Event, Component)]
pub struct Promote(pub Kind);

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
        .add_observer(on_show_ui)
        .add_observer(on_hide_ui);
}
