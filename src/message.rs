//! Game uses message passing mechanism to perform specific actions. For example to spawn
//! a bot or item everything you need is to send appropriate message and level will create
//! required entity. This is very effective decoupling mechanism that works perfectly with
//! strict ownership rules of Rust.
//!
//! Each message can be handle in multiple "systems", for example when bot dies, leader board
//! detects it and counts deaths of bot and adds one frag to a killer (if any). This way leader
//! board know nothing about bots, it just knows the fact that bot died. In other way bot knows
//! nothing about leader board - its can just die. Not sure if this mechanism is suitable for
//! all kinds of games, but at least it very useful for first-person shooters.

use crate::{
    actor::Actor,
    bot::BotKind,
    effects::EffectKind,
    item::{Item, ItemKind},
    sound::SoundKind,
    weapon::{
        projectile::{Damage, ProjectileKind, ProjectileOwner},
        Weapon, WeaponKind,
    },
};
use rg3d::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        pool::Handle,
    },
    physics::parry::shape::FeatureId,
    scene::ColliderHandle,
};
use std::path::PathBuf;

#[derive(Debug)]
pub enum Message {
    GiveNewWeapon {
        actor: Handle<Actor>,
        kind: WeaponKind,
    },
    AddBot {
        kind: BotKind,
        position: Vector3<f32>,
        rotation: UnitQuaternion<f32>,
    },
    RemoveActor {
        actor: Handle<Actor>,
    },
    SpawnBot {
        spawn_point_id: usize,
    },
    /// Gives item of specified kind to a given actor. Basically it means that actor will take
    /// item and consume it immediately (heal itself, add ammo, etc.)
    UseItem {
        actor: Handle<Actor>,
        kind: ItemKind,
    },
    /// Gives specified actor to a given actor. Removes item from level if temporary or deactivates
    /// it for short period of time if it constant.
    PickUpItem {
        actor: Handle<Actor>,
        item: Handle<Item>,
    },
    SpawnItem {
        kind: ItemKind,
        position: Vector3<f32>,
        adjust_height: bool,
    },
    CreateProjectile {
        kind: ProjectileKind,
        position: Vector3<f32>,
        direction: Vector3<f32>,
        initial_velocity: Vector3<f32>,
        owner: ProjectileOwner,
    },
    ShootWeapon {
        weapon: Handle<Weapon>,
        direction: Option<Vector3<f32>>,
    },
    ShootRay {
        weapon: Handle<Weapon>,
        begin: Vector3<f32>,
        end: Vector3<f32>,
        damage: Damage,
    },
    PlaySound {
        path: PathBuf,
        position: Vector3<f32>,
        gain: f32,
        rolloff_factor: f32,
        radius: f32,
    },
    Play2DSound {
        path: PathBuf,
        gain: f32,
    },
    /// Plays environment-specific sound. It also handles foot step sounds.
    PlayEnvironmentSound {
        collider: ColliderHandle,
        feature: FeatureId,
        position: Vector3<f32>,
        sound_kind: SoundKind,
        gain: f32,
        rolloff_factor: f32,
        radius: f32,
    },
    ShowWeapon {
        weapon: Handle<Weapon>,
        state: bool,
    },
    /// Forces actor to use a weapon of given kind.
    GrabWeapon {
        kind: WeaponKind,
        actor: Handle<Actor>,
    },
    SwitchFlashLight {
        weapon: Handle<Weapon>,
    },
    DamageActor {
        /// Which actor should be damaged.
        actor: Handle<Actor>,
        /// Actor who damaged target actor, can be Handle::NONE if damage came from environment
        /// or not from any actor.
        who: Handle<Actor>,
        /// Numeric value of damage.
        amount: f32,
    },
    CreateEffect {
        kind: EffectKind,
        position: Vector3<f32>,
        orientation: UnitQuaternion<f32>,
    },
    ApplySplashDamage {
        amount: f32,
        radius: f32,
        center: Vector3<f32>,
        /// Damage initiator
        who: Handle<Actor>,
    },
    /// Save game state to a file. TODO: Add filename field.
    SaveGame,
    /// Loads game state from a file. TODO: Add filename field.
    LoadGame,
    StartNewGame,
    QuitGame,
    ToggleMainMenu,
    SetMusicVolume {
        volume: f32,
    },
    EndMatch,
    SyncInventory,
    ShowItemDisplay {
        item: ItemKind,
        count: u32,
    },
    DropItems {
        actor: Handle<Actor>,
        item: ItemKind,
        count: u32,
    },
}
