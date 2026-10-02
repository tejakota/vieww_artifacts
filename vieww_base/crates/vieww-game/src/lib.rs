//! The game layer — the engine architecture rows of the comparison
//! document, as a library.
//!
//! | engine layer | here |
//! |---|---|
//! | Unity L1 GameObject/Component, Unreal L1 Actor/Component, Godot L1 "everything is a node" | [`World`]: entities with typed components |
//! | transform hierarchy (parent/child, world transforms) | [`Transform2`], [`World::set_parent`], [`World::propagate`] |
//! | Unity L6 script lifecycle — `Awake`, `Start`, `Update`, `FixedUpdate`, `LateUpdate`, `OnDestroy` | [`Behaviour`] on [`Game::frame`]'s fixed-step loop |
//! | coroutines (`yield return new WaitForSeconds`) | [`Coroutine`] |
//! | Godot signals / Unity events | [`Game::emit`], [`Game::connect`] |
//! | input system (action maps) | [`InputMap`] |
//! | Unity L2 scene management, `DontDestroyOnLoad`; Godot `PackedScene` instancing | [`SceneDesc`], [`Game::load_scene`], [`Game::instantiate`] |
//! | Godot L7 / Unity L7 resources and serialisation | scenes as JSON through registered component codecs |
//! | timers | [`Game::after`] |
//!
//! Physics, rendering and audio live in their own crates; a behaviour
//! reaches them through whatever it captured, the way a Unity script holds
//! a `Rigidbody` reference.

use std::any::{Any, TypeId};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fmt;

use vieww_foundation::json::Json;
use vieww_foundation::{Offset, Transform};

/// An entity: an index and a generation, so a stale handle to a despawned
/// entity never aliases a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity {
    pub index: u32,
    pub generation: u32,
}

trait Storage: Any {
    fn remove(&mut self, e: Entity);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: 'static> Storage for BTreeMap<Entity, T> {
    fn remove(&mut self, e: Entity) {
        BTreeMap::remove(self, &e);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Position, rotation (radians) and scale, relative to the parent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2 {
    pub position: Offset,
    pub rotation: f32,
    pub scale: f32,
}

impl Default for Transform2 {
    fn default() -> Self {
        Self {
            position: Offset::ZERO,
            rotation: 0.0,
            scale: 1.0,
        }
    }
}

impl Transform2 {
    #[must_use]
    pub const fn at(position: Offset) -> Self {
        Self {
            position,
            rotation: 0.0,
            scale: 1.0,
        }
    }

    #[must_use]
    pub fn matrix(&self) -> Transform {
        Transform::scale(self.scale, self.scale)
            .then(Transform::rotate(self.rotation))
            .then(Transform::translate(self.position))
    }
}

/// The computed world transform (written by [`World::propagate`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalTransform(pub Transform);

/// A display name (Unity's `name`, Godot's node name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name(pub String);

/// Entities and their components.
#[derive(Default)]
pub struct World {
    generations: Vec<u32>,
    alive: Vec<bool>,
    free: Vec<u32>,
    storages: HashMap<TypeId, Box<dyn Storage>>,
    parent: BTreeMap<Entity, Entity>,
    children: BTreeMap<Entity, Vec<Entity>>,
}

impl fmt::Debug for World {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("World")
            .field("entities", &self.len())
            .field("component_types", &self.storages.len())
            .finish()
    }
}

impl World {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(&mut self) -> Entity {
        if let Some(i) = self.free.pop() {
            let i_us = i as usize;
            self.alive[i_us] = true;
            return Entity {
                index: i,
                generation: self.generations[i_us],
            };
        }
        #[allow(clippy::cast_possible_truncation)]
        let index = self.generations.len() as u32;
        self.generations.push(0);
        self.alive.push(true);
        Entity {
            index,
            generation: 0,
        }
    }

    #[must_use]
    pub fn is_alive(&self, e: Entity) -> bool {
        let i = e.index as usize;
        i < self.alive.len() && self.alive[i] && self.generations[i] == e.generation
    }

    /// Live entity count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.alive.iter().filter(|a| **a).count()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn storage<T: 'static>(&self) -> Option<&BTreeMap<Entity, T>> {
        self.storages
            .get(&TypeId::of::<T>())
            .and_then(|s| s.as_any().downcast_ref())
    }

