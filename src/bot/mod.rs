use crate::{
    actor::{Actor, TargetDescriptor},
    bot::{
        lower_body::{LowerBodyMachine, LowerBodyMachineInput},
        upper_body::{UpperBodyMachine, UpperBodyMachineInput},
    },
    character::{find_hit_boxes, Character},
    item::ItemKind,
    level::{footstep_ray_check, UpdateContext},
    message::Message,
    utils::BodyImpactHandler,
    weapon::projectile::Damage,
    CollisionGroups, GameTime,
};
use rg3d::{
    animation::machine::{Machine, PoseNode},
    core::{
        algebra::{Isometry3, Matrix4, Point3, Translation3, UnitQuaternion, Vector3},
        color::Color,
        math::{frustum::Frustum, ray::Ray, SmoothAngle},
        pool::Handle,
        rand::Rng,
        visitor::{Visit, VisitResult, Visitor},
    },
    engine::resource_manager::ResourceManager,
    lazy_static::lazy_static,
    physics::{
        dynamics::{BodyStatus, CoefficientCombineRule, RigidBodyBuilder},
        geometry::{ColliderBuilder, InteractionGroups},
    },
    scene::{
        self,
        base::BaseBuilder,
        graph::Graph,
        node::Node,
        physics::{Physics, RayCastOptions},
        transform::TransformBuilder,
        ColliderHandle, Scene, SceneDrawingContext,
    },
    utils::{
        log::{Log, MessageKind},
        navmesh::{Navmesh, NavmeshAgent, NavmeshAgentBuilder},
    },
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs::File,
    ops::{Deref, DerefMut},
    sync::mpsc::Sender,
};

mod lower_body;
mod upper_body;

#[derive(Deserialize, Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum BotKind {
    Mutant,
    Parasite,
    Zombie,
}

impl BotKind {
    pub fn from_id(id: i32) -> Result<Self, String> {
        match id {
            0 => Ok(BotKind::Mutant),
            1 => Ok(BotKind::Parasite),
            2 => Ok(BotKind::Zombie),
            _ => Err(format!("Invalid bot kind {}", id)),
        }
    }

    pub fn id(self) -> i32 {
        match self {
            BotKind::Mutant => 0,
            BotKind::Parasite => 1,
            BotKind::Zombie => 2,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            BotKind::Mutant => "Mutant",
            BotKind::Parasite => "Parasite",
            BotKind::Zombie => "Zombie",
        }
    }
}

#[derive(Debug)]
pub struct Target {
    position: Vector3<f32>,
    handle: Handle<Actor>,
}

impl Default for Target {
    fn default() -> Self {
        Self {
            position: Default::default(),
            handle: Default::default(),
        }
    }
}

impl Visit for Target {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        self.position.visit("Position", visitor)?;
        self.handle.visit("Handle", visitor)?;

        visitor.leave_region()
    }
}

pub struct Bot {
    target: Option<Target>,
    kind: BotKind,
    model: Handle<Node>,
    character: Character,
    pub definition: &'static BotDefinition,
    lower_body_machine: LowerBodyMachine,
    upper_body_machine: UpperBodyMachine,
    last_health: f32,
    restoration_time: f32,
    move_target: Vector3<f32>,
    frustum: Frustum,
    last_move_dir: Vector3<f32>,
    spine: Handle<Node>,
    yaw: SmoothAngle,
    pitch: SmoothAngle,
    attack_timeout: f32,
    hips: Handle<Node>,
    attack_animation_index: u32,
    agent: NavmeshAgent,
    pub impact_handler: BodyImpactHandler,
}

impl Deref for Bot {
    type Target = Character;

    fn deref(&self) -> &Self::Target {
        &self.character
    }
}

impl DerefMut for Bot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.character
    }
}

impl Default for Bot {
    fn default() -> Self {
        Self {
            character: Default::default(),
            kind: BotKind::Mutant,
            model: Default::default(),
            target: Default::default(),
            definition: Self::get_definition(BotKind::Mutant),
            lower_body_machine: Default::default(),
            upper_body_machine: Default::default(),
            last_health: 0.0,
            restoration_time: 0.0,
            move_target: Default::default(),
            frustum: Default::default(),
            last_move_dir: Default::default(),
            spine: Default::default(),
            yaw: SmoothAngle {
                angle: 0.0,
                target: 0.0,
                speed: 260.0f32.to_radians(), // rad/s
            },
            pitch: SmoothAngle {
                angle: 0.0,
                target: 0.0,
                speed: 260.0f32.to_radians(), // rad/s
            },
            attack_timeout: 0.0,
            hips: Default::default(),
            attack_animation_index: 0,
            agent: Default::default(),
            impact_handler: Default::default(),
        }
    }
}

