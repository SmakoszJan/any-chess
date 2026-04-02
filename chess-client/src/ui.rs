use bevy::{
    color::palettes::css,
    input_focus::InputDispatchPlugin,
    picking::hover::Hovered,
    prelude::*,
    ui_widgets::{Activate, Button, UiWidgetsPlugins, observe},
};
use chess_core::Kind;

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

fn hover_color(buttons: Query<(&mut BackgroundColor, &Hovered), (With<Button>, Changed<Hovered>)>) {
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

pub fn plugin(app: &mut App) {
    app.add_plugins((UiWidgetsPlugins, InputDispatchPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, (hover_color,))
        .add_observer(on_show_ui)
        .add_observer(on_hide_ui);
}