    fn storage_mut<T: 'static>(&mut self) -> &mut BTreeMap<Entity, T> {
        self.storages
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(BTreeMap::<Entity, T>::new()))
            .as_any_mut()
            .downcast_mut()
            .expect("storage type matches its TypeId")
    }

    /// Attach (or replace) a component.
    pub fn insert<T: 'static>(&mut self, e: Entity, c: T) {
        if self.is_alive(e) {
            self.storage_mut::<T>().insert(e, c);
        }
    }

    #[must_use]
    pub fn get<T: 'static>(&self, e: Entity) -> Option<&T> {
        self.storage::<T>()?.get(&e)
    }

    pub fn get_mut<T: 'static>(&mut self, e: Entity) -> Option<&mut T> {
        self.storage_mut::<T>().get_mut(&e)
    }

    pub fn remove<T: 'static>(&mut self, e: Entity) -> Option<T> {
        self.storage_mut::<T>().remove(&e)
    }

    #[must_use]
    pub fn has<T: 'static>(&self, e: Entity) -> bool {
        self.get::<T>(e).is_some()
    }

    /// Every entity with a `T`, in id order.
    #[must_use]
    pub fn query<T: 'static>(&self) -> Vec<Entity> {
        self.storage::<T>()
            .map(|s| s.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Every entity with both an `A` and a `B`.
    #[must_use]
    pub fn query2<A: 'static, B: 'static>(&self) -> Vec<Entity> {
        self.query::<A>()
            .into_iter()
            .filter(|e| self.has::<B>(*e))
            .collect()
    }

    /// The first entity named `name`.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<Entity> {
        self.query::<Name>()
            .into_iter()
            .find(|e| self.get::<Name>(*e).is_some_and(|n| n.0 == name))
    }

    /// Parent `child` under `parent` (or detach with `None`).
    pub fn set_parent(&mut self, child: Entity, parent: Option<Entity>) {
        if let Some(old) = self.parent.remove(&child) {
            if let Some(v) = self.children.get_mut(&old) {
                v.retain(|c| *c != child);
            }
        }
        if let Some(p) = parent {
            self.parent.insert(child, p);
            self.children.entry(p).or_default().push(child);
        }
    }

    #[must_use]
    pub fn parent_of(&self, e: Entity) -> Option<Entity> {
        self.parent.get(&e).copied()
    }

    #[must_use]
    pub fn children_of(&self, e: Entity) -> Vec<Entity> {
        self.children.get(&e).cloned().unwrap_or_default()
    }

    /// Despawn an entity and its descendants; returns everything removed.
    pub fn despawn(&mut self, e: Entity) -> Vec<Entity> {
        if !self.is_alive(e) {
            return Vec::new();
        }
        let mut gone = Vec::new();
        let mut stack = vec![e];
        while let Some(x) = stack.pop() {
            stack.extend(self.children_of(x));
            gone.push(x);
        }
        self.set_parent(e, None);
        for x in &gone {
            for s in self.storages.values_mut() {
                s.remove(*x);
            }
            self.children.remove(x);
            self.parent.remove(x);
            let i = x.index as usize;
            self.alive[i] = false;
            self.generations[i] += 1;
            self.free.push(x.index);
        }
        gone
    }

    /// Compute every [`GlobalTransform`] from the [`Transform2`] hierarchy.
    pub fn propagate(&mut self) {
        let roots: Vec<Entity> = self
            .query::<Transform2>()
            .into_iter()
            .filter(|e| !self.parent.contains_key(e))
            .collect();
        let mut stack: Vec<(Entity, Transform)> = roots
            .into_iter()
            .map(|r| (r, Transform::IDENTITY))
            .collect();
        while let Some((e, parent)) = stack.pop() {
            let local = self
                .get::<Transform2>(e)
                .map_or(Transform::IDENTITY, Transform2::matrix);
            let world = local.then(parent);
            self.insert(e, GlobalTransform(world));
            for c in self.children_of(e) {
                stack.push((c, world));
            }
        }
    }
}

/// Deferred world edits a behaviour can request mid-frame.
#[derive(Debug, Default)]
pub struct Commands {
    despawn: Vec<Entity>,
    emits: Vec<(String, Entity)>,
    load: Option<String>,
    instantiate: Vec<(String, Offset)>,
}

impl Commands {
    pub fn despawn(&mut self, e: Entity) {
        self.despawn.push(e);
    }
    /// Emit a signal (delivered after the phase finishes).
    pub fn emit(&mut self, signal: &str, from: Entity) {
        self.emits.push((signal.to_owned(), from));
    }
    /// Switch scenes at the end of the frame.
    pub fn load_scene(&mut self, name: &str) {
        self.load = Some(name.to_owned());
    }
    /// Instantiate a registered prefab at a position.
    pub fn instantiate(&mut self, prefab: &str, at: Offset) {
        self.instantiate.push((prefab.to_owned(), at));
    }
}

