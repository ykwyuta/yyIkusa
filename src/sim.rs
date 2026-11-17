//! 戦闘のシミュレーション：準備（配置）と戦闘、射撃と装甲、視界、基地の占領、勝敗
//!
//! - 準備フェーズ：決まった予算で部隊を買って配置し、デコイと柵を置く。
//! - 戦闘フェーズ：デコイや柵は追加できない。偵察鷹（各陣営1機）が出る。
//! - 相手の基地を占領するか、相手の部隊をすべて撃破すれば勝ち。

use crate::data::{Nation, TroopSpec, Terrain, Weight, DECOY_COST, FENCE_COST, FENCE_HP};
use crate::generals::GENERALS;
use crate::map::{center, idx, in_zone, inside, tile_of, Field, Map, MH, MW, TS};
use crate::rng::Rng;

/// シミュレーションの1歩（秒）
pub const DT: f32 = 1.0 / 30.0;
/// 占領にかかる秒数（1隊のとき）
pub const CAPTURE_TIME: f32 = 60.0;
/// 占領範囲（マス）
pub const CAPTURE_R: f32 = 3.5;
/// 戦闘の制限時間（秒）
pub const TIME_LIMIT: f32 = 15.0 * 60.0;
/// 鷹の視界と速さ（マス、マス/秒）
pub const HAWK_SIGHT: f32 = 9.0;
pub const HAWK_SPEED: f32 = 4.0;
const HAWK_WAYPOINTS: [(f32, f32); 6] = [
    (0.55, 0.25), (0.70, 0.50), (0.55, 0.75),
    (0.85, 0.70), (0.85, 0.30), (0.45, 0.50),
];
/// 部隊への損害の倍率（戦闘が一瞬で終わらないように）
pub const DAMAGE_SCALE: f32 = 0.6;
/// デコイを見破る距離（マス）
pub const DECOY_REVEAL: f32 = 3.5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Troop,
    Decoy,
    Hawk,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Unit(usize),
    Fence(usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Order {
    Idle,
    /// 移動（attack なら敵を見つけたら止まって撃つ）
    Move { goal: (f32, f32), attack: bool },
    Attack(Target),
}

pub struct Unit {
    pub side: usize,
    pub kind: Kind,
    pub general: Option<usize>,
    patrol_leg: Option<usize>,
    pub nation: Nation,
    /// 部隊の種類（デコイは見せかけの種類）
    pub ti: usize,
    pub pos: (f32, f32),
    pub hull: f32,
    pub turret: f32,
    pub hp: f32,
    pub alive: bool,
    pub reload: f32,
    pub order: Order,
    pub path: Vec<(i32, i32)>,
    pub repath: f32,
    pub target: Option<Target>,
    pub scan: f32,
    pub moving: bool,
    /// 最後に撃ってからの秒数
    pub fired: f32,
    /// デコイが相手に見破られたか
    pub revealed: bool,
    /// 撃破されてからの秒数（残骸の煙）
    pub dead_t: f32,
    pub kills: u32,
    /// 進路をふさぐ敵の柵
    pub breach: Option<usize>,
}

impl Unit {
    pub(crate) fn armor_toward(&self, from: (f32, f32)) -> f32 {
        let a = (from.1 - self.pos.1).atan2(from.0 - self.pos.0);
        let rel = angle_diff(a, self.hull).abs();
        let spec = self.spec();
        if rel < 0.9 { spec.front }
        else if rel > 2.3 { spec.rear() }
        else { spec.side }
    }

    pub fn manually_controlled(&self) -> bool {
        self.alive && self.side == 0 && self.kind != Kind::Decoy && self.general.is_none()
    }

    pub fn spec(&self) -> &'static TroopSpec {
        &self.nation.troops()[self.ti]
    }
    pub fn tile(&self) -> (i32, i32) {
        tile_of(self.pos)
    }
}

pub struct Fence {
    pub tile: (i32, i32),
    pub side: usize,
    pub hp: f32,
    pub alive: bool,
    /// 各陣営が見つけたか
    pub seen: [bool; 2],
}