#[derive(Deserialize)]
pub struct AttackAnimationDefinition {
    path: String,
    timestamp: f32,
    damage: Damage,
    speed: f32,
}

#[derive(Deserialize)]
pub struct BotDefinition {
    pub scale: f32,
    pub health: f32,
    pub walk_speed: f32,
    pub weapon_scale: f32,
    pub model: String,
    pub weapon_hand_name: String,
    pub left_leg_name: String,
    pub right_leg_name: String,
    pub spine: String,
    pub hips: String,
    pub v_aim_angle_hack: f32,
    pub can_use_weapons: bool,
    pub close_combat_distance: f32,
    pub pain_sounds: Vec<String>,
    pub scream_sounds: Vec<String>,
    pub idle_sounds: Vec<String>,

    // Animations.
    pub idle_animation: String,
    pub scream_animation: String,
    pub attack_animations: Vec<AttackAnimationDefinition>,
    pub walk_animation: String,
    pub aim_animation: String,
    pub dying_animation: String,
}

#[derive(Deserialize, Default)]
pub struct BotDefinitionsContainer {
    map: HashMap<BotKind, BotDefinition>,
}

impl BotDefinitionsContainer {
    pub fn new() -> Self {
        let file = File::open("data/configs/bots.ron").unwrap();
        ron::de::from_reader(file).unwrap()
    }
}

lazy_static! {
    static ref DEFINITIONS: BotDefinitionsContainer = BotDefinitionsContainer::new();
}