/// What a behaviour sees each call.
#[derive(Debug)]
pub struct Ctx<'a> {
    pub world: &'a mut World,
    pub entity: Entity,
    pub input: &'a Input,
    pub commands: &'a mut Commands,
    pub time: f32,
    pub frame: u64,
}

/// A script on an entity — `MonoBehaviour`, a Godot script.
pub trait Behaviour {
    fn awake(&mut self, _ctx: &mut Ctx<'_>) {}
    fn start(&mut self, _ctx: &mut Ctx<'_>) {}
    fn update(&mut self, _ctx: &mut Ctx<'_>, _dt: f32) {}
    fn fixed_update(&mut self, _ctx: &mut Ctx<'_>, _dt: f32) {}
    fn late_update(&mut self, _ctx: &mut Ctx<'_>, _dt: f32) {}
    fn on_destroy(&mut self, _ctx: &mut Ctx<'_>) {}
    /// Handle a signal this entity connected to.
    fn on_signal(&mut self, _ctx: &mut Ctx<'_>, _signal: &str, _from: Entity) {}
}

/// A coroutine's action step.
pub type Action = Box<dyn FnMut(&mut World, &mut Commands)>;

/// One step of a coroutine.
pub enum Step {
    WaitSeconds(f32),
    WaitFrames(u32),
    WaitUntil(Box<dyn Fn(&World) -> bool>),
    Do(Action),
}

impl fmt::Debug for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WaitSeconds(s) => write!(f, "WaitSeconds({s})"),
            Self::WaitFrames(n) => write!(f, "WaitFrames({n})"),
            Self::WaitUntil(_) => f.write_str("WaitUntil(..)"),
            Self::Do(_) => f.write_str("Do(..)"),
        }
    }
}

/// A sequence of steps run across frames — Unity's `IEnumerator`
/// coroutine, Godot's `await`.
#[derive(Debug, Default)]
pub struct Coroutine {
    steps: VecDeque<Step>,
    waited: f32,
    frames: u32,
    /// Stop when this entity dies (a coroutine belongs to its owner).
    owner: Option<Entity>,
}

impl Coroutine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub fn wait(mut self, seconds: f32) -> Self {
        self.steps.push_back(Step::WaitSeconds(seconds));
        self
    }
    #[must_use]
    pub fn wait_frames(mut self, n: u32) -> Self {
        self.steps.push_back(Step::WaitFrames(n));
        self
    }
    #[must_use]
    pub fn wait_until(mut self, f: impl Fn(&World) -> bool + 'static) -> Self {
        self.steps.push_back(Step::WaitUntil(Box::new(f)));
        self
    }
    #[must_use]
    pub fn then(mut self, f: impl FnMut(&mut World, &mut Commands) + 'static) -> Self {
        self.steps.push_back(Step::Do(Box::new(f)));
        self
    }
    #[must_use]
    pub const fn owned_by(mut self, e: Entity) -> Self {
        self.owner = Some(e);
        self
    }

    /// Advance; `true` while unfinished.
    fn tick(&mut self, world: &mut World, commands: &mut Commands, dt: f32) -> bool {
        if self.owner.is_some_and(|o| !world.is_alive(o)) {
            return false;
        }
        let mut budget = dt;
        while let Some(step) = self.steps.front_mut() {
            match step {
                Step::WaitSeconds(s) => {
                    let need = *s - self.waited;
                    if budget < need {
                        self.waited += budget;
                        return true;
                    }
                    budget -= need;
                    self.waited = 0.0;
                    self.steps.pop_front();
                }
                Step::WaitFrames(n) => {
                    if self.frames < *n {
                        self.frames += 1;
                        return true;
                    }
                    self.frames = 0;
                    self.steps.pop_front();
                }
                Step::WaitUntil(f) => {
                    if !f(world) {
                        return true;
                    }
                    self.steps.pop_front();
                }
                Step::Do(f) => {
                    f(world, commands);
                    self.steps.pop_front();
                }
            }
        }
        false
    }
}

/// Named actions bound to keys — Unity's Input System action maps,
/// Godot's `InputMap`.
#[derive(Debug, Clone, Default)]
pub struct InputMap {
    bindings: BTreeMap<String, Vec<String>>,
}

impl InputMap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn bind(mut self, action: &str, keys: &[&str]) -> Self {
        self.bindings.insert(
            action.to_owned(),
            keys.iter().map(|k| (*k).to_owned()).collect(),
        );
        self
    }
}

/// Keys held now and last frame.
#[derive(Debug, Clone, Default)]
pub struct Input {
    map: InputMap,
    held: BTreeSet<String>,
    previous: BTreeSet<String>,
}

impl Input {
    #[must_use]
    pub fn new(map: InputMap) -> Self {
        Self {
            map,
            ..Self::default()
        }
    }
    pub fn press(&mut self, key: &str) {
        self.held.insert(key.to_owned());
    }
    pub fn release(&mut self, key: &str) {
        self.held.remove(key);
    }
    fn any(&self, action: &str, set: &BTreeSet<String>) -> bool {
        self.map
            .bindings
            .get(action)
            .is_some_and(|keys| keys.iter().any(|k| set.contains(k)))
    }
    #[must_use]
    pub fn is_action_pressed(&self, action: &str) -> bool {
        self.any(action, &self.held)
    }
    #[must_use]
    pub fn is_action_just_pressed(&self, action: &str) -> bool {
        self.any(action, &self.held) && !self.any(action, &self.previous)
    }
    #[must_use]
    pub fn is_action_just_released(&self, action: &str) -> bool {
        !self.any(action, &self.held) && self.any(action, &self.previous)
    }
    /// −1..1 from a negative and a positive action (Godot's
    /// `get_axis`).
    #[must_use]
    pub fn axis(&self, negative: &str, positive: &str) -> f32 {
        f32::from(u8::from(self.is_action_pressed(positive)))
            - f32::from(u8::from(self.is_action_pressed(negative)))
    }
    fn end_frame(&mut self) {
        self.previous = self.held.clone();
    }
}

/// A codec that saves and loads one component type in scenes.
pub struct Codec {
    pub name: &'static str,
    pub save: fn(&World, Entity) -> Option<Json>,
    pub load: fn(&mut World, Entity, &Json),
}

impl fmt::Debug for Codec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Codec")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The built-in codecs: `Name` and `Transform2`.
#[must_use]
pub fn builtin_codecs() -> Vec<Codec> {
    vec![
        Codec {
            name: "name",
            save: |w, e| w.get::<Name>(e).map(|n| Json::from(n.0.as_str())),
            load: |w, e, j| {
                if let Some(s) = j.as_str() {
                    w.insert(e, Name(s.to_owned()));
                }
            },
        },
        Codec {
            name: "transform",
            save: |w, e| {
                w.get::<Transform2>(e).map(|t| {
                    Json::numbers([
                        f64::from(t.position.dx),
                        f64::from(t.position.dy),
                        f64::from(t.rotation),
                        f64::from(t.scale),
                    ])
                })
            },
            load: |w, e, j| {
                if let Some(v) = j.as_f32_vec() {
                    if v.len() == 4 {
                        w.insert(
                            e,
                            Transform2 {
                                position: Offset::new(v[0], v[1]),
                                rotation: v[2],
                                scale: v[3],
                            },
                        );
                    }
                }
            },
        },
    ]
}

/// A serialised entity tree — a Unity scene, a Godot `PackedScene`.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneDesc(pub Json);

impl SceneDesc {
    /// Save `roots` (and their descendants) with `codecs`.
    #[must_use]
    pub fn save(world: &World, roots: &[Entity], codecs: &[Codec]) -> Self {
        fn one(world: &World, e: Entity, codecs: &[Codec]) -> Json {
            let mut fields: Vec<(String, Json)> = codecs
                .iter()
                .filter_map(|c| (c.save)(world, e).map(|j| (c.name.to_owned(), j)))
                .collect();
            let kids: Vec<Json> = world
                .children_of(e)
                .into_iter()
                .map(|c| one(world, c, codecs))
                .collect();
            if !kids.is_empty() {
                fields.push(("children".into(), Json::Array(kids)));
            }
            Json::Object(fields)
        }
        Self(Json::Array(
            roots.iter().map(|r| one(world, *r, codecs)).collect(),
        ))
    }

    /// Instantiate into `world`; returns the new roots.
    pub fn instantiate(&self, world: &mut World, codecs: &[Codec]) -> Vec<Entity> {
        fn one(world: &mut World, j: &Json, parent: Option<Entity>, codecs: &[Codec]) -> Entity {
            let e = world.spawn();
            for c in codecs {
                if let Some(v) = j.get(c.name) {
                    (c.load)(world, e, v);
                }
            }
            world.set_parent(e, parent);
            for k in j.get("children").and_then(Json::as_array).unwrap_or(&[]) {
                one(world, k, Some(e), codecs);
            }
            e
        }
        self.0
            .as_array()
            .unwrap_or(&[])
            .iter()
            .map(|j| one(world, j, None, codecs))
            .collect()
    }
}

/// Counters for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameStats {
    pub updates: usize,
    pub fixed_steps: usize,
    pub coroutines: usize,
    pub signals: usize,
    pub despawned: usize,
}

type SignalHandler = (Entity, String);
type Spawner = Box<dyn Fn(&mut World, Offset) -> (Entity, Option<Box<dyn Behaviour>>)>;

/// The loop that owns a world. See the [crate docs](crate).
pub struct Game {
    pub world: World,
    pub input: Input,
    pub fixed_dt: f32,
    behaviours: BTreeMap<Entity, Box<dyn Behaviour>>,
    started: BTreeSet<Entity>,
    coroutines: Vec<Coroutine>,
    connections: Vec<(String, SignalHandler)>,
    scenes: BTreeMap<String, SceneDesc>,
    prefabs: BTreeMap<String, Spawner>,
    persistent: BTreeSet<Entity>,
    pub codecs: Vec<Codec>,
    accumulator: f32,
    pub time: f32,
    pub frame: u64,
    pub current_scene: Option<String>,
    /// Signals delivered, in order: `(signal, from, to)`.
    pub signal_log: Vec<(String, Entity, Entity)>,
}

impl fmt::Debug for Game {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Game")
            .field("world", &self.world)
            .field("behaviours", &self.behaviours.len())
            .field("coroutines", &self.coroutines.len())
            .field("frame", &self.frame)
            .finish_non_exhaustive()
    }
}