pub struct Shell {
    pub from: (f32, f32),
    pub to: (f32, f32),
    pub t: f32,
    pub dur: f32,
    pub side: usize,
    pub shooter: usize,
    pub hit: Option<(Target, f32, bool)>,
    pub big: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FxKind {
    Flash,
    Boom,
    Spark,
    Dust,
}

pub struct Fx {
    pub pos: (f32, f32),
    pub t: f32,
    pub life: f32,
    pub kind: FxKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Prep,
    Battle,
    Over,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EndReason {
    Annihilation,
    Capture,
    TimeUp,
}

/// 画面に出す出来事
pub struct Event {
    pub side: usize,
    pub text: String,
    pub pos: Option<(f32, f32)>,
}

pub struct World {
    pub map: Map,
    pub units: Vec<Unit>,
    pub fences: Vec<Fence>,
    /// マス → 柵の番号（無ければ usize::MAX）
    pub fences_at: Vec<usize>,
    pub shells: Vec<Shell>,
    pub fx: Vec<Fx>,
    pub phase: Phase,
    pub time: f32,
    pub budget: u32,
    pub spent: [u32; 2],
    /// 陣営ごとの見えているマス
    pub vis: [Vec<bool>; 2],
    /// 陣営ごとの見えている部隊（units と同じ並び）
    pub spotted: [Vec<bool>; 2],
    /// 各陣営の基地が占領されつつある度合い（0..1）
    pub capture: [f32; 2],
    pub winner: Option<usize>,
    pub draw: bool,
    pub reason: Option<EndReason>,
    pub rng: Rng,
    vis_t: f32,
    /// 命中率の補正（難易度）
    pub skill: [f32; 2],
    pub events: Vec<Event>,
    /// 連続して出さないための記録（占領の知らせ）
    cap_warned: [bool; 2],
    /// 指揮権を返還して副官になっている自軍将軍
    pub adjutants: [bool; GENERALS.len()],
}

fn angle_diff(a: f32, b: f32) -> f32 {
    let mut d = a - b;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

fn turn_toward(cur: f32, goal: f32, max: f32) -> f32 {
    let d = angle_diff(goal, cur);
    cur + d.clamp(-max, max)
}

pub fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn separation_position(unit: &Unit, mut push: (f32, f32)) -> (f32, f32) {
    if unit.moving {
        let heading = (unit.hull.cos(), unit.hull.sin());
        let backward = (push.0 * heading.0 + push.1 * heading.1).min(0.0);
        // 狭い道でも、押し合いが1フレームの前進を打ち消さないようにする。
        push.0 -= heading.0 * backward;
        push.1 -= heading.1 * backward;
    }
    (unit.pos.0 + push.0, unit.pos.1 + push.1)
}

impl World {
    pub fn new(field: Field, seed: u32, budget: u32) -> World {
        let map = Map::generate(field, seed);
        let n = (MW * MH) as usize;
        World {
            map,
            units: Vec::new(),
            fences: Vec::new(),
            fences_at: vec![usize::MAX; n],
            shells: Vec::new(),
            fx: Vec::new(),
            phase: Phase::Prep,
            time: 0.0,
            budget,
            spent: [0, 0],
            vis: [vec![false; n], vec![false; n]],
            spotted: [Vec::new(), Vec::new()],
            capture: [0.0, 0.0],
            winner: None,
            draw: false,
            reason: None,
            rng: Rng::new(seed as u64 ^ 0xbadc0de),
            vis_t: 0.0,
            skill: [1.0, 1.0],
            events: Vec::new(),
            cap_warned: [false, false],
            adjutants: [false; GENERALS.len()],
        }
    }

    pub fn left(&self, side: usize) -> u32 {
        self.budget.saturating_sub(self.spent[side])
    }

    pub fn base_pos(&self, side: usize) -> (f32, f32) {
        center(self.map.bases[side])
    }

    fn event(&mut self, side: usize, text: String, pos: Option<(f32, f32)>) {
        self.events.push(Event { side, text, pos });
    }

    // ------------------------------------------------------------ 準備（配置）

    /// その場所に部隊（wide=false）・デコイ／柵（wide=true）を置けるか
    pub fn can_place(&self, side: usize, p: (f32, f32), wide: bool, fences: bool) -> bool {
        if self.phase != Phase::Prep {
            return false;
        }
        let (tx, ty) = tile_of(p);
        if !inside(tx, ty) || !in_zone(side, tx, wide) {
            return false;
        }
        let t = self.map.at(tx, ty);
        if !t.passable() || self.fences_at[idx(tx, ty)] != usize::MAX {
            return false;
        }
        let b = self.map.bases[side];
        if fences {
            // 基地の真上には置けない
            if (tx - b.0).abs() <= 1 && (ty - b.1).abs() <= 1 {
                return false;
            }
            if self.units.iter().any(|u| u.alive && u.tile() == (tx, ty)) {
                return false;
            }
            return true;
        }
        !self.units.iter().any(|u| u.alive && u.kind != Kind::Hawk && dist(u.pos, p) < TS * 0.8)
    }

    pub fn new_unit(side: usize, kind: Kind, nation: Nation, ti: usize, pos: (f32, f32)) -> Unit {
        let face = if side == 0 { 0.0 } else { std::f32::consts::PI };
        let hp = match kind {
            Kind::Troop => nation.troops()[ti].hp,
            _ => 1.0,
        };
        Unit {
            side,
            kind,
            general: None,
            patrol_leg: None,
            nation,
            ti,
            pos,
            hull: face,
            turret: face,
            hp,
            alive: true,
            reload: 0.0,
            order: Order::Idle,
            path: Vec::new(),
            repath: 0.0,
            target: None,
            scan: 0.0,
            moving: false,
            fired: 99.0,
            revealed: false,
            dead_t: 0.0,
            kills: 0,
            breach: None,
        }
    }

    /// 部隊を買って置く
    pub fn place_troop(&mut self, side: usize, nation: Nation, ti: usize, p: (f32, f32)) -> bool {
        let cost = nation.troops()[ti].cost;
        if cost > self.left(side) || !self.can_place(side, p, false, false) {
            return false;
        }
        let c = center(tile_of(p));
        self.units.push(Self::new_unit(side, Kind::Troop, nation, ti, c));
        self.spent[side] += cost;
        true
    }

    /// デコイを置く（見せかけの部隊の種類を選ぶ）
    pub fn place_decoy(&mut self, side: usize, nation: Nation, ti: usize, p: (f32, f32)) -> bool {
        if DECOY_COST > self.left(side) || !self.can_place(side, p, true, false) {
            return false;
        }
        let c = center(tile_of(p));
        self.units.push(Self::new_unit(side, Kind::Decoy, nation, ti, c));
        self.spent[side] += DECOY_COST;
        true
    }

    /// 柵を置く
    pub fn place_fences(&mut self, side: usize, p: (f32, f32)) -> bool {
        if FENCE_COST > self.left(side) || !self.can_place(side, p, true, true) {
            return false;
        }
        let tile = tile_of(p);
        let mut seen = [false; 2];
        seen[side] = true;
        self.fences_at[idx(tile.0, tile.1)] = self.fences.len();
        self.fences.push(Fence { tile, side, hp: FENCE_HP, alive: true, seen });
        self.spent[side] += FENCE_COST;
        true
    }

    /// その場所の自軍の配置物を取り除いて払い戻す
    pub fn remove_at(&mut self, side: usize, p: (f32, f32)) -> bool {
        if self.phase != Phase::Prep {
            return false;
        }
        if let Some(i) = self.units.iter().position(|u| u.alive && u.side == side && dist(u.pos, p) < TS * 0.6) {
            let u = self.units.remove(i);
            self.spent[side] -= if u.kind == Kind::Troop { u.spec().cost } else { DECOY_COST };
            return true;
        }
        let (tx, ty) = tile_of(p);
        let k = self.fences_at[idx(tx, ty)];
        if k != usize::MAX && self.fences[k].side == side {
            self.fences.remove(k);
            self.rebuild_fences_index();
            self.spent[side] -= FENCE_COST;
            return true;
        }
        false
    }

    /// 自軍の配置を全部取り除く
    pub fn clear_side(&mut self, side: usize) {
        self.units.retain(|u| u.side != side);
        self.fences.retain(|t| t.side != side);
        self.rebuild_fences_index();
        self.spent[side] = 0;
    }

    fn rebuild_fences_index(&mut self) {
        self.fences_at.iter_mut().for_each(|v| *v = usize::MAX);
        for (k, t) in self.fences.iter().enumerate() {
            if t.alive {
                self.fences_at[idx(t.tile.0, t.tile.1)] = k;
            }
        }
    }

    pub fn troop_count(&self, side: usize) -> usize {
        self.units.iter().filter(|u| u.alive && u.side == side && u.kind == Kind::Troop).count()
    }

    /// 準備中の自軍部隊に将軍を任命する。None で直轄に戻す。
    pub fn assign_general(&mut self, index: usize, general: Option<usize>) -> bool {
        if self.phase != Phase::Prep || general.is_some_and(|id| id >= GENERALS.len()) {
            return false;
        }
        let Some(unit) = self.units.get_mut(index) else { return false };
        if !unit.alive || unit.side != 0 || unit.kind != Kind::Troop || unit.general == general {
            return false;
        }
        unit.general = general;
        true
    }

    /// 戦闘中に指定の将軍から指揮権を返還してもらい、副官扱いにする。
    pub fn return_general_command(&mut self, general: usize) -> Vec<usize> {
        if self.phase != Phase::Battle || general >= GENERALS.len() {
            return Vec::new();
        }
        let freed: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| {
                u.alive && u.side == 0 && u.kind == Kind::Troop && u.general == Some(general)
            })
            .map(|(i, _)| i)
            .collect();
        if freed.is_empty() {
            return freed;
        }
        for &i in &freed {
            self.units[i].general = None;
            self.order(i, Order::Idle);
        }
        self.adjutants[general] = true;
        freed
    }

    /// 戦闘中に自軍部隊の指揮を将軍（または副官）へ委任する。None で直轄へ戻す。
    pub fn delegate_in_battle(&mut self, index: usize, general: Option<usize>) -> bool {
        if self.phase != Phase::Battle || general.is_some_and(|id| id >= GENERALS.len()) {
            return false;
        }
        let Some(unit) = self.units.get_mut(index) else {
            return false;
        };
        if !unit.alive || unit.side != 0 || unit.kind != Kind::Troop || unit.general == general {
            return false;
        }
        let prev = unit.general;
        unit.general = general;
        if let Some(id) = general {
            self.adjutants[id] = false;
        } else {
            self.order(index, Order::Idle);
        }
        if let Some(old_id) = prev {
            let remaining = self
                .units
                .iter()
                .any(|u| u.alive && u.side == 0 && u.kind == Kind::Troop && u.general == Some(old_id));
            if !remaining {
                self.adjutants[old_id] = true;
            }
        }
        true
    }

    /// 戦闘を始める（鷹が基地から飛び立つ）
    pub fn start_battle(&mut self) {
        if self.phase != Phase::Prep {
            return;
        }
        self.phase = Phase::Battle;
        for side in 0..2 {
            let b = self.base_pos(side);
            let mut d = Self::new_unit(side, Kind::Hawk, Nation::Roster, 0, b);
            d.hp = 1.0;
            self.units.push(d);
        }
        self.update_vis();
        self.event(0, t!("戦闘開始！ もうデコイや柵は追加できない", "Battle begins! Decoys and palisades can no longer be added.", "战斗开始！不能再追加诱饵和栅栏").to_string(), None);
    }

    pub fn hawk(&self, side: usize) -> Option<usize> {
        self.units.iter().position(|u| u.side == side && u.kind == Kind::Hawk)
    }

    pub fn hawk_patrolling(&self, side: usize) -> bool {
        self.hawk(side).is_some_and(|i| self.units[i].patrol_leg.is_some())
    }

    pub fn set_hawk_patrol(&mut self, side: usize, enabled: bool) {
        if self.phase != Phase::Battle || self.hawk_patrolling(side) == enabled {
            return;
        }
        if let Some(i) = self.hawk(side) {
            self.order(i, Order::Idle);
            self.units[i].patrol_leg = enabled.then_some(0);
        }
    }

    // ------------------------------------------------------------ 命令

    pub fn order(&mut self, i: usize, o: Order) {
        if let Some(u) = self.units.get_mut(i) {
            if !u.alive || u.kind == Kind::Decoy {
                return;
            }
            u.order = o;
            u.path.clear();
            u.repath = 0.0;
            u.breach = None;
            if u.kind == Kind::Hawk {
                u.patrol_leg = None;
                if let Order::Attack(_) = o {
                    u.order = Order::Idle;
                }
            }
        }
    }

    /// 相手から見て、その部隊が攻撃してよいもの（見えていて、デコイなら見破られていない）か
    pub fn targetable_by(&self, side: usize, j: usize) -> bool {
        let u = &self.units[j];
        u.alive && u.side != side && u.kind != Kind::Hawk && self.spotted[side].get(j).copied().unwrap_or(false) && !(u.kind == Kind::Decoy && u.revealed)
    }

    // ------------------------------------------------------------ 1歩

    pub fn step(&mut self) {
        // 演出は準備中も進める
        self.fx.retain_mut(|f| {
            f.t += DT;
            f.t < f.life
        });
        if self.phase != Phase::Battle {
            return;
        }
        self.time += DT;
        self.vis_t -= DT;
        if self.vis_t <= 0.0 {
            self.vis_t = 0.25;
            self.update_vis();
        }
        for i in 0..self.units.len() {
            self.update_unit(i);
        }
        self.separate();
        self.update_shells();
        for u in self.units.iter_mut().filter(|u| !u.alive) {
            u.dead_t += DT;
        }
        self.update_capture();
        self.check_end();
    }

    pub(crate) fn extra_cost(&self, side: usize) -> impl Fn(i32, i32) -> Option<f32> + '_ {
        move |x, y| {
            let k = self.fences_at[idx(x, y)];
            if k == usize::MAX {
                return Some(0.0);
            }
            let t = &self.fences[k];
            if !t.alive {
                Some(0.0)
            } else if t.side == side {
                None
            } else if t.seen[side] {
                Some(14.0)
            } else {
                Some(0.0)
            }
        }
    }

