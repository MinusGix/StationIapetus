use crate::{
    config::Config,
    control_scheme::{ControlButton, ControlScheme},
    gui::{create_check_box, create_scroll_bar, BuildContext, GuiMessage, ScrollBarData, UiNode},
    level::Level,
    message::Message,
    GameEngine,
};
use rg3d::{
    core::{algebra::Vector2, pool::Handle},
    event::{Event, MouseButton, MouseScrollDelta, WindowEvent},
    gui::{
        border::BorderBuilder,
        button::ButtonBuilder,
        decorator::DecoratorBuilder,
        dropdown_list::DropdownListBuilder,
        grid::{Column, GridBuilder, Row},
        message::{
            ButtonMessage, CheckBoxMessage, DropdownListMessage, MessageDirection,
            ScrollBarMessage, TextMessage, UiMessageData,
        },
        node::UINode,
        scroll_viewer::ScrollViewerBuilder,
        tab_control::{TabControlBuilder, TabDefinition},
        text::TextBuilder,
        widget::WidgetBuilder,
        window::{WindowBuilder, WindowTitle},
        HorizontalAlignment, Orientation, Thickness, VerticalAlignment,
    },
    monitor::VideoMode,
    renderer::ShadowMapPrecision,
    scene::Scene,
    utils::log::{Log, MessageKind},
    window::Fullscreen,
};
use std::sync::mpsc::Sender;

pub struct OptionsMenu {
    pub window: Handle<UiNode>,
    sender: Sender<Message>,
    sound_volume: Handle<UiNode>,
    pub music_volume: Handle<UiNode>,
    video_mode: Handle<UiNode>,
    spot_shadows: Handle<UiNode>,
    soft_spot_shadows: Handle<UiNode>,
    point_shadows: Handle<UiNode>,
    soft_point_shadows: Handle<UiNode>,
    point_shadow_distance: Handle<UiNode>,
    spot_shadow_distance: Handle<UiNode>,
    use_light_scatter: Handle<UiNode>,
    fxaa: Handle<UiNode>,
    ssao: Handle<UiNode>,
    available_video_modes: Vec<VideoMode>,
    control_scheme_buttons: Vec<Handle<UiNode>>,
    active_control_button: Option<usize>,
    mouse_sens: Handle<UiNode>,
    mouse_y_inverse: Handle<UiNode>,
    reset_control_scheme: Handle<UiNode>,
    use_hrtf: Handle<UiNode>,
    reset_audio_settings: Handle<UiNode>,
    point_shadows_quality: Handle<UiNode>,
    spot_shadows_quality: Handle<UiNode>,
}

fn make_text_mark(text: &str, row: usize, ctx: &mut BuildContext) -> Handle<UiNode> {
    TextBuilder::new(
        WidgetBuilder::new()
            .on_row(row)
            .on_column(0)
            .with_margin(Thickness::uniform(2.0)),
    )
    .with_text(text)
    .with_vertical_text_alignment(VerticalAlignment::Center)
    .build(ctx)
}

fn make_tab_header(text: &str, ctx: &mut BuildContext) -> Handle<UiNode> {
    TextBuilder::new(
        WidgetBuilder::new()
            .with_width(100.0)
            .with_height(30.0)
            .with_margin(Thickness::uniform(1.0)),
    )
    .with_text(text)
    .with_vertical_text_alignment(VerticalAlignment::Center)
    .with_horizontal_text_alignment(HorizontalAlignment::Center)
    .build(ctx)
}

fn make_video_mode_item(video_mode: &VideoMode, ctx: &mut BuildContext) -> Handle<UiNode> {
    let size = video_mode.size();
    let rate = video_mode.refresh_rate();
    DecoratorBuilder::new(
        BorderBuilder::new(
            WidgetBuilder::new().with_child(
                TextBuilder::new(WidgetBuilder::new())
                    .with_text(format!("{} x {} @ {}Hz", size.width, size.height, rate).as_str())
                    .with_vertical_text_alignment(VerticalAlignment::Center)
                    .with_horizontal_text_alignment(HorizontalAlignment::Center)
                    .build(ctx),
            ),
        )
        .with_stroke_thickness(Thickness {
            left: 1.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
        }),
    )
    .build(ctx)
}