impl Game {
    #[must_use]
    pub fn new(input: InputMap) -> Self {
        Self {
            world: World::new(),
            input: Input::new(input),
            fixed_dt: 1.0 / 60.0,
            behaviours: BTreeMap::new(),
            started: BTreeSet::new(),
            coroutines: Vec::new(),
            connections: Vec::new(),
            scenes: BTreeMap::new(),
            prefabs: BTreeMap::new(),
            persistent: BTreeSet::new(),
            codecs: builtin_codecs(),
            accumulator: 0.0,
            time: 0.0,
            frame: 0,
            current_scene: None,
            signal_log: Vec::new(),
        }
    }

    /// Attach a behaviour; `awake` runs now, `start` before its first
    /// update.
    pub fn attach(&mut self, e: Entity, mut b: impl Behaviour + 'static) {
        let mut commands = Commands::default();
        {
            let mut ctx = Ctx {
                world: &mut self.world,
                entity: e,
                input: &self.input,
                commands: &mut commands,
                time: self.time,
                frame: self.frame,
            };
            b.awake(&mut ctx);
        }
        self.behaviours.insert(e, Box::new(b));
        self.apply(commands);
    }

    fn attach_boxed(&mut self, e: Entity, mut b: Box<dyn Behaviour>) {
        let mut commands = Commands::default();
        {
            let mut ctx = Ctx {
                world: &mut self.world,
                entity: e,
                input: &self.input,
                commands: &mut commands,
                time: self.time,
                frame: self.frame,
            };
            b.awake(&mut ctx);
        }
        self.behaviours.insert(e, b);
        self.apply(commands);
    }