    fn update_unit(&mut self, i: usize) {
        if !self.units[i].alive {
            return;
        }
        match self.units[i].kind {
            Kind::Decoy => return,
            Kind::Hawk => {
                self.update_hawk(i);
                return;
            }
            Kind::Troop => {}
        }
        let spec = self.units[i].spec();
        let side = self.units[i].side;
        {
            let u = &mut self.units[i];
            u.reload -= DT;
            u.fired += DT;
            u.scan -= DT;
            u.repath -= DT;
        }
        // 狙う相手
        if self.units[i].scan <= 0.0 {
            self.units[i].scan = 0.4 + (i % 5) as f32 * 0.02;
            let t = self.choose_target(i);
            self.units[i].target = t;
        }
        // 命令された攻撃
        let mut goal: Option<(f32, f32)> = None;
        let mut hold = false;
        match self.units[i].order {
            Order::Idle => {}
            Order::Move { goal: g, attack } => {
                goal = Some(g);
                if attack && matches!(self.units[i].target, Some(Target::Unit(_))) {
                    hold = true;
                }
            }
            Order::Attack(t) => {
                let valid = match t {
                    Target::Unit(j) => self.targetable_by(side, j),
                    Target::Fence(k) => self.fences[k].alive && self.fences[k].seen[side],
                };
                if !valid {
                    self.units[i].order = Order::Idle;
                } else {
                    let tp = self.target_pos(t);
                    if self.can_shoot(i, tp) {
                        self.units[i].target = Some(t);
                        hold = true;
                    } else {
                        goal = Some(tp);
                    }
                }
            }
        }
        // 柵に阻まれている
        if let Some(k) = self.units[i].breach {
            if self.fences[k].alive {
                hold = true;
                if self.units[i].target.is_none() || matches!(self.units[i].order, Order::Move { attack: false, .. }) && !matches!(self.units[i].target, Some(Target::Unit(_))) {
                    self.units[i].target = Some(Target::Fence(k));
                }
            } else {
                self.units[i].breach = None;
                self.units[i].repath = 0.0;
            }
        }
        // 移動
        let mut moving = false;
        if let (Some(g), false) = (goal, hold) {
            moving = self.move_toward(i, g, spec);
        }
        self.units[i].moving = moving;
        // 砲塔
        let aim = self.units[i].target.map(|t| {
            let tp = self.target_pos(t);
            let p = self.units[i].pos;
            (tp.1 - p.1).atan2(tp.0 - p.0)
        });
        let trate = match spec.weight {
            Weight::Light => 2.6,
            Weight::Medium => 1.8,
            Weight::Heavy => 1.2,
        } * DT;
        {
            let u = &mut self.units[i];
            if spec.turret {
                let want = aim.unwrap_or(u.hull);
                u.turret = turn_toward(u.turret, want, trate);
            } else {
                if let (Some(a), false) = (aim, moving) {
                    u.hull = turn_toward(u.hull, a, trate * 0.6);
                }
                let want = aim.unwrap_or(u.hull);
                let off = angle_diff(want, u.hull).clamp(-0.25, 0.25);
                u.turret = u.hull + off;
            }
        }
        // 射撃
        if let (Some(t), Some(a)) = (self.units[i].target, aim) {
            let u = &self.units[i];
            if u.reload <= 0.0 && angle_diff(a, u.turret).abs() < 0.06 && self.can_shoot(i, self.target_pos(t)) {
                self.fire(i, t);
            }
        }
    }