fn make_shadows_quality_drop_down(
    ctx: &mut BuildContext,
    row: usize,
    current: usize,
) -> Handle<UiNode> {
    DropdownListBuilder::new(
        WidgetBuilder::new()
            .on_row(row)
            .on_column(1)
            .with_margin(Thickness::uniform(1.0)),
    )
    .with_items({
        ["Low", "Medium", "High", "Ultra"]
            .iter()
            .map(|o| {
                DecoratorBuilder::new(BorderBuilder::new(
                    WidgetBuilder::new().with_child(
                        TextBuilder::new(WidgetBuilder::new())
                            .with_text(o)
                            .build(ctx),
                    ),
                ))
                .build(ctx)
            })
            .collect::<Vec<_>>()
    })
    .with_selected(current)
    .build(ctx)
}

fn shadows_quality(size: usize) -> usize {
    if size < 256 {
        0
    } else if (256..512).contains(&size) {
        1
    } else if (512..1024).contains(&size) {
        2
    } else {
        3
    }
}

fn index_to_shadow_map_size(index: usize) -> usize {
    match index {
        0 => 256,
        1 => 512,
        2 => 1024,
        _ => 2048,
    }
}

impl OptionsMenu {
    pub fn new(
        engine: &mut GameEngine,
        control_scheme: &ControlScheme,
        sender: Sender<Message>,
    ) -> Self {
        let video_modes: Vec<VideoMode> = engine
            .get_window()
            .primary_monitor()
            .unwrap()
            .video_modes()
            .filter(|vm| vm.size().width > 800 && vm.size().height > 600 && vm.bit_depth() == 32)
            .collect();

        let ctx = &mut engine.user_interface.build_ctx();

        let common_row = Row::strict(36.0);

        let settings = engine.renderer.get_quality_settings();

        let margin = Thickness::uniform(2.0);

        let sound_volume;
        let music_volume;
        let video_mode;
        let spot_shadows;
        let soft_spot_shadows;
        let point_shadows;
        let soft_point_shadows;
        let point_shadow_distance;
        let spot_shadow_distance;
        let mouse_sens;
        let mouse_y_inverse;
        let reset_control_scheme;
        let mut control_scheme_buttons = Vec::new();
        let use_hrtf;
        let reset_audio_settings;
        let use_light_scatter;
        let fxaa;
        let ssao;
        let point_shadows_quality;
        let spot_shadows_quality;

        let graphics_tab = TabDefinition {
            header: make_tab_header("Graphics", ctx),
            content: {
                ScrollViewerBuilder::new(WidgetBuilder::new())
                    .with_content(
                        GridBuilder::new(
                            WidgetBuilder::new()
                                .with_margin(Thickness::uniform(5.0))
                                .with_child(make_text_mark("Resolution", 0, ctx))
                                .with_child({
                                    video_mode = DropdownListBuilder::new(
                                        WidgetBuilder::new()
                                            .on_column(1)
                                            .on_row(0)
                                            .with_margin(margin),
                                    )
                                    .with_selected(0)
                                    .with_items({
                                        let mut modes =
                                            vec![DecoratorBuilder::new(BorderBuilder::new(
                                                WidgetBuilder::new().with_child(
                                                    TextBuilder::new(WidgetBuilder::new())
                                                        .with_text("Windowed")
                                                        .build(ctx),
                                                ),
                                            ))
                                            .build(ctx)];
                                        modes.extend(video_modes.iter().map(|video_mode| {
                                            make_video_mode_item(video_mode, ctx)
                                        }));
                                        modes
                                    })
                                    .build(ctx);
                                    video_mode
                                })
                                // Spot Shadows Enabled
                                .with_child(make_text_mark("Spot Shadows", 1, ctx))
                                .with_child({
                                    spot_shadows =
                                        create_check_box(ctx, 1, 1, settings.spot_shadows_enabled);
                                    spot_shadows
                                })
                                // Soft Spot Shadows
                                .with_child(make_text_mark("Soft Spot Shadows", 2, ctx))
                                .with_child({
                                    soft_spot_shadows =
                                        create_check_box(ctx, 2, 1, settings.spot_soft_shadows);
                                    soft_spot_shadows
                                })
                                // Spot Shadows Distance
                                .with_child(make_text_mark("Spot Shadows Distance", 3, ctx))
                                .with_child({
                                    spot_shadow_distance = create_scroll_bar(
                                        ctx,
                                        ScrollBarData {
                                            min: 1.0,
                                            max: 15.0,
                                            value: settings.spot_shadows_distance,
                                            step: 0.25,
                                            row: 3,
                                            column: 1,
                                            margin,
                                            show_value: true,
                                            orientation: Orientation::Horizontal,
                                        },
                                    );
                                    spot_shadow_distance
                                })
                                // Point Shadows Enabled
                                .with_child(make_text_mark("Point Shadows", 4, ctx))
                                .with_child({
                                    point_shadows =
                                        create_check_box(ctx, 4, 1, settings.point_shadows_enabled);
                                    point_shadows
                                })
                                // Soft Point Shadows
                                .with_child(make_text_mark("Soft Point Shadows", 5, ctx))
                                .with_child({
                                    soft_point_shadows =
                                        create_check_box(ctx, 5, 1, settings.point_soft_shadows);
                                    soft_point_shadows
                                })
                                // Point Shadows Distance
                                .with_child(make_text_mark("Point Shadows Distance", 6, ctx))
                                .with_child({
                                    point_shadow_distance = create_scroll_bar(
                                        ctx,
                                        ScrollBarData {
                                            min: 1.0,
                                            max: 15.0,
                                            value: settings.point_shadows_distance,
                                            step: 0.25,
                                            row: 6,
                                            column: 1,
                                            margin,
                                            show_value: true,
                                            orientation: Orientation::Horizontal,
                                        },
                                    );
                                    point_shadow_distance
                                })
                                .with_child(make_text_mark("Use Light Scatter", 7, ctx))
                                .with_child({
                                    use_light_scatter =
                                        create_check_box(ctx, 7, 1, settings.light_scatter_enabled);
                                    use_light_scatter
                                })
                                .with_child(make_text_mark("FXAA", 8, ctx))
                                .with_child({
                                    fxaa = create_check_box(ctx, 8, 1, settings.fxaa);
                                    fxaa
                                })
                                .with_child(make_text_mark("SSAO", 9, ctx))
                                .with_child({
                                    ssao = create_check_box(ctx, 9, 1, settings.fxaa);
                                    ssao
                                })
                                .with_child(make_text_mark("Point Shadows Quality", 10, ctx))
                                .with_child({
                                    point_shadows_quality = make_shadows_quality_drop_down(
                                        ctx,
                                        10,
                                        shadows_quality(settings.point_shadow_map_size),
                                    );
                                    point_shadows_quality
                                })
                                .with_child(make_text_mark("Spot Shadows Quality", 11, ctx))
                                .with_child({
                                    spot_shadows_quality = make_shadows_quality_drop_down(
                                        ctx,
                                        11,
                                        shadows_quality(settings.spot_shadow_map_size),
                                    );
                                    spot_shadows_quality
                                }),
                        )
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_column(Column::strict(250.0))
                        .add_column(Column::stretch())
                        .build(ctx),
                    )
                    .build(ctx)
            },
        };

        let sound_tab = TabDefinition {
            header: make_tab_header("Sound", ctx),
            content: {
                ScrollViewerBuilder::new(WidgetBuilder::new())
                    .with_content(
                        GridBuilder::new(
                            WidgetBuilder::new()
                                .with_child(make_text_mark("Sound Volume", 0, ctx))
                                .with_child({
                                    sound_volume = create_scroll_bar(
                                        ctx,
                                        ScrollBarData {
                                            min: 0.0,
                                            max: 1.0,
                                            value: 1.0,
                                            step: 0.025,
                                            row: 0,
                                            column: 1,
                                            margin,
                                            show_value: true,
                                            orientation: Orientation::Horizontal,
                                        },
                                    );
                                    sound_volume
                                })
                                .with_child(make_text_mark("Music Volume", 1, ctx))
                                .with_child({
                                    music_volume = create_scroll_bar(
                                        ctx,
                                        ScrollBarData {
                                            min: 0.0,
                                            max: 1.0,
                                            value: 0.0,
                                            step: 0.025,
                                            row: 1,
                                            column: 1,
                                            margin,
                                            show_value: true,
                                            orientation: Orientation::Horizontal,
                                        },
                                    );
                                    music_volume
                                })
                                .with_child(make_text_mark("Use HRTF", 2, ctx))
                                .with_child({
                                    use_hrtf = create_check_box(ctx, 2, 1, true);
                                    use_hrtf
                                })
                                .with_child({
                                    reset_audio_settings = ButtonBuilder::new(
                                        WidgetBuilder::new().on_row(4).with_margin(margin),
                                    )
                                    .with_text("Reset")
                                    .build(ctx);
                                    reset_audio_settings
                                }),
                        )
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_row(Row::stretch())
                        .add_row(common_row)
                        .add_column(Column::strict(250.0))
                        .add_column(Column::stretch())
                        .build(ctx),
                    )
                    .build(ctx)
            },
        };

        let controls_tab = TabDefinition {
            header: make_tab_header("Controls", ctx),
            content: {
                ScrollViewerBuilder::new(WidgetBuilder::new())
                    .with_content({
                        let mut children = Vec::new();

                        for (row, button) in control_scheme.buttons().iter().enumerate() {
                            // Offset by total amount of rows that goes before
                            let row = row + 2;

                            children.push(make_text_mark(button.description.as_str(), row, ctx));

                            let button = ButtonBuilder::new(
                                WidgetBuilder::new()
                                    .with_margin(margin)
                                    .on_row(row)
                                    .on_column(1),
                            )
                            .with_text(button.button.name())
                            .build(ctx);
                            children.push(button);
                            control_scheme_buttons.push(button);
                        }

                        GridBuilder::new(
                            WidgetBuilder::new()
                                .with_child(make_text_mark("Mouse Sensitivity", 0, ctx))
                                .with_child({
                                    mouse_sens = create_scroll_bar(
                                        ctx,
                                        ScrollBarData {
                                            min: 0.05,
                                            max: 2.0,
                                            value: control_scheme.mouse_sens,
                                            step: 0.05,
                                            row: 0,
                                            column: 1,
                                            margin,
                                            show_value: true,
                                            orientation: Orientation::Horizontal,
                                        },
                                    );
                                    mouse_sens
                                })
                                .with_child(make_text_mark("Inverse Mouse Y", 1, ctx))
                                .with_child({
                                    mouse_y_inverse =
                                        create_check_box(ctx, 1, 1, control_scheme.mouse_y_inverse);
                                    mouse_y_inverse
                                })
                                .with_child({
                                    reset_control_scheme = ButtonBuilder::new(
                                        WidgetBuilder::new()
                                            .on_row(2 + control_scheme.buttons().len())
                                            .with_margin(margin),
                                    )
                                    .with_text("Reset")
                                    .build(ctx);
                                    reset_control_scheme
                                })
                                .with_children(&children),
                        )
                        .add_column(Column::strict(250.0))
                        .add_column(Column::stretch())
                        .add_row(common_row)
                        .add_row(common_row)
                        .add_rows(
                            (0..control_scheme.buttons().len())
                                .map(|_| common_row)
                                .collect(),
                        )
                        .add_row(common_row)
                        .build(ctx)
                    })
                    .build(ctx)
            },
        };

        let tab_control = TabControlBuilder::new(WidgetBuilder::new())
            .with_tab(graphics_tab)
            .with_tab(sound_tab)
            .with_tab(controls_tab)
            .build(ctx);

        let options_window: Handle<UiNode> = WindowBuilder::new(
            WidgetBuilder::new()
                .with_max_size(Vector2::new(f32::INFINITY, 600.0))
                .with_width(500.0),
        )
        .can_minimize(false)
        .with_title(WindowTitle::text("Options"))
        .open(false)
        .with_content(tab_control)
        .build(ctx);

        Self {
            sender,
            window: options_window,
            sound_volume,
            music_volume,
            video_mode,
            spot_shadows,
            soft_spot_shadows,
            point_shadows,
            soft_point_shadows,
            point_shadow_distance,
            spot_shadow_distance,
            available_video_modes: video_modes,
            control_scheme_buttons,
            active_control_button: None,
            mouse_sens,
            mouse_y_inverse,
            reset_control_scheme,
            use_hrtf,
            reset_audio_settings,
            point_shadows_quality,
            use_light_scatter,
            fxaa,
            ssao,
            spot_shadows_quality,
        }
    }