    /// Start a coroutine.
    pub fn start_coroutine(&mut self, c: Coroutine) {
        self.coroutines.push(c);
    }

    /// Run `f` after `seconds` (a one-shot timer).
    pub fn after(&mut self, seconds: f32, f: impl FnMut(&mut World, &mut Commands) + 'static) {
        self.coroutines.push(Coroutine::new().wait(seconds).then(f));
    }

    /// Deliver `signal` emitted by any entity to `listener`'s behaviour.
    pub fn connect(&mut self, signal: &str, listener: Entity) {
        self.connections
            .push((signal.to_owned(), (listener, signal.to_owned())));
    }

    /// Emit a signal now.
    pub fn emit(&mut self, signal: &str, from: Entity) {
        let mut c = Commands::default();
        c.emit(signal, from);
        self.apply(c);
    }

    /// Register a scene by name.
    pub fn register_scene(&mut self, name: &str, desc: SceneDesc) {
        self.scenes.insert(name.to_owned(), desc);
    }

    /// Register a prefab: a function that builds an entity (and optionally
    /// its behaviour) at a position.
    pub fn register_prefab(
        &mut self,
        name: &str,
        f: impl Fn(&mut World, Offset) -> (Entity, Option<Box<dyn Behaviour>>) + 'static,
    ) {
        self.prefabs.insert(name.to_owned(), Box::new(f));
    }