    fn update_hawk(&mut self, i: usize) {
        let u = &mut self.units[i];
        if let (Order::Idle, Some(leg)) = (u.order, u.patrol_leg) {
            let (x, y) = HAWK_WAYPOINTS[leg];
            let x = if u.side == 0 { x } else { 1.0 - x };
            u.order = Order::Move { goal: (x * MW as f32 * TS, y * MH as f32 * TS), attack: false };
            u.patrol_leg = Some((leg + 1) % HAWK_WAYPOINTS.len());
        }
        if let Order::Move { goal, .. } = u.order {
            let d = dist(u.pos, goal);
            let step = HAWK_SPEED * TS * DT;
            if d <= step {
                u.pos = goal;
                u.order = Order::Idle;
                u.moving = false;
            } else {
                let a = (goal.1 - u.pos.1).atan2(goal.0 - u.pos.0);
                u.hull = turn_toward(u.hull, a, 6.0 * DT);
                u.pos.0 += a.cos() * step;
                u.pos.1 += a.sin() * step;
                u.moving = true;
            }
        } else {
            u.moving = false;
        }
        u.turret += DT * 20.0;
    }

    /// 目標へ進む。動いたら true
    fn move_toward(&mut self, i: usize, goal: (f32, f32), spec: &TroopSpec) -> bool {
        let side = self.units[i].side;
        let margin = TS * 0.05;
        let goal = (goal.0.clamp(margin, MW as f32 * TS - margin),
            goal.1.clamp(margin, MH as f32 * TS - margin));
        let gt = tile_of(goal);
        let cur = self.units[i].tile();
        if self.units[i].repath <= 0.0 {
            let p = self.map.path(cur, gt, spec.weight, &self.extra_cost(side));
            let u = &mut self.units[i];
            if p.is_empty() && cur != gt {
                if matches!(u.order, Order::Move { .. }) {
                    u.order = Order::Idle;
                }
                u.repath = 0.5;
                return false;
            }
            u.path = p;
            u.repath = 3.0 + (i % 7) as f32 * 0.2;
        }
        // 次の目標点
        let next = loop {
            let u = &mut self.units[i];
            match u.path.first().copied() {
                None => {
                    // 経路の終わり
                    if dist(u.pos, goal) < TS * 0.45 {
                        if let Order::Move { .. } = u.order {
                            u.order = Order::Idle;
                        }
                        return false;
                    }
                    if gt == cur {
                        break goal;
                    }
                    return false;
                }
                Some(t) => {
                    let c = center(t);
                    let last = u.path.len() == 1;
                    let wp = if last && tile_of(goal) == t { goal } else { c };
                    if dist(u.pos, wp) < TS * 0.3 {
                        u.path.remove(0);
                        if u.path.is_empty() && cur != gt {
                            u.repath = 0.0;
                        }
                        continue;
                    }
                    break wp;
                }
            }
        };
        // 柵
        let nt = tile_of(next);
        let k = self.fences_at[idx(nt.0, nt.1)];
        if k != usize::MAX && self.fences[k].alive && nt != cur {
            if self.fences[k].side == side {
                self.units[i].repath = 0.0;
                self.units[i].path.clear();
                return false;
            }
            self.fences[k].seen[side] = true;
            self.units[i].breach = Some(k);
            return false;
        }
        let terr = self.map.at(cur.0, cur.1);
        let position = self.units[i].pos;
        let route_angle = (next.1 - position.1).atan2(next.0 - position.0);
        let a = self.avoid_troop(i, route_angle).unwrap_or(route_angle);
        let u = &mut self.units[i];
        let hrate = match spec.weight {
            Weight::Light => 2.6,
            Weight::Medium => 2.0,
            Weight::Heavy => 1.4,
        };
        u.hull = turn_toward(u.hull, a, hrate * DT);
        let align = angle_diff(a, u.hull).cos().max(0.0);
        let sp = spec.speed * TS * terr.speed(spec.weight).max(0.15) * align * align * DT;
        let step = sp.min(dist(position, next));
        let hull = u.hull;
        if step <= 0.01 {
            return false;
        }
        for heading in [hull, route_angle] {
            let p = (position.0 + heading.cos() * step, position.1 + heading.sin() * step);
            if self.free_for(i, p) {
                self.units[i].pos = p;
                return true;
            }
        }
        false
    }