    pub fn sync_to_model(
        &mut self,
        scene: Handle<Scene>,
        engine: &mut GameEngine,
        control_scheme: &ControlScheme,
    ) {
        let ui = &mut engine.user_interface;
        let settings = engine.renderer.get_quality_settings();

        let sync_check_box = |handle: Handle<UiNode>, value: bool| {
            ui.send_message(CheckBoxMessage::checked(
                handle,
                MessageDirection::ToWidget,
                Some(value),
            ));
        };
        sync_check_box(self.spot_shadows, settings.spot_shadows_enabled);
        sync_check_box(self.soft_spot_shadows, settings.spot_soft_shadows);
        sync_check_box(self.point_shadows, settings.point_shadows_enabled);
        sync_check_box(self.soft_point_shadows, settings.point_soft_shadows);
        sync_check_box(self.use_light_scatter, settings.light_scatter_enabled);
        sync_check_box(self.ssao, settings.use_ssao);
        sync_check_box(self.fxaa, settings.fxaa);
        sync_check_box(self.mouse_y_inverse, control_scheme.mouse_y_inverse);
        let is_hrtf = if scene.is_some() {
            matches!(
                engine.scenes[scene].sound_context.state().renderer(),
                rg3d::sound::renderer::Renderer::HrtfRenderer(_)
            )
        } else {
            false
        };

        sync_check_box(self.use_hrtf, is_hrtf);

        let sync_scroll_bar = |handle: Handle<UiNode>, value: f32| {
            ui.send_message(ScrollBarMessage::value(
                handle,
                MessageDirection::ToWidget,
                value,
            ));
        };
        sync_scroll_bar(self.point_shadow_distance, settings.point_shadows_distance);
        sync_scroll_bar(self.spot_shadow_distance, settings.spot_shadows_distance);
        sync_scroll_bar(self.mouse_sens, control_scheme.mouse_sens);
        sync_scroll_bar(
            self.sound_volume,
            engine.sound_engine.lock().unwrap().master_gain(),
        );

        for (btn, def) in self
            .control_scheme_buttons
            .iter()
            .zip(control_scheme.buttons().iter())
        {
            if let UINode::Button(button) = ui.node(*btn) {
                ui.send_message(TextMessage::text(
                    button.content(),
                    MessageDirection::ToWidget,
                    def.button.name().to_owned(),
                ));
            }
        }
    }