    /// Instantiate a prefab now.
    pub fn instantiate(&mut self, prefab: &str, at: Offset) -> Option<Entity> {
        let spawn = self.prefabs.get(prefab)?;
        let (e, b) = spawn(&mut self.world, at);
        if let Some(b) = b {
            self.attach_boxed(e, b);
        }
        Some(e)
    }

    /// Keep `e` (and its subtree) across scene loads — `DontDestroyOnLoad`.
    pub fn dont_destroy_on_load(&mut self, e: Entity) {
        self.persistent.insert(e);
    }

    /// Unload everything not persistent and instantiate scene `name`.
    pub fn load_scene(&mut self, name: &str) -> Vec<Entity> {
        let Some(desc) = self.scenes.get(name).cloned() else {
            return Vec::new();
        };
        let doomed: Vec<Entity> = (0..self.world.generations.len())
            .filter(|&i| self.world.alive[i])
            .map(|i| {
                #[allow(clippy::cast_possible_truncation)]
                Entity {
                    index: i as u32,
                    generation: self.world.generations[i],
                }
            })
            .filter(|e| self.world.parent_of(*e).is_none() && !self.persistent.contains(e))
            .collect();
        for e in doomed {
            self.destroy(e);
        }
        self.current_scene = Some(name.to_owned());
        desc.instantiate(&mut self.world, &self.codecs)
    }

    fn destroy(&mut self, e: Entity) -> usize {
        let gone = self.world.children_of_recursive(e);
        let mut commands = Commands::default();
        for x in &gone {
            if let Some(mut b) = self.behaviours.remove(x) {
                let mut ctx = Ctx {
                    world: &mut self.world,
                    entity: *x,
                    input: &self.input,
                    commands: &mut commands,
                    time: self.time,
                    frame: self.frame,
                };
                b.on_destroy(&mut ctx);
            }
            self.started.remove(x);
        }
        self.world.despawn(e);
        gone.len()
    }

    fn apply(&mut self, mut c: Commands) -> (usize, usize) {
        let mut despawned = 0;
        let mut signals = 0;
        // Signals may emit more signals; bounded to avoid ping-pong loops.
        let mut rounds = 0;
        while !c.emits.is_empty() && rounds < 16 {
            rounds += 1;
            let emits = std::mem::take(&mut c.emits);
            for (sig, from) in emits {
                let targets: Vec<Entity> = self
                    .connections
                    .iter()
                    .filter(|(s, _)| *s == sig)
                    .map(|(_, (e, _))| *e)
                    .collect();
                for t in targets {
                    if let Some(mut b) = self.behaviours.remove(&t) {
                        let mut ctx = Ctx {
                            world: &mut self.world,
                            entity: t,
                            input: &self.input,
                            commands: &mut c,
                            time: self.time,
                            frame: self.frame,
                        };
                        b.on_signal(&mut ctx, &sig, from);
                        self.behaviours.insert(t, b);
                        self.signal_log.push((sig.clone(), from, t));
                        signals += 1;
                    }
                }
            }
        }
        for (prefab, at) in std::mem::take(&mut c.instantiate) {
            self.instantiate(&prefab, at);
        }
        for e in std::mem::take(&mut c.despawn) {
            if self.world.is_alive(e) {
                despawned += self.destroy(e);
            }
        }
        if let Some(scene) = c.load.take() {
            self.load_scene(&scene);
        }
        (despawned, signals)
    }