    /// 前方に実車がいる場合は、地形が許す側から追い越す。
    fn avoid_troop(&self, i: usize, route: f32) -> Option<f32> {
        let pos = self.units[i].pos;
        let forward = (route.cos(), route.sin());
        let lateral = (-forward.1, forward.0);
        let (obstacle, offset) = self.units.iter().enumerate()
            .filter(|(j, unit)| *j != i && unit.alive && unit.kind == Kind::Troop)
            .filter_map(|(_, unit)| {
                let delta = (unit.pos.0 - pos.0, unit.pos.1 - pos.1);
                let ahead = delta.0 * forward.0 + delta.1 * forward.1;
                let across = delta.0 * lateral.0 + delta.1 * lateral.1;
                (ahead > 0.0 && ahead < TS * 1.25 && across.abs() < TS * 0.72)
                    .then_some((ahead, across, unit.pos))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, across, obstacle)| (obstacle, across))?;
        let preferred = if offset < -TS * 0.10 { 1.0 } else { -1.0 };
        for sign in [preferred, -preferred] {
            let shift = sign * TS * 0.88;
            let goal = (obstacle.0 + lateral.0 * shift, obstacle.1 + lateral.1 * shift);
            let angle = (goal.1 - pos.1).atan2(goal.0 - pos.0);
            let near = (pos.0 + angle.cos() * TS * 0.5, pos.1 + angle.sin() * TS * 0.5);
            if self.free_for(i, goal) && self.free_for(i, near) {
                return Some(angle);
            }
        }
        None
    }

    pub fn target_pos(&self, t: Target) -> (f32, f32) {
        match t {
            Target::Unit(j) => self.units[j].pos,
            Target::Fence(k) => center(self.fences[k].tile),
        }
    }

    fn can_shoot(&self, i: usize, tp: (f32, f32)) -> bool {
        let u = &self.units[i];
        dist(u.pos, tp) <= u.spec().range * TS && self.map.los(u.tile(), tile_of(tp))
    }

    fn choose_target(&self, i: usize) -> Option<Target> {
        let u = &self.units[i];
        let side = u.side;
        let spec = u.spec();
        let range = spec.range * TS;
        let mut best: Option<(f32, usize)> = None;
        for (j, e) in self.units.iter().enumerate() {
            if !self.targetable_by(side, j) {
                continue;
            }
            let d = dist(u.pos, e.pos);
            if d > range || !self.map.los(u.tile(), e.tile()) {
                continue;
            }
            let mut score = d / range;
            // 正面から抜けない相手は後回し
            if e.kind == Kind::Troop && spec.pen < e.spec().front * 0.9 {
                score += 0.6;
            }
            if u.target == Some(Target::Unit(j)) {
                score -= 0.25;
            }
            if best.is_none_or(|b| score < b.0) {
                best = Some((score, j));
            }
        }
        if let Some((_, j)) = best {
            return Some(Target::Unit(j));
        }
        if let Some(k) = u.breach {
            if self.fences[k].alive {
                return Some(Target::Fence(k));
            }
        }
        if let Order::Attack(t @ Target::Fence(_)) = u.order {
            return Some(t);
        }
        None
    }

    fn fire(&mut self, i: usize, t: Target) {
        let (pos, turret, spec, side, moving) = {
            let u = &self.units[i];
            (u.pos, u.turret, u.spec(), u.side, u.moving)
        };
        let tp = self.target_pos(t);
        let d = dist(pos, tp);
        let r = (d / (spec.range * TS)).min(1.0);
        let melee = spec.range < 2.0;
        let mut acc = (if melee { 0.98 } else { 0.9 - 0.48 * r }) * if moving { 0.55 } else { 1.0 } * self.skill[side];
        let mut hit = None;
        match t {
            Target::Unit(j) => {
                let e = &self.units[j];
                if e.moving {
                    acc *= 0.85;
                }
                let (tx, ty) = e.tile();
                if self.map.at(tx, ty).cover() {
                    acc *= 0.72;
                }
                if self.rng.f32() < acc {
                    let mut dmg;
                    let mut pen = true;
                    if e.kind == Kind::Troop {
                        let armor = e.armor_toward(pos);
                        let p = spec.pen * (1.0 - 0.3 * r) * self.rng.range(0.8, 1.2);
                        dmg = spec.dmg * DAMAGE_SCALE * self.rng.range(0.85, 1.15);
                        if p < armor {
                            dmg *= 0.12;
                            pen = false;
                        }
                    } else {
                        dmg = 99.0;
                    }
                    hit = Some((t, dmg, pen));
                }
            }
            Target::Fence(_) => {
                if self.rng.f32() < (acc * 1.3).min(0.97) {
                    hit = Some((t, spec.dmg * (spec.pen / 100.0).clamp(0.4, 1.4), true));
                }
            }
        }
        let to = if hit.is_some() {
            (tp.0 + self.rng.range(-4.0, 4.0), tp.1 + self.rng.range(-4.0, 4.0))
        } else {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let m = self.rng.range(0.6, 1.4) * TS;
            (tp.0 + a.cos() * m, tp.1 + a.sin() * m)
        };
        let blen = spec.len * TS * (0.15 + spec.barrel());
        let muzzle = (pos.0 + turret.cos() * blen, pos.1 + turret.sin() * blen);
        self.shells.push(Shell { from: muzzle, to, t: 0.0, dur: d / (TS * 45.0) + 0.05, side, shooter: i, hit, big: spec.dmg >= 40.0 });
        if !melee { self.fx.push(Fx { pos: muzzle, t: 0.0, life: 0.18, kind: FxKind::Flash }); }
        let u = &mut self.units[i];
        u.reload = spec.reload * self.rng.range(0.95, 1.05);
        u.fired = 0.0;
    }