    pub fn process_input_event(
        &mut self,
        engine: &mut GameEngine,
        event: &Event<()>,
        control_scheme: &mut ControlScheme,
    ) {
        if let Event::WindowEvent { event, .. } = event {
            let mut control_button = None;

            match event {
                WindowEvent::MouseWheel {
                    delta: MouseScrollDelta::LineDelta(_, y),
                    ..
                } => {
                    if *y != 0.0 {
                        control_button = if *y >= 0.0 {
                            Some(ControlButton::WheelUp)
                        } else {
                            Some(ControlButton::WheelDown)
                        };
                    }
                }
                WindowEvent::KeyboardInput { input, .. } => {
                    if let Some(code) = input.virtual_keycode {
                        control_button = Some(ControlButton::Key(code));
                    }
                }
                WindowEvent::MouseInput { button, .. } => {
                    let index = match button {
                        MouseButton::Left => 1,
                        MouseButton::Right => 2,
                        MouseButton::Middle => 3,
                        MouseButton::Other(i) => *i,
                    };

                    control_button = Some(ControlButton::Mouse(index));
                }
                _ => {}
            }

            if let Some(control_button) = control_button {
                if let Some(active_control_button) = self.active_control_button {
                    if let UINode::Button(button) = engine
                        .user_interface
                        .node(self.control_scheme_buttons[active_control_button])
                    {
                        engine.user_interface.send_message(TextMessage::text(
                            button.content(),
                            MessageDirection::ToWidget,
                            control_button.name().to_owned(),
                        ));
                    }

                    control_scheme.buttons_mut()[active_control_button].button = control_button;

                    self.active_control_button = None;
                }
            }
        }
    }