impl Bot {
    pub fn get_definition(kind: BotKind) -> &'static BotDefinition {
        DEFINITIONS.map.get(&kind).unwrap()
    }

    pub async fn new(
        kind: BotKind,
        resource_manager: ResourceManager,
        scene: &mut Scene,
        position: Vector3<f32>,
        rotation: UnitQuaternion<f32>,
        sender: Sender<Message>,
    ) -> Self {
        let definition = Self::get_definition(kind);

        let body_height = 0.55;
        let body_radius = 0.16;

        let model = resource_manager
            .request_model(&definition.model)
            .await
            .unwrap()
            .instantiate_geometry(scene);

        scene.graph[model]
            .local_transform_mut()
            .set_position(Vector3::new(0.0, -body_height * 0.5 - body_radius, 0.0))
            .set_scale(Vector3::new(
                definition.scale,
                definition.scale,
                definition.scale,
            ));

        let spine = scene.graph.find_by_name(model, &definition.spine);
        if spine.is_none() {
            Log::writeln(
                MessageKind::Warning,
                "Spine bone not found, bot won't aim vertically!".to_owned(),
            );
        }

        let pivot = BaseBuilder::new()
            .with_children(&[model])
            .build(&mut scene.graph);

        let body = scene.physics.add_body(
            RigidBodyBuilder::new(BodyStatus::Dynamic)
                .lock_rotations()
                .position(Isometry3 {
                    translation: Translation3 { vector: position },
                    rotation,
                })
                .build(),
        );
        scene.physics.add_collider(
            ColliderBuilder::capsule_y(body_height * 0.5, body_radius)
                .friction(0.0)
                .friction_combine_rule(CoefficientCombineRule::Min)
                .collision_groups(InteractionGroups::new(
                    CollisionGroups::ActorCapsule as u16,
                    0xFFFF,
                ))
                .build(),
            body,
        );

        scene.physics_binder.bind(pivot, body);

        let hand = scene
            .graph
            .find_by_name(model, &definition.weapon_hand_name);
        let wpn_scale = definition.weapon_scale * (1.0 / definition.scale);
        let weapon_pivot = BaseBuilder::new()
            .with_local_transform(
                TransformBuilder::new()
                    .with_local_scale(Vector3::new(wpn_scale, wpn_scale, wpn_scale))
                    .with_local_rotation(
                        UnitQuaternion::from_axis_angle(&Vector3::x_axis(), -90.0f32.to_radians())
                            * UnitQuaternion::from_axis_angle(
                                &Vector3::z_axis(),
                                -90.0f32.to_radians(),
                            ),
                    )
                    .build(),
            )
            .build(&mut scene.graph);

        scene.graph.link_nodes(weapon_pivot, hand);

        let hips = scene.graph.find_by_name(model, &definition.hips);

        let lower_body_machine =
            LowerBodyMachine::new(resource_manager.clone(), &definition, model, scene).await;
        let upper_body_machine =
            UpperBodyMachine::new(resource_manager.clone(), definition, model, scene, hips).await;

        Self {
            character: Character {
                pivot,
                body,
                weapon_pivot,
                health: definition.health,
                sender: Some(sender),
                hit_boxes: find_hit_boxes(pivot, scene),
                ..Default::default()
            },
            hips,
            spine,
            definition,
            last_health: definition.health,
            model,
            kind,
            lower_body_machine,
            upper_body_machine,
            agent: NavmeshAgentBuilder::new()
                .with_position(position)
                .with_speed(definition.walk_speed)
                .build(),
            ..Default::default()
        }
    }

    pub fn can_be_removed(&self, scene: &Scene) -> bool {
        scene
            .animations
            .get(self.upper_body_machine.dying_animation)
            .has_ended()
    }

    pub fn can_shoot(&self) -> bool {
        self.upper_body_machine.machine.active_state() == self.upper_body_machine.aim_state
            && self.definition.can_use_weapons
    }

    fn select_target(
        &mut self,
        self_handle: Handle<Actor>,
        scene: &mut Scene,
        targets: &[TargetDescriptor],
    ) {
        // Check if existing target is valid.
        if let Some(target) = self.target.as_mut() {
            for target_desc in targets {
                if target_desc.handle != self_handle
                    && target_desc.handle == target.handle
                    && target_desc.health > 0.0
                {
                    target.position = target_desc.position;
                    return;
                }
            }
        }

        let position = self.character.position(&scene.graph);
        let mut closest_distance = std::f32::MAX;

        let mut query_buffer = Vec::default();
        'target_loop: for desc in targets.iter().filter(|desc| desc.handle != self_handle) {
            let distance = position.metric_distance(&desc.position);
            if distance != 0.0 && distance < 1.6 || self.frustum.is_contains_point(desc.position) {
                let ray = Ray::from_two_points(desc.position, position);
                scene.physics.cast_ray(
                    RayCastOptions {
                        ray,
                        groups: InteractionGroups::all(),
                        max_len: ray.dir.norm(),
                        sort_results: true,
                    },
                    &mut query_buffer,
                );

                'hit_loop: for hit in query_buffer.iter() {
                    let collider = scene.physics.colliders.get(hit.collider.into()).unwrap();
                    let body = collider.parent();

                    if collider.shape().as_trimesh().is_some() {
                        // Target is behind something.
                        continue 'target_loop;
                    } else {
                        // Prevent setting self as target.
                        if self.character.body == body.into() {
                            continue 'hit_loop;
                        }
                    }
                }

                if distance < closest_distance {
                    self.target = Some(Target {
                        position: desc.position,
                        handle: desc.handle,
                    });
                    closest_distance = distance;
                }
            }
        }
    }

    pub fn debug_draw(&self, context: &mut SceneDrawingContext) {
        for pts in self.agent.path().windows(2) {
            let a = pts[0];
            let b = pts[1];
            context.add_line(scene::Line {
                begin: a,
                end: b,
                color: Color::from_rgba(255, 0, 0, 255),
            });
        }

        context.draw_frustum(&self.frustum, Color::from_rgba(0, 200, 0, 255));
    }

    fn update_frustum(&mut self, position: Vector3<f32>, graph: &Graph) {
        let head_pos = position + Vector3::new(0.0, 0.4, 0.0);
        let up = graph[self.model].up_vector();
        let look_at = head_pos + graph[self.model].look_vector();
        let view_matrix = Matrix4::look_at_rh(&Point3::from(head_pos), &Point3::from(look_at), &up);
        let projection_matrix =
            Matrix4::new_perspective(16.0 / 9.0, 90.0f32.to_radians(), 0.1, 20.0);
        let view_projection_matrix = projection_matrix * view_matrix;
        self.frustum = Frustum::from(view_projection_matrix).unwrap();
    }

    fn aim_vertically(&mut self, look_dir: Vector3<f32>, graph: &mut Graph, time: GameTime) {
        let angle = self.pitch.angle();
        self.pitch
            .set_target(
                look_dir.dot(&Vector3::y()).acos() - std::f32::consts::PI / 2.0
                    + self.definition.v_aim_angle_hack.to_radians(),
            )
            .update(time.delta);

        if self.spine.is_some() {
            graph[self.spine]
                .local_transform_mut()
                .set_rotation(UnitQuaternion::from_axis_angle(&Vector3::x_axis(), angle));
        }
    }

    fn aim_horizontally(&mut self, look_dir: Vector3<f32>, physics: &mut Physics, time: GameTime) {
        let angle = self.yaw.angle();
        self.yaw
            .set_target(look_dir.x.atan2(look_dir.z))
            .update(time.delta);

        let body = physics.bodies.get_mut(self.body.into()).unwrap();
        let mut position = *body.position();
        position.rotation = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), angle);
        body.set_position(position, true);
    }

    fn update_agent(&mut self, navmesh: &mut Navmesh, time: GameTime) {
        if let Some(target) = self.target.as_ref() {
            self.agent.set_target(target.position);
            let _ = self.agent.update(time.delta, navmesh);
        }
    }

    pub fn set_target(&mut self, handle: Handle<Actor>, position: Vector3<f32>) {
        self.target = Some(Target { position, handle });
    }

    fn calculate_movement_speed_factor(&self, physics: &Physics) -> f32 {
        let mut k = 1.0;

        // Slowdown bot according to damaged body parts.
        for hitbox in self.hit_boxes.iter() {
            let body = physics
                .colliders
                .get(hitbox.collider.into())
                .unwrap()
                .parent();
            if self.impact_handler.is_affected(body.into()) {
                k = hitbox.movement_speed_factor.min(k);
            }
        }

        k
    }

    pub fn update(
        &mut self,
        self_handle: Handle<Actor>,
        context: &mut UpdateContext,
        targets: &[TargetDescriptor],
    ) {
        let mut is_moving = false;
        let mut can_aim = false;
        let mut in_close_combat = false;
        let mut movement_speed_factor = 1.0;

        if self.is_dead() {
            for &animation in &[
                self.upper_body_machine.dying_animation,
                self.lower_body_machine.dying_animation,
            ] {
                context
                    .scene
                    .animations
                    .get_mut(animation)
                    .set_enabled(true);
            }

            for &animation in self.upper_body_machine.attack_animations.iter() {
                context
                    .scene
                    .animations
                    .get_mut(animation)
                    .set_enabled(false);
            }

            if self.body.is_some() {
                context.scene.physics.remove_body(self.body);
                self.body = Default::default();
            }
        } else {
            movement_speed_factor = self.calculate_movement_speed_factor(&context.scene.physics);
            self.agent
                .set_speed(self.definition.walk_speed * movement_speed_factor);

            self.select_target(self_handle, context.scene, targets);

            let body = context
                .scene
                .physics
                .bodies
                .get_mut(self.character.body.into())
                .unwrap();
            let look_dir = match self.target.as_ref() {
                None => {
                    in_close_combat = false;
                    Vector3::z()
                }
                Some(target) => {
                    let d = target.position - body.position().translation.vector;
                    in_close_combat = d.norm() <= self.definition.close_combat_distance;
                    d
                }
            };

            let position = body.position().translation.vector;
            let navmesh = &mut context.scene.navmeshes[context.navmesh];
            self.agent.warp(position);
            self.update_agent(navmesh, context.time);
            self.move_target = self.agent.position();

            self.update_frustum(position, &context.scene.graph);

            let was_damaged = self.character.health < self.last_health;
            if was_damaged {
                self.restoration_time = 0.8;
            }

            can_aim = self.restoration_time <= 0.0;
            self.last_health = self.character.health;

            if !self.is_dead() && !in_close_combat && self.target.is_some() {
                let mut vel = (self.move_target - position).scale(1.0 / context.time.delta);
                vel.y = body.linvel().y;
                body.set_linvel(vel, true);
                self.last_move_dir = vel;
                is_moving = true;
            } else {
                body.set_linvel(Vector3::new(0.0, body.linvel().y, 0.0), true);
            }

            let sender = self.character.sender.clone().unwrap();

            if !in_close_combat && can_aim && self.can_shoot() && self.target.is_some() {
                if let Some(weapon) = self
                    .character
                    .weapons
                    .get(self.character.current_weapon as usize)
                {
                    let weapon = *weapon;

                    let ammo_per_shot =
                        context.weapons[weapon].definition.ammo_consumption_per_shot;

                    if context.weapons[weapon].can_shoot(context.time)
                        && self
                            .inventory
                            .try_extract_exact_items(ItemKind::Ammo, ammo_per_shot)
                            == ammo_per_shot
                    {
                        sender
                            .send(Message::ShootWeapon {
                                weapon,
                                direction: Some(look_dir),
                            })
                            .unwrap();
                    }
                }
            }

            let current_attack_animation =
                self.upper_body_machine.attack_animations[self.attack_animation_index as usize];

            // Apply damage to target from melee attack
            if let Some(target) = self.target.as_ref() {
                while let Some(event) = context
                    .scene
                    .animations
                    .get_mut(current_attack_animation)
                    .pop_event()
                {
                    if event.signal_id == UpperBodyMachine::HIT_SIGNAL && in_close_combat {
                        sender
                            .send(Message::DamageActor {
                                actor: target.handle,
                                who: Default::default(),
                                amount: self.definition.attack_animations
                                    [self.attack_animation_index as usize]
                                    .damage
                                    .amount(),
                            })
                            .unwrap();
                    }
                }
            }

            // Emit step sounds from walking animation.
            if self.lower_body_machine.is_walking() {
                while let Some(event) = context
                    .scene
                    .animations
                    .get_mut(self.lower_body_machine.walk_animation)
                    .pop_event()
                {
                    if event.signal_id == LowerBodyMachine::STEP_SIGNAL {
                        let begin = context.scene.graph[self.model].global_position()
                            + Vector3::new(0.0, 10.0, 0.0);

                        let self_collider = if let Some(body) =
                            context.scene.physics.bodies.get(self.body.into())
                        {
                            ColliderHandle::from(body.colliders()[0])
                        } else {
                            Default::default()
                        };

                        footstep_ray_check(begin, context.scene, self_collider, sender.clone());
                    }
                }
            }

            self.restoration_time -= context.time.delta;

            let attack_animation = context.scene.animations.get_mut(current_attack_animation);
            let attack_animation_ended = attack_animation.has_ended();

            if in_close_combat
                && self.attack_timeout <= 0.0
                && (attack_animation_ended || !attack_animation.is_enabled())
            {
                attack_animation.set_enabled(true).rewind();
                self.attack_animation_index = rg3d::core::rand::thread_rng()
                    .gen_range(0..self.upper_body_machine.attack_animations.len())
                    as u32;

                context
                    .scene
                    .animations
                    .get_mut(
                        self.upper_body_machine.attack_animations
                            [self.attack_animation_index as usize],
                    )
                    .set_enabled(true)
                    .rewind();
            }

            if self.attack_timeout < 0.0 && attack_animation_ended {
                self.attack_timeout = 0.3;
            }
            self.attack_timeout -= context.time.delta;

            // Aim overrides result of machines for spine bone.
            if self.target.is_some() {
                if let Some(look_dir) = look_dir.try_normalize(std::f32::EPSILON) {
                    self.aim_vertically(look_dir, &mut context.scene.graph, context.time);
                    self.aim_horizontally(look_dir, &mut context.scene.physics, context.time);
                }
            }
        }

        self.lower_body_machine
            .set_walk_animation_speed(context.scene, movement_speed_factor);
        self.lower_body_machine.apply(
            context.scene,
            context.time.delta,
            LowerBodyMachineInput {
                walk: is_moving,
                scream: false,
                dead: self.is_dead(),
            },
        );
        self.upper_body_machine.apply(
            context.scene,
            context.time,
            UpperBodyMachineInput {
                attack: in_close_combat && self.attack_timeout <= 0.0,
                walk: is_moving,
                scream: false,
                dead: self.is_dead(),
                aim: self.definition.can_use_weapons && can_aim,
                attack_animation_index: self.attack_animation_index,
            },
        );
        self.impact_handler
            .update_and_apply(context.time.delta, context.scene);
    }

    pub fn clean_up(&mut self, scene: &mut Scene) {
        self.upper_body_machine.clean_up(scene);
        self.lower_body_machine.clean_up(scene);
        self.character.clean_up(scene);
    }

    pub fn on_actor_removed(&mut self, handle: Handle<Actor>) {
        if let Some(target) = self.target.as_ref() {
            if target.handle == handle {
                self.target = None;
            }
        }
    }
}

fn clean_machine(machine: &Machine, scene: &mut Scene) {
    for node in machine.nodes() {
        if let PoseNode::PlayAnimation(node) = node {
            scene.animations.remove(node.animation);
        }
    }
}

impl Visit for Bot {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        let mut kind_id = self.kind.id();
        kind_id.visit("Kind", visitor)?;
        if visitor.is_reading() {
            self.kind = BotKind::from_id(kind_id)?;
        }

        self.definition = Self::get_definition(self.kind);
        self.character.visit("Character", visitor)?;
        self.model.visit("Model", visitor)?;
        self.target.visit("Target", visitor)?;
        self.lower_body_machine
            .visit("LocomotionMachine", visitor)?;
        self.upper_body_machine.visit("AimMachine", visitor)?;
        self.restoration_time.visit("RestorationTime", visitor)?;
        self.yaw.visit("Yaw", visitor)?;
        self.pitch.visit("Pitch", visitor)?;
        self.hips.visit("Hips", visitor)?;
        self.attack_animation_index
            .visit("AttackAnimationIndex", visitor)?;
        self.agent.visit("Agent", visitor)?;

        visitor.leave_region()
    }
}