    fn update_shells(&mut self) {
        let mut done = Vec::new();
        for (k, s) in self.shells.iter_mut().enumerate() {
            s.t += DT;
            if s.t >= s.dur {
                done.push(k);
            }
        }
        for &k in done.iter().rev() {
            let s = self.shells.remove(k);
            match s.hit {
                None => self.fx.push(Fx { pos: s.to, t: 0.0, life: 0.6, kind: FxKind::Dust }),
                Some((Target::Unit(j), dmg, pen)) => {
                    if !self.units[j].alive {
                        continue;
                    }
                    let kind = self.units[j].kind;
                    self.fx.push(Fx { pos: s.to, t: 0.0, life: 0.3, kind: if pen { FxKind::Spark } else { FxKind::Dust } });
                    if kind == Kind::Decoy {
                        let u = &mut self.units[j];
                        u.alive = false;
                        u.revealed = true;
                        let side = u.side;
                        let pos = u.pos;
                        self.event(side, if side == 0 { t!("デコイが撃たれた（敵を欺いた）", "A decoy was shot (the enemy was fooled)", "诱饵被击中（成功欺骗敌人）") } else { t!("撃った相手はデコイだった", "That target was a decoy", "击中的目标是诱饵") }.to_string(), Some(pos));
                        continue;
                    }
                    if !pen && dmg < 6.0 && self.rng.chance(0.5) {
                        // 跳弾
                    }
                    self.units[j].hp -= dmg;
                    if self.units[j].hp <= 0.0 {
                        self.units[j].alive = false;
                        self.units[j].hp = 0.0;
                        let pos = self.units[j].pos;
                        self.fx.push(Fx { pos, t: 0.0, life: 1.2, kind: FxKind::Boom });
                        self.units[s.shooter].kills += 1;
                        let side = self.units[j].side;
                        let name = self.units[j].spec().name();
                        let text = if side == 0 { tf!("自軍の{}が撃破された", "Our {} was destroyed", "我方{}被击毁", name) } else { tf!("敵の{}を撃破", "Enemy {} destroyed", "击毁敌方{}", name) };
                        self.event(side, text, Some(pos));
                    }
                }
                Some((Target::Fence(k2), dmg, _)) => {
                    self.fx.push(Fx { pos: s.to, t: 0.0, life: 0.4, kind: FxKind::Dust });
                    let tt = &mut self.fences[k2];
                    if tt.alive {
                        tt.hp -= dmg;
                        if tt.hp <= 0.0 {
                            tt.alive = false;
                            let (side, tile) = (tt.side, tt.tile);
                            self.fences_at[idx(tile.0, tile.1)] = usize::MAX;
                            self.fx.push(Fx { pos: center(tile), t: 0.0, life: 0.9, kind: FxKind::Boom });
                            let text = if side == 0 { t!("柵が破壊された", "Our palisades were destroyed", "我方栅栏被摧毁") } else { t!("敵の柵を破壊", "Enemy palisades destroyed", "摧毁敌方栅栏") };
                            self.event(side, text.to_string(), Some(center(tile)));
                            for u in self.units.iter_mut().filter(|u| u.breach == Some(k2)) {
                                u.breach = None;
                                u.repath = 0.0;
                            }
                        }
                    }
                }
            }
        }
    }

    /// 部隊どうしが重ならないよう押し合う
    fn separate(&mut self) {
        let n = self.units.len();
        let min = TS * 0.72;
        for a in 0..n {
            for b in a + 1..n {
                let (ua, ub) = (&self.units[a], &self.units[b]);
                if !ua.alive || !ub.alive || ua.kind != Kind::Troop || ub.kind != Kind::Troop {
                    continue;
                }
                let d = dist(ua.pos, ub.pos);
                if d >= min {
                    continue;
                }
                let (dx, dy) = if d < 0.01 {
                    let angle = ((a * 7 + b) % 8) as f32 * std::f32::consts::FRAC_PI_4;
                    (angle.cos(), angle.sin())
                } else {
                    ((ub.pos.0 - ua.pos.0) / d, (ub.pos.1 - ua.pos.1) / d)
                };
                let push = (min - d) * 0.5;
                let pa = separation_position(ua, (-dx * push, -dy * push));
                let pb = separation_position(ub, (dx * push, dy * push));
                if self.free_for(a, pa) {
                    self.units[a].pos = pa;
                }
                if self.free_for(b, pb) {
                    self.units[b].pos = pb;
                }
            }
        }
    }

    fn free_for(&self, i: usize, p: (f32, f32)) -> bool {
        let (x, y) = tile_of(p);
        if p.0 < 0.0 || p.1 < 0.0 || p.0 >= MW as f32 * TS || p.1 >= MH as f32 * TS {
            return false;
        }
        if !self.map.at(x, y).passable() {
            return false;
        }
        let k = self.fences_at[idx(x, y)];
        k == usize::MAX || !self.fences[k].alive || self.units[i].tile() == (x, y)
    }

    // ------------------------------------------------------------ 視界

    pub fn update_vis(&mut self) {
        for side in 0..2 {
            let mut vis = std::mem::take(&mut self.vis[side]);
            vis.iter_mut().for_each(|v| *v = false);
            for u in self.units.iter().filter(|u| u.alive && u.side == side) {
                let (cx, cy) = u.tile();
                match u.kind {
                    Kind::Hawk => {
                        let r = HAWK_SIGHT;
                        let ri = r.ceil() as i32;
                        for y in cy - ri..=cy + ri {
                            for x in cx - ri..=cx + ri {
                                if inside(x, y) && ((x - cx).pow(2) + (y - cy).pow(2)) as f32 <= r * r {
                                    vis[idx(x, y)] = true;
                                }
                            }
                        }
                    }
                    _ => {
                        let r = u.spec().sight + if self.map.at(cx, cy) == Terrain::Hill { 3.0 } else { 0.0 };
                        let r = if u.kind == Kind::Decoy { 0.0 } else { r };
                        let ri = r.ceil() as i32;
                        for y in cy - ri..=cy + ri {
                            for x in cx - ri..=cx + ri {
                                if inside(x, y) && !vis[idx(x, y)] && ((x - cx).pow(2) + (y - cy).pow(2)) as f32 <= r * r && self.map.los((cx, cy), (x, y)) {
                                    vis[idx(x, y)] = true;
                                }
                            }
                        }
                    }
                }
            }
            // 基地の周りは常に見える
            let (bx, by) = self.map.bases[side];
            for y in by - 2..=by + 2 {
                for x in bx - 2..=bx + 2 {
                    if inside(x, y) {
                        vis[idx(x, y)] = true;
                    }
                }
            }
            self.vis[side] = vis;
        }
        // 部隊の発見
        let n = self.units.len();
        for side in 0..2 {
            let mut sp = vec![false; n];
            for j in 0..n {
                let e = &self.units[j];
                if !e.alive {
                    continue;
                }
                if e.side == side {
                    sp[j] = true;
                    continue;
                }
                let (tx, ty) = e.tile();
                if !self.vis[side][idx(tx, ty)] {
                    continue;
                }
                if e.kind == Kind::Hawk || !self.map.at(tx, ty).cover() || e.fired < 2.5 {
                    sp[j] = true;
                    continue;
                }
                // 森・村落・生垣に隠れた部隊は近づかないと見つからない
                sp[j] = self.units.iter().any(|o| {
                    o.alive
                        && o.side == side
                        && match o.kind {
                            Kind::Troop => dist(o.pos, e.pos) <= o.spec().sight * 0.45 * TS && self.map.los(o.tile(), (tx, ty)),
                            Kind::Hawk => dist(o.pos, e.pos) <= 4.0 * TS,
                            Kind::Decoy => false,
                        }
                });
            }
            self.spotted[side] = sp;
        }
        // デコイの見破り：相手の部隊が近づくと分かる
        for j in 0..n {
            let e = &self.units[j];
            if !e.alive || e.kind != Kind::Decoy || e.revealed {
                continue;
            }
            let side = e.side;
            let near = self.units.iter().any(|o| o.alive && o.side != side && o.kind == Kind::Troop && dist(o.pos, e.pos) <= DECOY_REVEAL * TS && self.map.los(o.tile(), e.tile()));
            if near {
                let pos = e.pos;
                self.units[j].revealed = true;
                let text = if side == 0 { t!("デコイが見破られた", "A decoy was exposed", "诱饵被识破") } else { t!("敵のデコイを見破った", "Enemy decoy exposed", "识破敌方诱饵") };
                self.event(side, text.to_string(), Some(pos));
            }
        }
        // 柵の発見
        for t in self.fences.iter_mut() {
            for side in 0..2 {
                if self.vis[side][idx(t.tile.0, t.tile.1)] {
                    t.seen[side] = true;
                }
            }
        }
    }