    #[allow(clippy::cognitive_complexity)]
    pub fn handle_ui_event(
        &mut self,
        engine: &mut GameEngine,
        level: Option<&Level>,
        message: &GuiMessage,
        control_scheme: &mut ControlScheme,
    ) {
        let old_settings = engine.renderer.get_quality_settings();
        let mut settings = old_settings;

        let mut changed = false;

        match message.data() {
            UiMessageData::ScrollBar(ScrollBarMessage::Value(new_value))
                if message.direction() == MessageDirection::FromWidget =>
            {
                if message.destination() == self.sound_volume {
                    engine
                        .sound_engine
                        .lock()
                        .unwrap()
                        .set_master_gain(*new_value);
                    changed = true;
                } else if message.destination() == self.point_shadow_distance {
                    settings.point_shadows_distance = *new_value;
                    changed = true;
                } else if message.destination() == self.spot_shadow_distance {
                    settings.spot_shadows_distance = *new_value;
                    changed = true;
                } else if message.destination() == self.mouse_sens {
                    control_scheme.mouse_sens = *new_value;
                    changed = true;
                } else if message.destination() == self.music_volume {
                    self.sender
                        .send(Message::SetMusicVolume { volume: *new_value })
                        .unwrap();
                    changed = true;
                }
            }
            UiMessageData::DropdownList(DropdownListMessage::SelectionChanged(Some(index))) => {
                if message.destination() == self.video_mode {
                    // -1 here because we have Windowed item in the list.
                    if let Some(video_mode) = self.available_video_modes.get(*index - 1) {
                        engine
                            .get_window()
                            .set_fullscreen(Some(Fullscreen::Exclusive(video_mode.clone())));
                        changed = true;
                    } else {
                        engine.get_window().set_fullscreen(None);
                        changed = true;
                    }
                } else if message.destination() == self.spot_shadows_quality {
                    settings.spot_shadow_map_size = index_to_shadow_map_size(*index);
                    if *index > 0 {
                        settings.spot_shadow_map_precision = ShadowMapPrecision::Full;
                    } else {
                        settings.spot_shadow_map_precision = ShadowMapPrecision::Half;
                    }
                    changed = true;
                } else if message.destination() == self.point_shadows_quality {
                    settings.point_shadow_map_size = index_to_shadow_map_size(*index);
                    if *index > 0 {
                        settings.point_shadow_map_precision = ShadowMapPrecision::Full;
                    } else {
                        settings.point_shadow_map_precision = ShadowMapPrecision::Half;
                    }
                    changed = true;
                }
            }
            UiMessageData::CheckBox(msg) => {
                let CheckBoxMessage::Check(value) = msg;
                let value = value.unwrap_or(false);
                if message.destination() == self.point_shadows {
                    settings.point_shadows_enabled = value;
                    changed = true;
                } else if message.destination() == self.spot_shadows {
                    settings.spot_shadows_enabled = value;
                    changed = true;
                } else if message.destination() == self.soft_spot_shadows {
                    settings.spot_soft_shadows = value;
                    changed = true;
                } else if message.destination() == self.soft_point_shadows {
                    settings.point_soft_shadows = value;
                    changed = true;
                } else if message.destination() == self.mouse_y_inverse {
                    control_scheme.mouse_y_inverse = value;
                    changed = true;
                } else if message.destination() == self.use_light_scatter {
                    settings.light_scatter_enabled = value;
                    changed = true;
                } else if message.destination() == self.fxaa {
                    settings.fxaa = value;
                    changed = true;
                } else if message.destination() == self.ssao {
                    settings.use_ssao = value;
                    changed = true;
                }
            }
            UiMessageData::Button(ButtonMessage::Click) => {
                if message.destination() == self.reset_control_scheme {
                    control_scheme.reset();
                    self.sync_to_model(
                        level.map_or(Default::default(), |m| m.scene),
                        engine,
                        control_scheme,
                    );
                    changed = true;
                } else if message.destination() == self.reset_audio_settings {
                    engine.sound_engine.lock().unwrap().set_master_gain(1.0);
                    self.sync_to_model(
                        level.map_or(Default::default(), |m| m.scene),
                        engine,
                        control_scheme,
                    );
                    changed = true;
                }

                for (i, button) in self.control_scheme_buttons.iter().enumerate() {
                    if message.destination() == *button {
                        if let UINode::Button(button) = engine.user_interface.node(*button) {
                            engine.user_interface.send_message(TextMessage::text(
                                button.content(),
                                MessageDirection::ToWidget,
                                "[WAITING INPUT]".to_owned(),
                            ))
                        }

                        self.active_control_button = Some(i);
                    }
                }
            }
            _ => (),
        }

        if settings != old_settings {
            if let Err(err) = engine.renderer.set_quality_settings(&settings) {
                Log::writeln(
                    MessageKind::Error,
                    format!("Failed to set renderer quality settings! Reason: {:?}", err),
                );
            }
        }

        if changed {
            match Config::save(engine, control_scheme.clone(), Default::default()) {
                Ok(_) => {
                    Log::writeln(MessageKind::Information, "Settings saved!".to_string());
                }
                Err(e) => Log::writeln(
                    MessageKind::Error,
                    format!("Failed to save settings. Reason: {:?}", e),
                ),
            }
        }
    }
}
