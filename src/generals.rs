//! 将軍の名簿と、委任された自軍部隊の自動指揮。

use crate::data::Terrain;
use crate::general_skills::{Bonuses, Skill, Trigger};
use crate::map::{center, idx, inside, tile_of, MH, MW, TS};
use crate::sim::{dist, Kind, Order, Phase, Target, Unit, World, CAPTURE_R};

type Point = (f32, f32);

#[derive(Clone, Copy)]
pub enum Strategy {
    Offensive,
    Guerrilla,
    Defense,
    Raid,
}

impl Strategy {
    pub fn name(self) -> &'static str {
        match self {
            Self::Offensive => t!("積極攻勢", "Aggressive advance", "积极攻势"),
            Self::Guerrilla => t!("ゲリラ抵抗", "Guerrilla resistance", "游击抵抗"),
            Self::Defense => t!("拠点防衛", "Base defense", "据点防御"),
            Self::Raid => t!("遊撃", "Mobile raiding", "机动突袭"),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Offensive => t!(
                "敵を攻撃しながら敵基地へ前進。",
                "Engages nearby enemies and advances on their base.",
                "攻击附近敌军，向敌方基地推进。"
            ),
            Self::Guerrilla => t!(
                "遮蔽物に待ち伏せし、損傷や集中攻撃で後退。",
                "Ambushes from cover and falls back when damaged or outnumbered.",
                "依托掩体伏击，受损或寡不敌众时后撤。"
            ),
            Self::Defense => t!(
                "自軍基地の周辺で迎撃し、離れた敵は深追いしない。",
                "Intercepts threats near your base and stays within its perimeter.",
                "拦截基地周边威胁，坚守防区。"
            ),
            Self::Raid => t!(
                "側面を巡回し、孤立した弱い敵や損傷車を狙う。",
                "Patrols a flank and strikes vulnerable or damaged enemies.",
                "沿侧翼巡逻，攻击薄弱或受损的敌军。"
            ),
        }
    }

    pub fn color(self) -> [f32; 4] {
        match self {
            Self::Offensive => [1.0, 0.61, 0.42, 1.0],
            Self::Guerrilla => [0.65, 0.86, 0.48, 1.0],
            Self::Defense => [0.58, 0.80, 1.0, 1.0],
            Self::Raid => [0.86, 0.66, 1.0, 1.0],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gender { Female, Male }

pub struct General {
    pub gender: Gender,
    names: [&'static str; 3],
    pub strategy: Strategy,
    pub skill: Skill,
}

impl General {
    pub fn name(&self) -> &'static str {
        t!(self.names[0], self.names[1], self.names[2])
    }
}

use Strategy::{Defense, Guerrilla, Offensive, Raid};

// 徴側・徴弐は共同指揮の1枠。台詞はゲーム用創作。
pub const GENERALS: [General; 20] = [
    General { names: ["巴御前", "Tomoe Gozen", "巴御前"], gender: Gender::Female, strategy: Raid, skill: Skill { names: ["最期の一太刀", "Last Blade", "临终一刀"], trigger: Trigger::Revenge, bonuses: Bonuses::NONE }, },
    General { names: ["ジャンヌ・ダルク", "Joan of Arc", "贞德"], gender: Gender::Female, strategy: Offensive, skill: Skill { names: ["オルレアンの旗", "Banner of Orleans", "奥尔良之旗"], trigger: Trigger::Once, bonuses: Bonuses { damage: 1.5, ..Bonuses::NONE } }, },
    General { names: ["徴姉妹（徴側・徴弐）", "Trung Sisters", "征氏姐妹"], gender: Gender::Female, strategy: Guerrilla, skill: Skill { names: ["姉妹の結束", "Sisters United", "姐妹同心"], trigger: Trigger::Passive, bonuses: Bonuses { armor: 1.2, fire_rate: 1.2, ..Bonuses::NONE } }, },
    General { names: ["ラクシュミー・バーイー", "Lakshmibai", "拉克希米·巴伊"], gender: Gender::Female, strategy: Raid, skill: Skill { names: ["ジャーンシーの疾風", "Gale of Jhansi", "占西疾风"], trigger: Trigger::Passive, bonuses: Bonuses { speed: 1.4, penetration: 1.2, ..Bonuses::NONE } }, },
    General { names: ["ブーディカ", "Boudica", "布狄卡"], gender: Gender::Female, strategy: Offensive, skill: Skill { names: ["戦車女王の憤怒", "Queen of Vengeance", "战车女王之怒"], trigger: Trigger::Passive, bonuses: Bonuses { damage: 1.35, speed: 1.15, ..Bonuses::NONE } }, },
    General { names: ["アルテミシア", "Artemisia I", "阿尔忒弥西亚"], gender: Gender::Female, strategy: Defense, skill: Skill { names: ["海戦の盾", "Naval Shield", "海战之盾"], trigger: Trigger::Passive, bonuses: Bonuses { armor: 1.35, range: 1.1, ..Bonuses::NONE } }, },
    General { names: ["ゼノビア", "Zenobia", "芝诺比娅"], gender: Gender::Female, strategy: Defense, skill: Skill { names: ["砂漠の城塞", "Desert Citadel", "沙漠城塞"], trigger: Trigger::Passive, bonuses: Bonuses { range: 1.3, armor: 1.15, ..Bonuses::NONE } }, },
    General { names: ["タマル", "Tamar", "塔玛尔"], gender: Gender::Female, strategy: Defense, skill: Skill { names: ["黄金時代の加護", "Golden Age", "黄金时代庇护"], trigger: Trigger::Passive, bonuses: Bonuses { repair: 0.004, ..Bonuses::NONE } }, },
    General { names: ["花木蘭", "Hua Mulan", "花木兰"], gender: Gender::Female, strategy: Guerrilla, skill: Skill { names: ["千里の斥候", "Far-Ranging Scout", "千里斥候"], trigger: Trigger::Passive, bonuses: Bonuses { sight: 1.5, speed: 1.15, ..Bonuses::NONE } }, },
    General { names: ["トミュリス", "Tomyris", "托米丽司"], gender: Gender::Female, strategy: Raid, skill: Skill { names: ["草原女王の猛撃", "Steppe Queen Strike", "草原女王猛击"], trigger: Trigger::Passive, bonuses: Bonuses { damage: 1.25, penetration: 1.25, ..Bonuses::NONE } }, },
    General { names: ["アレクサンドロス", "Alexander the Great", "亚历山大"], gender: Gender::Male, strategy: Offensive, skill: Skill { names: ["征服王の進撃", "Conqueror March", "征服王进军"], trigger: Trigger::Passive, bonuses: Bonuses { speed: 1.25, damage: 1.25, ..Bonuses::NONE } }, },
    General { names: ["ハンニバル", "Hannibal", "汉尼拔"], gender: Gender::Male, strategy: Guerrilla, skill: Skill { names: ["包囲殲滅", "Encirclement", "包围歼灭"], trigger: Trigger::Passive, bonuses: Bonuses { penetration: 1.4, sight: 1.2, ..Bonuses::NONE } }, },
    General { names: ["チンギス・ハン", "Genghis Khan", "成吉思汗"], gender: Gender::Male, strategy: Raid, skill: Skill { names: ["蒼き狼の軍勢", "Host of the Blue Wolf", "苍狼军势"], trigger: Trigger::Passive, bonuses: Bonuses { speed: 1.5, range: 1.2, ..Bonuses::NONE } }, },
    General { names: ["サラディン", "Saladin", "萨拉丁"], gender: Gender::Male, strategy: Defense, skill: Skill { names: ["慈悲の守護", "Merciful Guard", "仁慈守护"], trigger: Trigger::Passive, bonuses: Bonuses { armor: 1.2, repair: 0.002, ..Bonuses::NONE } }, },
    General { names: ["織田信長", "Oda Nobunaga", "织田信长"], gender: Gender::Male, strategy: Offensive, skill: Skill { names: ["三段撃ち", "Rotating Volley", "三段射击"], trigger: Trigger::Passive, bonuses: Bonuses { fire_rate: 1.5, ..Bonuses::NONE } }, },
    General { names: ["カエサル", "Julius Caesar", "凯撒"], gender: Gender::Male, strategy: Offensive, skill: Skill { names: ["軍団の規律", "Legion Discipline", "军团纪律"], trigger: Trigger::Passive, bonuses: Bonuses { armor: 1.25, fire_rate: 1.15, ..Bonuses::NONE } }, },
    General { names: ["スブタイ", "Subutai", "速不台"], gender: Gender::Male, strategy: Raid, skill: Skill { names: ["大迂回", "Great Flanking March", "大迂回"], trigger: Trigger::Passive, bonuses: Bonuses { speed: 1.35, sight: 1.4, ..Bonuses::NONE } }, },
    General { names: ["武田信玄", "Takeda Shingen", "武田信玄"], gender: Gender::Male, strategy: Defense, skill: Skill { names: ["不動如山", "Immovable Mountain", "不动如山"], trigger: Trigger::Passive, bonuses: Bonuses { armor: 1.5, ..Bonuses::NONE } }, },
    General { names: ["李舜臣", "Yi Sun-sin", "李舜臣"], gender: Gender::Male, strategy: Guerrilla, skill: Skill { names: ["鶴翼の射陣", "Crane Wing Formation", "鹤翼射阵"], trigger: Trigger::Passive, bonuses: Bonuses { range: 1.4, penetration: 1.2, ..Bonuses::NONE } }, },
    General { names: ["ナポレオン", "Napoleon Bonaparte", "拿破仑"], gender: Gender::Male, strategy: Offensive, skill: Skill { names: ["大陸軍の砲火", "Grande Armee Fire", "大军团炮火"], trigger: Trigger::Passive, bonuses: Bonuses { damage: 1.3, range: 1.25, ..Bonuses::NONE } }, },
];

// Generated 2D character portraits, ordered exactly like GENERALS.
pub const PORTRAIT_SIZE: u32 = 128;
const PORTRAIT_BYTES: usize = (PORTRAIT_SIZE * PORTRAIT_SIZE * 4) as usize;
const PORTRAITS: &[u8; PORTRAIT_BYTES * GENERALS.len()] =
    include_bytes!("../assets/generals/portraits.bgra");
pub fn portrait_pixels(id: usize) -> Vec<u32> {
    PORTRAITS[id * PORTRAIT_BYTES..(id + 1) * PORTRAIT_BYTES]
        .chunks_exact(4)
        .map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
        .collect()
}

#[derive(Default)]
pub struct GeneralCommand {
    side: usize,
    next_update: f32,
    plans: Vec<Option<(usize, Plan)>>,
}

enum Plan {
    Advance(Point),
    Ambush {
        cover: Point,
        forward: Point,
        rear: Point,
    },
    Guard(Point),
    Patrol {
        points: [Point; 3],
        leg: usize,
    },
}

impl GeneralCommand {
    pub fn new(side: usize) -> Self {
        Self {
            side,
            ..Self::default()
        }
    }

    pub fn update(&mut self, world: &mut World) {
        if world.phase != Phase::Battle || world.time < self.next_update {
            return;
        }
        self.next_update = world.time + 1.0;
        self.sync_plans(world);
        if !self.plans.iter().any(Option::is_some) {
            return;
        }
        let visible: Vec<_> = (0..world.units.len())
            .filter(|&i| world.targetable_by(self.side, i))
            .collect();
        for (i, entry) in self.plans.iter_mut().enumerate() {
            let Some((_, plan)) = entry.as_mut() else { continue };
            let unit = &world.units[i];
            if !unit.alive {
                continue;
            }
            let order = plan.order(world, i, &visible);
            let same = match (unit.order, order) {
                (
                    Order::Move {
                        goal: old,
                        attack: a,
                    },
                    Order::Move {
                        goal: new,
                        attack: b,
                    },
                ) => a == b && dist(old, new) < TS,
                (old, new) => old == new,
            };
            if !same {
                world.order(i, order);
            }
        }
    }

    fn sync_plans(&mut self, world: &World) {
        self.plans.resize_with(world.units.len(), || None);
        let mut reachable: Option<Vec<bool>> = None;
        for (i, unit) in world.units.iter().enumerate() {
            if !unit.alive || unit.side != self.side {
                self.plans[i] = None;
                continue;
            }
            let Some(id) = unit.general else {
                self.plans[i] = None;
                continue;
            };
            if self.plans[i].as_ref().is_some_and(|(cur, _)| *cur == id) {
                continue;
            }
            let reach = reachable.get_or_insert_with(|| world.map.flood(world.map.bases[self.side]));
            self.plans[i] = Some((id, self.build_plan(world, reach, unit, i, id)));
        }
    }

    fn build_plan(
        &self,
        world: &World,
        reachable: &[bool],
        unit: &Unit,
        i: usize,
        id: usize,
    ) -> Plan {
        let home = world.map.bases[self.side];
        let enemy = world.map.bases[1 - self.side];
        let step = if self.side == 0 { 1 } else { -1 };
        let across = |x| if self.side == 0 { x } else { MW - 1 - x };
        let lane = unit.tile().1;
        let lateral = (i % 5) as i32 - 2;
        let point = |goal, radius, cover| best_tile(world, reachable, unit.pos, goal, radius, cover);
        match GENERALS[id].strategy {
            Offensive => Plan::Advance(point((enemy.0 - step, enemy.1 + lateral), 2, false)),
            Guerrilla => Plan::Ambush {
                cover: point((across(MW / 2 - 8), lane), 7, true),
                forward: point((across(MW * 2 / 3), (lane + enemy.1) / 2), 6, true),
                rear: point((across(MW / 3 - 6), lane), 7, true),
            },
            Defense => Plan::Guard(point((home.0 + step * 2, home.1 + lateral), 3, false)),
            Raid => {
                let flank = if id % 2 == 0 { MH / 5 } else { MH * 4 / 5 };
                let flank = flank + lateral;
                Plan::Patrol {
                    points: [
                        point((across(MW / 2 - 5), flank), 9, false),
                        point((across(MW * 3 / 4), flank), 9, false),
                        point((enemy.0 - step, enemy.1 + lateral), 2, false),
                    ],
                    leg: 0,
                }
            }
        }
    }
}

/// 敵の前衛の一部を、プレイヤーが使っていない将軍に委任する。
pub fn delegate_enemy(world: &mut World, seed: u32) {
    let available: Vec<_> = GENERALS
        .iter()
        .enumerate()
        .filter(|(_, general)| matches!(general.strategy, Offensive | Raid))
        .map(|(i, _)| i)
        .collect();
    let choices: Vec<_> = available
        .iter()
        .copied()
        .filter(|&i| {
            !world
                .units
                .iter()
                .any(|unit| unit.side == 0 && unit.general == Some(i))
        })
        .collect();
    let pool = if choices.is_empty() {
        &available
    } else {
        &choices
    };
    let general = pool[seed as usize % pool.len()];
    let base = world.base_pos(1);
    let mut troops: Vec<_> = world
        .units
        .iter()
        .enumerate()
        .filter(|(_, unit)| unit.alive && unit.side == 1 && unit.kind == Kind::Troop)
        .map(|(i, unit)| (i, dist(unit.pos, base)))
        .collect();
    troops.sort_by(|a, b| b.1.total_cmp(&a.1));
    let count = troops.len().div_ceil(2);
    for (i, _) in troops.into_iter().take(count) {
        world.units[i].general = Some(general);
    }
}

fn best_tile(
    world: &World,
    reachable: &[bool],
    fallback: Point,
    goal: (i32, i32),
    radius: i32,
    prefer_cover: bool,
) -> Point {
    let mut best = None;
    for y in (goal.1 - radius)..=(goal.1 + radius) {
        for x in (goal.0 - radius)..=(goal.0 + radius) {
            if !inside(x, y) || !reachable[idx(x, y)] || world.fences_at[idx(x, y)] != usize::MAX {
                continue;
            }
            let terrain = world.map.at(x, y);
            let covered = matches!(terrain, Terrain::Forest | Terrain::Town | Terrain::Bocage);
            let score = (x - goal.0).pow(2) + (y - goal.1).pow(2)
                - if prefer_cover && covered { 50 } else { 0 };
            if best.is_none_or(|(value, _)| score < value) {
                best = Some((score, (x, y)));
            }
        }
    }
    best.map_or(fallback, |(_, tile)| center(tile))
}

impl Plan {
    fn order(&mut self, world: &World, index: usize, visible: &[usize]) -> Order {
        let unit = &world.units[index];
        let position = unit.pos;
        let home = world.base_pos(unit.side);
        let home_dist = dist(position, home);
        if world.capture[unit.side] >= 0.08 && home_dist < TS * 20.0 {
            let radius = CAPTURE_R * TS;
            if home_dist > radius * 0.8 {
                return go(position, home, false);
            }
            return engage(world, unit, visible, Some((home, radius))).unwrap_or(Order::Idle);
        }
        let limit = match self {
            Self::Guard(_) => Some((home, TS * 12.0)),
            _ => None,
        };
        if let Self::Ambush { rear, .. } = self {
            let nearby = |pos| dist(position, pos) < TS * 6.0;
            let threats = visible
                .iter()
                .filter(|&&i| nearby(world.units[i].pos))
                .count();
            let friends = world
                .units
                .iter()
                .filter(|ally| {
                    ally.alive
                        && ally.side == unit.side
                        && ally.kind == Kind::Troop
                        && nearby(ally.pos)
                })
                .count();
            if unit.hp < unit.spec().hp * 0.42 || threats > friends + 1 {
                return go(position, *rear, false);
            }
        }
        if let Some(order) = engage(world, unit, visible, limit) {
            return order;
        }

        // 撃てても抜けない相手の前では、攻撃移動で足を止めない。
        let advance_fire = !visible
            .iter()
            .any(|&i| sight(world, unit, position, &world.units[i]));
        match self {
            Self::Advance(goal) => go(position, *goal, advance_fire),
            Self::Ambush { cover, forward, .. } => {
                let goal = if world.time >= 40.0 { *forward } else { *cover };
                go(position, goal, advance_fire)
            }
            Self::Guard(goal) => go(position, *goal, false),
            Self::Patrol { points, leg } => {
                while *leg + 1 < points.len() && dist(position, points[*leg]) < TS {
                    *leg += 1;
                }
                go(position, points[*leg], false)
            }
        }
    }
}

fn go(position: Point, goal: Point, attack: bool) -> Order {
    if dist(position, goal) < TS * 0.62 {
        Order::Idle
    } else {
        Order::Move { goal, attack }
    }
}

fn sight(world: &World, unit: &Unit, from: Point, enemy: &Unit) -> bool {
    dist(from, enemy.pos) <= world.effective_range(unit) && world.map.los(tile_of(from), enemy.tile())
}

fn penetration(world: &World, unit: &Unit, from: Point, enemy: &Unit) -> f32 {
    let fraction = (dist(from, enemy.pos) / (world.effective_range(unit))).min(1.0);
    unit.spec().pen * world.command_bonuses(unit).penetration * (1.0 - 0.3 * fraction)
}

fn engage(
    world: &World,
    unit: &Unit,
    visible: &[usize],
    limit: Option<(Point, f32)>,
) -> Option<Order> {
    let in_sector = |point| limit.is_none_or(|(base, radius)| dist(base, point) <= radius);
    let range = world.effective_range(unit);
    let viable = |enemy: &Unit| {
        let near = dist(unit.pos, enemy.pos) < range + TS * 4.0;
        near && in_sector(enemy.pos)
            && (limit.is_none()
                || dist(world.base_pos(unit.side), enemy.pos) <= (CAPTURE_R + 6.0) * TS)
    };
    let mut best_shot = None;
    let mut maneuvers = Vec::new();
    for &i in visible {
        let enemy = &world.units[i];
        if !viable(enemy) {
            continue;
        }
        let hp = if enemy.kind == Kind::Troop {
            enemy.hp / enemy.spec().hp
        } else {
            1.0
        };
        let focus = if unit.order == Order::Attack(Target::Unit(i)) {
            0.25
        } else {
            0.0
        };
        let score = dist(unit.pos, enemy.pos) / range + hp * 0.58 - focus;
        let armor = world.effective_armor(enemy, unit.pos);
        let pen = penetration(world, unit, unit.pos, enemy);
        if pen >= armor * 0.88 && sight(world, unit, unit.pos, enemy) {
            let shot_score = score + armor / pen * 0.30;
            if best_shot.is_none_or(|(best, _)| shot_score < best) {
                best_shot = Some((shot_score, i));
            }
        } else if unit.spec().pen * world.command_bonuses(unit).penetration >= enemy.spec().rear() * world.command_bonuses(enemy).armor * 0.83 {
            maneuvers.push((score, i));
        }
    }
    if let Some((_, i)) = best_shot {
        return Some(Order::Attack(Target::Unit(i)));
    }
    maneuvers.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, i) in maneuvers.into_iter().take(2) {
        let enemy = &world.units[i];
        let viable_shot = |point| {
            in_sector(point)
                && penetration(world, unit, point, enemy) >= world.effective_armor(enemy, point) * 0.90
                && sight(world, unit, point, enemy)
        };
        if let Order::Move {
            goal,
            attack: false,
        } = unit.order
        {
            if viable_shot(goal) {
                return Some(unit.order);
            }
        }
        if let Some(goal) = firing_position(world, unit, enemy, viable_shot) {
            return Some(Order::Move {
                goal,
                attack: false,
            });
        }
    }
    None
}

fn firing_position(
    world: &World,
    unit: &Unit,
    enemy: &Unit,
    viable: impl Fn(Point) -> bool,
) -> Option<Point> {
    let extra = world.extra_cost(unit.side);
    let mut candidates = Vec::new();
    for fraction in [0.58, 0.85] {
        let radius = world.effective_range(unit) * fraction;
        for direction in 0..8 {
            let angle = enemy.hull + direction as f32 * std::f32::consts::FRAC_PI_4;
            let target = tile_of((
                enemy.pos.0 + angle.cos() * radius,
                enemy.pos.1 + angle.sin() * radius,
            ));
            if !world.map.at(target.0, target.1).passable()
                || extra(target.0, target.1) != Some(0.0)
            {
                continue;
            }
            let point = center(target);
            let travel = dist(unit.pos, point);
            if travel < TS || !viable(point) {
                continue;
            }
            let exposed = if world.map.at(target.0, target.1).cover() {
                0.0
            } else {
                TS * 2.0
            };
            candidates.push((travel + exposed, target));
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    candidates.into_iter().take(3).find_map(|(_, tile)| {
        let path = world
            .map
            .path(unit.tile(), tile, unit.spec().weight, &extra);
        (path.last() == Some(&tile)).then(|| center(tile))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Nation;
    use crate::map::Field;

    fn troop(world: &mut World, side: usize, kind: Kind, tile: (i32, i32)) -> usize {
        let index = world.units.len();
        world.units.push(World::new_unit(
            side,
            kind,
            Nation::Roster,
            0,
            center(tile),
        ));
        index
    }

    #[test]
    fn assignment_only_accepts_friendly_troops_during_deployment() {
        let mut world = World::new(Field::Kursk, 19, 3000);
        let own = troop(&mut world, 0, Kind::Troop, (8, 15));
        let decoy = troop(&mut world, 0, Kind::Decoy, (9, 15));
        let enemy = troop(&mut world, 1, Kind::Troop, (69, 15));
        assert!(!world.assign_general(decoy, Some(0)));
        assert!(!world.assign_general(enemy, Some(0)));
        assert!(!world.assign_general(own, Some(GENERALS.len())));
        assert!(world.assign_general(own, Some(0)));
        assert!(!world.units[own].manually_controlled());
        assert!(world.assign_general(own, None));
        assert!(world.units[own].manually_controlled());
        assert!(world.assign_general(own, Some(2)));
        world.start_battle();
        assert!(!world.assign_general(own, None));
        assert_eq!(world.units[own].general, Some(2));
    }

    #[test]
    fn engages_an_effective_target_before_flanking_heavy_armor() {
        let mut world = World::new(Field::Kursk, 13, 3000);
        world.map.t.fill(Terrain::Plain);
        let own = troop(&mut world, 0, Kind::Troop, (15, 23));
        world.units[own].ti = 7;
        let heavy = troop(&mut world, 1, Kind::Troop, (19, 23));
        world.units[heavy].ti = 0;
        let light = troop(&mut world, 1, Kind::Troop, (23, 24));
        world.units[light].ti = 7;
        world.start_battle();
        world.spotted[0][heavy] = true;
        world.spotted[0][light] = true;
        let mut plan = Plan::Advance(center((70, 23)));
        assert_eq!(
            plan.order(&world, own, &[heavy, light]),
            Order::Attack(Target::Unit(light))
        );
        let Order::Move {
            goal,
            attack: false,
        } = plan.order(&world, own, &[heavy])
        else {
            panic!("a troop facing heavy frontal armor should find a firing position");
        };
        assert!(
            penetration(&world, &world.units[own], goal, &world.units[heavy])
                >= world.units[heavy].armor_toward(goal) * 0.90
        );
    }

    #[test]
    fn strategies_issue_distinct_orders_and_leave_direct_control_alone() {
        let mut world = World::new(Field::Kursk, 37, 3000);
        world.map.t.fill(Terrain::Plain);
        let lane = 26;
        let ambush_tile = (MW / 2 - 7, lane);
        world.map.t[idx(ambush_tile.0, ambush_tile.1)] = Terrain::Forest;
        let advance = troop(&mut world, 0, Kind::Troop, (12, lane - 1));
        let guerrilla = troop(&mut world, 0, Kind::Troop, (12, lane));
        let guard = troop(&mut world, 0, Kind::Troop, (12, lane + 1));
        let raid = troop(&mut world, 0, Kind::Troop, (12, lane + 2));
        let direct = troop(&mut world, 0, Kind::Troop, (12, lane + 3));
        for (unit, general) in [(advance, 1), (guerrilla, 2), (guard, 5), (raid, 0)] {
            assert!(world.assign_general(unit, Some(general)));
        }
        world.start_battle();
        let mut command = GeneralCommand::default();
        command.update(&mut world);
        assert!(
            matches!(world.units[advance].order, Order::Move { goal, attack: true } if tile_of(goal).0 > MW * 3 / 4)
        );
        assert_eq!(
            world.units[guerrilla].order,
            Order::Move {
                goal: center(ambush_tile),
                attack: true
            }
        );
        assert!(
            matches!(world.units[guard].order, Order::Move { goal, attack: false } if tile_of(goal).0 < 12)
        );
        assert!(
            matches!(world.units[raid].order, Order::Move { goal, attack: false } if tile_of(goal).1 < MH / 3)
        );
        assert_eq!(world.units[direct].order, Order::Idle);

        world.units[guerrilla].hp = 1.0;
        world.time = 2.0;
        command.update(&mut world);
        assert!(
            matches!(world.units[guerrilla].order, Order::Move { goal, attack: false } if tile_of(goal).0 < ambush_tile.0)
        );
    }

    #[test]
    fn returning_command_makes_adjutant_and_allows_redelegation() {
        let mut world = World::new(Field::Kursk, 41, 3000);
        world.map.t.fill(Terrain::Plain);
        let guard_a = troop(&mut world, 0, Kind::Troop, (12, 25));
        let guard_b = troop(&mut world, 0, Kind::Troop, (12, 27));
        assert!(world.assign_general(guard_a, Some(2)));
        assert!(world.assign_general(guard_b, Some(2)));
        world.start_battle();

        let mut command = GeneralCommand::default();
        command.update(&mut world);
        assert!(matches!(world.units[guard_a].order, Order::Move { .. }));

        let freed = world.return_general_command(2);
        assert_eq!(freed, vec![guard_a, guard_b]);
        assert!(world.adjutants[2]);
        assert!(world.units[guard_a].manually_controlled());
        assert_eq!(world.units[guard_a].order, Order::Idle);

        world.time = 2.0;
        command.update(&mut world);
        assert_eq!(world.units[guard_a].order, Order::Idle);

        assert!(world.delegate_in_battle(guard_a, Some(2)));
        assert!(!world.adjutants[2]);
        assert!(!world.units[guard_a].manually_controlled());
        assert!(world.units[guard_b].manually_controlled());

        world.time = 4.0;
        command.update(&mut world);
        assert!(matches!(world.units[guard_a].order, Order::Move { .. }));
        assert_eq!(world.units[guard_b].order, Order::Idle);
    }
}

#[cfg(test)]
mod roster_tests {
    use super::*;
    #[test]
    fn balanced_roster_and_joint_sisters_command() {
        assert_eq!(GENERALS.len(),20);
        assert_eq!(GENERALS.iter().filter(|g|g.gender==Gender::Female).count(),10);
        assert_eq!(GENERALS.iter().filter(|g|g.gender==Gender::Male).count(),10);
        assert_eq!(GENERALS.iter().filter(|g|g.names[1].contains("Trung")).count(),1);
        for id in 0..GENERALS.len() { assert_eq!(portrait_pixels(id).len(),128*128); }
    }
}