    // ------------------------------------------------------------ 占領と勝敗

    fn update_capture(&mut self) {
        let r = CAPTURE_R * TS;
        for side in 0..2 {
            let b = self.base_pos(side);
            let att = self.units.iter().filter(|u| u.alive && u.side != side && u.kind == Kind::Troop && dist(u.pos, b) <= r).count();
            let def = self.units.iter().filter(|u| u.alive && u.side == side && u.kind == Kind::Troop && dist(u.pos, b) <= r).count();
            if att > 0 && def == 0 {
                let rate = (1.0 + 0.25 * (att as f32 - 1.0)).min(2.0);
                self.capture[side] = (self.capture[side] + DT / CAPTURE_TIME * rate).min(1.0);
                if !self.cap_warned[side] {
                    self.cap_warned[side] = true;
                    let text = if side == 0 { t!("基地が占領されつつある！", "Our base is being captured!", "我方基地正被占领！") } else { t!("敵基地を占領中", "Capturing the enemy base", "正在占领敌方基地") };
                    self.event(side, text.to_string(), Some(b));
                }
            } else if att == 0 {
                self.capture[side] = (self.capture[side] - DT / 45.0).max(0.0);
                if self.capture[side] == 0.0 {
                    self.cap_warned[side] = false;
                }
            }
        }
    }