    fn each(
        &mut self,
        phase: impl Fn(&mut dyn Behaviour, &mut Ctx<'_>),
        commands: &mut Commands,
    ) -> usize {
        let ids: Vec<Entity> = self.behaviours.keys().copied().collect();
        let mut n = 0;
        for e in ids {
            if !self.world.is_alive(e) {
                continue;
            }
            if let Some(mut b) = self.behaviours.remove(&e) {
                let mut ctx = Ctx {
                    world: &mut self.world,
                    entity: e,
                    input: &self.input,
                    commands,
                    time: self.time,
                    frame: self.frame,
                };
                phase(b.as_mut(), &mut ctx);
                self.behaviours.insert(e, b);
                n += 1;
            }
        }
        n
    }

    /// One frame of `dt` seconds: Start (new behaviours), FixedUpdate × N,
    /// Update, coroutines, LateUpdate, transform propagation, then deferred
    /// commands (signals, spawns, despawns, scene loads).
    pub fn frame(&mut self, dt: f32) -> FrameStats {
        let mut stats = FrameStats::default();
        let mut commands = Commands::default();
        let fresh: Vec<Entity> = self
            .behaviours
            .keys()
            .copied()
            .filter(|e| !self.started.contains(e))
            .collect();
        for e in fresh {
            self.started.insert(e);
            if let Some(mut b) = self.behaviours.remove(&e) {
                let mut ctx = Ctx {
                    world: &mut self.world,
                    entity: e,
                    input: &self.input,
                    commands: &mut commands,
                    time: self.time,
                    frame: self.frame,
                };
                b.start(&mut ctx);
                self.behaviours.insert(e, b);
            }
        }
        self.accumulator += dt;
        let fixed = self.fixed_dt;
        while self.accumulator >= fixed - 1e-7 {
            self.accumulator -= fixed;
            self.each(|b, c| b.fixed_update(c, fixed), &mut commands);
            stats.fixed_steps += 1;
        }
        self.time += dt;
        stats.updates = self.each(|b, c| b.update(c, dt), &mut commands);
        let mut running = Vec::new();
        for mut co in std::mem::take(&mut self.coroutines) {
            if co.tick(&mut self.world, &mut commands, dt) {
                running.push(co);
            }
        }
        running.extend(std::mem::take(&mut self.coroutines));
        self.coroutines = running;
        stats.coroutines = self.coroutines.len();
        self.each(|b, c| b.late_update(c, dt), &mut commands);
        self.world.propagate();
        let (d, s) = self.apply(commands);
        stats.despawned = d;
        stats.signals = s;
        self.input.end_frame();
        self.frame += 1;
        stats
    }
}

impl World {
    /// `e` and all its descendants.
    #[must_use]
    pub fn children_of_recursive(&self, e: Entity) -> Vec<Entity> {
        let mut out = Vec::new();
        let mut stack = vec![e];
        while let Some(x) = stack.pop() {
            out.push(x);
            stack.extend(self.children_of(x));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Velocity(Offset);

    #[test]
    fn components_query_and_generations() {
        let mut w = World::new();
        let a = w.spawn();
        let b = w.spawn();
        w.insert(a, Velocity(Offset::new(1.0, 0.0)));
        w.insert(a, Name("a".into()));
        w.insert(b, Name("b".into()));
        assert_eq!(w.query2::<Name, Velocity>(), vec![a]);
        assert_eq!(w.find("b"), Some(b));
        w.despawn(a);
        let c = w.spawn();
        assert_eq!(c.index, a.index);
        assert_ne!(c, a, "a reused slot has a new generation");
        assert!(!w.is_alive(a));
        assert!(
            w.get::<Name>(c).is_none(),
            "components did not leak into the reused slot"
        );
    }

    #[test]
    fn transforms_propagate_down_the_hierarchy() {
        let mut w = World::new();
        let parent = w.spawn();
        let child = w.spawn();
        w.insert(
            parent,
            Transform2 {
                position: Offset::new(100.0, 0.0),
                rotation: std::f32::consts::FRAC_PI_2,
                scale: 2.0,
            },
        );
        w.insert(child, Transform2::at(Offset::new(10.0, 0.0)));
        w.set_parent(child, Some(parent));
        w.propagate();
        let p = w
            .get::<GlobalTransform>(child)
            .unwrap()
            .0
            .apply(Offset::ZERO);
        assert!(
            (p.dx - 100.0).abs() < 1e-3 && (p.dy - 20.0).abs() < 1e-3,
            "{p:?}"
        );
        assert_eq!(w.despawn(parent).len(), 2, "children go with the parent");
    }

    struct Recorder {
        log: Rc<RefCell<Vec<String>>>,
    }

    impl Behaviour for Recorder {
        fn awake(&mut self, _c: &mut Ctx<'_>) {
            self.log.borrow_mut().push("awake".into());
        }
        fn start(&mut self, _c: &mut Ctx<'_>) {
            self.log.borrow_mut().push("start".into());
        }
        fn update(&mut self, _c: &mut Ctx<'_>, _dt: f32) {
            self.log.borrow_mut().push("update".into());
        }
        fn fixed_update(&mut self, _c: &mut Ctx<'_>, _dt: f32) {
            self.log.borrow_mut().push("fixed".into());
        }
        fn late_update(&mut self, _c: &mut Ctx<'_>, _dt: f32) {
            self.log.borrow_mut().push("late".into());
        }
        fn on_destroy(&mut self, _c: &mut Ctx<'_>) {
            self.log.borrow_mut().push("destroy".into());
        }
        fn on_signal(&mut self, c: &mut Ctx<'_>, s: &str, _from: Entity) {
            self.log.borrow_mut().push(format!("signal {s}"));
            if s == "hit" {
                c.commands.despawn(c.entity);
            }
        }
    }

    #[test]
    fn lifecycle_runs_in_unity_order_with_fixed_steps() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut g = Game::new(InputMap::new());
        let e = g.world.spawn();
        g.attach(e, Recorder { log: log.clone() });
        let s = g.frame(1.0 / 30.0);
        assert_eq!(s.fixed_steps, 2, "two 60 Hz steps in a 30 Hz frame");
        assert_eq!(
            *log.borrow(),
            ["awake", "start", "fixed", "fixed", "update", "late"]
        );
        log.borrow_mut().clear();
        g.connect("hit", e);
        let other = g.world.spawn();
        g.emit("hit", other);
        assert_eq!(*log.borrow(), ["signal hit", "destroy"]);
        assert!(!g.world.is_alive(e));
    }

    #[test]
    fn coroutines_wait_for_time_frames_and_conditions() {
        let mut g = Game::new(InputMap::new());
        let flag = g.world.spawn();
        let log = Rc::new(RefCell::new(Vec::new()));
        let (l1, l2, l3) = (log.clone(), log.clone(), log.clone());
        g.start_coroutine(
            Coroutine::new()
                .then(move |_, _| l1.borrow_mut().push("a"))
                .wait(0.5)
                .then(move |_, _| l2.borrow_mut().push("b"))
                .wait_until(move |w| w.has::<Name>(flag))
                .then(move |_, _| l3.borrow_mut().push("c")),
        );
        for _ in 0..20 {
            g.frame(0.05);
        }
        assert_eq!(*log.borrow(), ["a", "b"], "waiting for the flag");
        g.world.insert(flag, Name("ready".into()));
        g.frame(0.05);
        assert_eq!(*log.borrow(), ["a", "b", "c"]);
        let fired = Rc::new(RefCell::new(0));
        let f = fired.clone();
        g.after(0.1, move |_, _| *f.borrow_mut() += 1);
        g.frame(0.05);
        assert_eq!(*fired.borrow(), 0);
        g.frame(0.06);
        assert_eq!(*fired.borrow(), 1);
    }

    #[test]
    fn input_actions_track_edges() {
        let mut g = Game::new(
            InputMap::new()
                .bind("jump", &["Space", "W"])
                .bind("left", &["A"])
                .bind("right", &["D"]),
        );
        g.input.press("W");
        assert!(g.input.is_action_just_pressed("jump"));
        g.frame(0.016);
        assert!(g.input.is_action_pressed("jump") && !g.input.is_action_just_pressed("jump"));
        g.input.release("W");
        assert!(g.input.is_action_just_released("jump"));
        g.input.press("A");
        assert_eq!(g.input.axis("left", "right"), -1.0);
    }

    #[test]
    fn scenes_save_load_and_keep_persistent_entities() {
        let mut g = Game::new(InputMap::new());
        let root = g.world.spawn();
        g.world.insert(root, Name("level".into()));
        g.world.insert(root, Transform2::at(Offset::new(5.0, 6.0)));
        let kid = g.world.spawn();
        g.world.insert(kid, Name("door".into()));
        g.world.set_parent(kid, Some(root));
        let desc = SceneDesc::save(&g.world, &[root], &g.codecs);
        let text = desc.0.pretty();
        let back = SceneDesc(Json::parse(&text).unwrap());
        g.register_scene("level1", back);
        let player = g.world.spawn();
        g.world.insert(player, Name("player".into()));
        g.dont_destroy_on_load(player);
        let roots = g.load_scene("level1");
        assert_eq!(roots.len(), 1);
        assert!(g.world.find("player").is_some(), "persistent survived");
        let level = g.world.find("level").unwrap();
        assert_eq!(
            g.world.get::<Transform2>(level).unwrap().position,
            Offset::new(5.0, 6.0)
        );
        assert_eq!(g.world.children_of(level).len(), 1);
        assert_eq!(
            g.world.len(),
            3,
            "old level gone, new level + door + player"
        );
    }

    #[test]
    fn prefabs_instantiate_with_behaviours_from_commands() {
        struct Spawner;
        impl Behaviour for Spawner {
            fn update(&mut self, c: &mut Ctx<'_>, _dt: f32) {
                if c.frame < 3 {
                    c.commands.instantiate("coin", Offset::new(10.0, 0.0));
                }
            }
        }
        let mut g = Game::new(InputMap::new());
        g.register_prefab("coin", |w, at| {
            let e = w.spawn();
            w.insert(e, Name("coin".into()));
            w.insert(e, Transform2::at(at));
            (e, None)
        });
        let s = g.world.spawn();
        g.attach(s, Spawner);
        for _ in 0..5 {
            g.frame(0.016);
        }
        assert_eq!(g.world.query::<Name>().len(), 3);
    }
}