    fn check_end(&mut self) {
        if self.phase != Phase::Battle {
            return;
        }
        let alive = [self.troop_count(0), self.troop_count(1)];
        let mut end = None;
        for side in 0..2 {
            if self.capture[side] >= 1.0 {
                end = Some((Some(1 - side), EndReason::Capture));
            }
        }
        if end.is_none() {
            end = match (alive[0], alive[1]) {
                (0, 0) => Some((None, EndReason::Annihilation)),
                (0, _) => Some((Some(1), EndReason::Annihilation)),
                (_, 0) => Some((Some(0), EndReason::Annihilation)),
                _ => None,
            };
        }
        if end.is_none() && self.time >= TIME_LIMIT {
            // 残った戦力（調達額）で判定
            let worth = |s: usize| self.units.iter().filter(|u| u.alive && u.side == s && u.kind == Kind::Troop).map(|u| u.spec().cost as f32 * u.hp / u.spec().hp).sum::<f32>();
            let (a, b) = (worth(0), worth(1));
            let w = if (a - b).abs() < 1.0 { None } else if a > b { Some(0) } else { Some(1) };
            end = Some((w, EndReason::TimeUp));
        }
        if let Some((w, r)) = end {
            self.phase = Phase::Over;
            self.winner = w;
            self.draw = w.is_none();
            self.reason = Some(r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Nation;

    fn world() -> World {
        World::new(Field::Kursk, 1234, 3000)
    }

    fn free_spot(w: &World, side: usize, wide: bool) -> (f32, f32) {
        for x in 0..MW {
            let x = if side == 0 { x } else { MW - 1 - x };
            for y in 0..MH {
                let p = center((x, y));
                if w.can_place(side, p, wide, false) && (!wide || !in_zone(side, x, false)) {
                    return p;
                }
            }
        }
        panic!("no spot");
    }

    #[test]
    fn budget_limits_purchases_and_refunds() {
        let mut w = World::new(Field::Kursk, 9, 1500);
        let mut n = 0;
        for y in 10..40 {
            let p = center((10, y));
            if w.place_troop(0, Nation::Roster, 3, p) {
                n += 1;
            }
        }
        assert_eq!(n, 1500 / Nation::Roster.troops()[3].cost);
        assert!(w.left(0) < Nation::Roster.troops()[3].cost);
        let p = w.units[0].pos;
        assert!(w.remove_at(0, p));
        assert_eq!(w.spent[0], 0);
    }

    #[test]
    fn melee_and_ranged_troops_respect_their_reach() {
        let mut w = world();
        w.map.t.fill(Terrain::Plain);
        let own = World::new_unit(0, Kind::Troop, Nation::Roster, 1, center((15, 20)));
        let enemy = World::new_unit(1, Kind::Troop, Nation::Roster, 1, center((20, 20)));
        w.units.extend([own, enemy]);
        for ti in 0..8 {
            w.units[0].ti = ti;
            assert_eq!(w.can_shoot(0, w.units[1].pos), !matches!(ti, 0 | 1 | 4));
        }
        w.units[0].ti = 1;
        w.units[1].pos = center((16,20));
        assert!(w.can_shoot(0,w.units[1].pos));
        w.fire(0,Target::Unit(1));
        assert!(w.fx.is_empty(), "Melee attacks have no firearm muzzle flash");
        w.units[0].ti = 2;
        w.fire(0,Target::Unit(1));
        assert!(w.fx.iter().any(|f|f.kind==FxKind::Flash));
    }

    #[test]
    fn placement_zones_and_battle_lock() {
        let mut w = world();
        // 敵陣には置けない
        assert!(!w.place_troop(0, Nation::Roster, 1, center((MW - 5, MH / 2))));
        // デコイと柵は自陣の半分まで置ける
        let p = free_spot(&w, 0, true);
        assert!(w.place_decoy(0, Nation::Roster, 1, p));
        let q = free_spot(&w, 0, true);
        assert!(w.place_fences(0, q));
        let t = free_spot(&w, 0, false);
        assert!(w.place_troop(0, Nation::Roster, 1, t));
        let t2 = free_spot(&w, 1, false);
        assert!(w.place_troop(1, Nation::Roster, 0, t2));
        w.start_battle();
        assert_eq!(w.phase, Phase::Battle);
        let r = free_spot_any(&w);
        assert!(!w.place_decoy(0, Nation::Roster, 1, r));
        assert!(!w.place_fences(0, r));
        // 鷹は戦力に数えない
        assert_eq!(w.troop_count(0), 1);
        assert!(w.hawk(0).is_some() && w.hawk(1).is_some());
    }

    fn free_spot_any(_w: &World) -> (f32, f32) {
        center((10, 10.min(MH - 1)))
    }

    #[test]
    fn hawks_are_never_targeted() {
        let mut w = world();
        let a = free_spot(&w, 0, false);
        w.place_troop(0, Nation::Roster, 3, a);
        let b = free_spot(&w, 1, false);
        w.place_troop(1, Nation::Roster, 1, b);
        w.start_battle();
        let d = w.hawk(1).unwrap();
        // 敵の鷹を自軍部隊の目の前に置く
        w.units[d].pos = (w.units[0].pos.0 + TS * 2.0, w.units[0].pos.1);
        w.update_vis();
        assert!(w.spotted[0][d]);
        assert!(!w.targetable_by(0, d));
        for _ in 0..90 {
            w.step();
        }
        assert!(w.units[d].alive);
        assert_ne!(w.units[0].target, Some(Target::Unit(d)));
    }

    #[test]
    fn battle_ends_by_annihilation() {
        let mut w = world();
        let a = free_spot(&w, 0, false);
        w.place_troop(0, Nation::Roster, 3, a);
        let b = free_spot(&w, 1, false);
        w.place_troop(1, Nation::Roster, 0, b);
        w.start_battle();
        let j = w.units.iter().position(|u| u.side == 1 && u.kind == Kind::Troop).unwrap();
        w.units[j].pos = (w.units[0].pos.0 + TS * 4.0, w.units[0].pos.1);
        w.units[j].hull = std::f32::consts::PI;
        for _ in 0..(30 * 120) {
            w.step();
            if w.phase == Phase::Over {
                break;
            }
        }
        assert_eq!(w.phase, Phase::Over);
        assert_eq!(w.winner, Some(0));
        assert_eq!(w.reason, Some(EndReason::Annihilation));
    }

    #[test]
    fn hawk_patrol_advances_through_waypoints_and_accepts_manual_override() {
        let mut w = world();
        w.start_battle();
        let hawk = w.hawk(0).unwrap();
        assert!(!w.hawk_patrolling(0));
        w.set_hawk_patrol(0, true);
        for (x, y) in HAWK_WAYPOINTS.into_iter().chain([HAWK_WAYPOINTS[0]]) {
            w.update_hawk(hawk);
            let goal = (x * MW as f32 * TS, y * MH as f32 * TS);
            assert_eq!(w.units[hawk].order, Order::Move { goal, attack: false });
            w.units[hawk].pos = goal;
            w.update_hawk(hawk);
            assert_eq!(w.units[hawk].order, Order::Idle);
        }
        let manual = Order::Move { goal: center((9, 9)), attack: false };
        w.order(hawk, manual);
        assert!(!w.hawk_patrolling(0));
        w.update_hawk(hawk);
        assert_eq!(w.units[hawk].order, manual);
        w.set_hawk_patrol(0, true);
        w.set_hawk_patrol(0, false);
        assert!(!w.hawk_patrolling(0));
        assert_eq!(w.units[hawk].order, Order::Idle);
    }

    #[test]
    fn troops_get_past_decoys_and_parked_troops_without_crossing_blocked_ground() {
        for (blocker, narrow) in [(Kind::Decoy, false), (Kind::Troop, false), (Kind::Troop, true)] {
            let mut w = world();
            w.map.t.fill(if narrow { Terrain::Water } else { Terrain::Plain });
            for x in 0..MW {
                w.map.t[idx(x, 20)] = Terrain::Road;
            }
            let start = center((8, 20));
            let block = center((10, 20));
            w.units.push(World::new_unit(0, Kind::Troop, Nation::Roster, 0, start));
            w.units.push(World::new_unit(0, blocker, Nation::Roster, 0, block));
            w.units.push(World::new_unit(1, Kind::Troop, Nation::Roster, 0, center((70, 20))));
            w.start_battle();
            w.order(0, Order::Move { goal: center((15, 20)), attack: false });
            let mut sidestep: f32 = 0.0;
            for _ in 0..(12.0 / DT) as usize {
                w.step();
                let (x, y) = w.units[0].tile();
                assert!(w.map.at(x, y).passable());
                sidestep = sidestep.max((w.units[0].pos.1 - start.1).abs());
            }
            assert!(w.units[0].pos.0 > block.0 + TS, "{blocker:?}, narrow={narrow}");
            if blocker == Kind::Decoy {
                assert_eq!(w.units[1].pos, block);
                assert!(sidestep < 1.0);
            }
            if blocker == Kind::Troop && !narrow {
                assert!(sidestep > TS * 0.2);
            }
        }
    }

    #[test]
    fn capture_wins_the_battle() {
        let mut w = world();
        let a = free_spot(&w, 0, false);
        w.place_troop(0, Nation::Roster, 0, a);
        let b = free_spot(&w, 1, false);
        w.place_troop(1, Nation::Roster, 0, b);
        w.start_battle();
        // 自軍を敵基地へ、敵を遠くへ
        let base1 = w.base_pos(1);
        w.units[0].pos = base1;
        let j = w.units.iter().position(|u| u.side == 1 && u.kind == Kind::Troop).unwrap();
        w.units[j].pos = center((MW / 2, 2));
        w.units[j].alive = true;
        for _ in 0..(30.0 * CAPTURE_TIME * 1.2) as usize {
            w.step();
            w.units[0].pos = base1;
            if w.phase == Phase::Over {
                break;
            }
        }
        assert_eq!(w.winner, Some(0));
        assert_eq!(w.reason, Some(EndReason::Capture));
    }

    #[test]
    fn decoy_is_exposed_up_close() {
        let mut w = world();
        let p = free_spot(&w, 1, true);
        assert!(w.place_decoy(1, Nation::Roster, 3, p));
        let a = free_spot(&w, 0, false);
        w.place_troop(0, Nation::Roster, 0, a);
        let b = free_spot(&w, 1, false);
        w.place_troop(1, Nation::Roster, 0, b);
        w.start_battle();
        let d = w.units.iter().position(|u| u.kind == Kind::Decoy).unwrap();
        let me = w.units.iter().position(|u| u.side == 0 && u.kind == Kind::Troop).unwrap();
        w.units[me].pos = (w.units[d].pos.0 - TS * 2.0, w.units[d].pos.1);
        w.update_vis();
        assert!(w.units[d].revealed || !w.map.los(w.units[me].tile(), w.units[d].tile()));
    }
}
