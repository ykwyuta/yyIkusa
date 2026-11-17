//! 委任部隊の命令や戦況の変化を、将軍ごとの通信として通知する。

use crate::general_dialogue;
use crate::generals::GENERALS;
use crate::map::{tile_of, TS};
use crate::sim::{dist, Order, Phase, Target, World, CAPTURE_R};
use std::cmp::Reverse;

pub const EXPRESSIONS: usize = 10;
pub const EXPRESSION_SIZE: u32 = 96;
const MAX_WAITING: usize = 12;

// 左から右、上から下の順。肖像シートと台詞集で共通。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Ready,
    Advance,
    Hold,
    Contact,
    Danger,
    Support,
    Loss,
    Relief,
    Capture,
    Victory,
}

impl Cue {
    fn priority(self) -> u8 {
        match self {
            Self::Support | Self::Victory => 3,
            Self::Danger | Self::Loss | Self::Capture => 2,
            _ => 1,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Report {
    pub side: usize,
    pub general: usize,
    pub cue: Cue,
    pub tile: (i32, i32),
    variant: usize,
    created: f32,
}

impl Report {
    fn priority(self) -> (u8, bool) {
        let friendly = self.side == 0;
        (
            if friendly {
                self.cue.priority()
            } else {
                self.cue.priority().min(2)
            },
            friendly,
        )
    }

    pub fn line(self) -> &'static str {
        general_dialogue::line(self.general, self.cue as usize, self.variant)
    }
}

pub fn expression_pixels(general: usize, _cue: Cue) -> Vec<u32> {
    let p = crate::generals::portrait_pixels(general);
    (0..96*96).map(|i| p[(i / 96 * 128 / 96) * 128 + (i % 96 * 128 / 96)]).collect()
}

#[derive(Clone, Copy, Default)]
struct Status {
    alive: usize,
    dead: usize,
    advancing: bool,
    contact: bool,
    danger: bool,
    capturing: bool,
    tile: (i32, i32),
}

#[derive(Clone, Copy, Default)]
struct Group {
    status: Status,
    sum: (f32, f32),
    /// 最も急を要する車隊の状態と位置。
    aid_at: Option<((bool, usize), (i32, i32))>,
}

#[derive(Default)]
pub struct Radio {
    last: [[Status; GENERALS.len()]; 2],
    aid_after: [[f32; GENERALS.len()]; 2],
    variants: [[u16; GENERALS.len()]; 2],
    pending: Vec<Report>,
    current: Option<(Report, f32)>,
    next_check: f32,
    finished: bool,
}

impl Radio {
    pub fn current(&self) -> Option<Report> {
        self.current.map(|(report, _)| report)
    }

    pub fn update(&mut self, world: &World, dt: f32, paused: bool) {
        if world.phase == Phase::Prep {
            return;
        }
        if world.phase == Phase::Over && !self.finished {
            self.finish(world);
        } else if world.phase == Phase::Battle && world.time >= self.next_check && !paused {
            self.next_check = world.time + 1.0;
            self.observe(world, 0);
            self.observe(world, 1);
        }
        if paused {
            return;
        }
        if let Some((_, remaining)) = &mut self.current {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.current = None;
            }
        }
        self.pending
            .retain(|report| world.time - report.created < 36.0);
        let best = self
            .pending
            .iter()
            .enumerate()
            .max_by_key(|&(i, report)| (report.priority(), Reverse(i)));
        if let Some((i, next)) = best {
            if self.current.is_none_or(|(shown, _)| {
                next.side == 0 && next.cue == Cue::Support && shown.priority() < next.priority()
            }) {
                self.current = Some((self.pending.remove(i), 5.5));
            }
        }
    }

    fn observe(&mut self, world: &World, side: usize) {
        if !world
            .units
            .iter()
            .any(|unit| unit.side == side && unit.general.is_some())
            && self.last[side].iter().all(|s| s.alive == 0)
        {
            return;
        }
        let visible: Vec<_> = world
            .units
            .iter()
            .enumerate()
            .filter(|(i, _)| world.targetable_by(side, *i))
            .map(|(_, unit)| unit.pos)
            .collect();
        let foe = 1 - side;
        let enemy_base = world.base_pos(foe);
        let forward = if side == 0 { 1.0 } else { -1.0 };
        let mut groups = [Group::default(); GENERALS.len()];
        for (i, unit) in world.units.iter().enumerate() {
            if unit.side != side {
                continue;
            }
            let Some(id) = unit.general else { continue };
            if !unit.alive {
                groups[id].status.dead += 1;
                continue;
            }
            if side == 1 && !world.targetable_by(0, i) {
                continue;
            }
            let group = &mut groups[id];
            let threats = visible
                .iter()
                .filter(|&&pos| dist(pos, unit.pos) < 9.0 * TS)
                .count();
            group.status.alive += 1;
            group.status.advancing |= matches!(unit.order, Order::Move { goal, .. } if (goal.0 - unit.pos.0) * forward >= -TS);
            group.status.contact |= threats != 0
                || matches!(unit.target, Some(Target::Unit(_)))
                || matches!(unit.order, Order::Attack(Target::Unit(_)));
            let health = unit.hp / unit.spec().hp;
            group.status.danger |= threats > 1 || (threats != 0 && health < 0.65);
            let risk = (threats != 0 && health < 0.40, threats);
            if group.aid_at.is_none_or(|(old, _)| risk > old) {
                group.aid_at = Some((risk, unit.tile()));
            }
            group.status.capturing |=
                world.capture[foe] > 0.0 && dist(unit.pos, enemy_base) <= CAPTURE_R * TS;
            group.sum.0 += unit.pos.0;
            group.sum.1 += unit.pos.1;
        }
        for (id, mut group) in groups.into_iter().enumerate() {
            let prior = self.last[side][id];
            let status = &mut group.status;
            if (side == 1 || world.adjutants[id]) && status.alive == 0 {
                self.last[side][id] = Status::default();
                self.pending
                    .retain(|report| report.side != side || report.general != id);
                if self.current.is_some_and(|(r, _)| r.side == side && r.general == id) {
                    self.current = None;
                }
                continue;
            }
            if status.alive == 0 && prior.alive == 0 {
                continue;
            }
            let alive = status.alive as f32;
            status.tile = if status.alive == 0 {
                prior.tile
            } else {
                tile_of((group.sum.0 / alive, group.sum.1 / alive))
            };
            let aid = group.aid_at.and_then(|((critical, threats), tile)| {
                (critical
                    || (side == 0 && status.dead > prior.dead && threats >= status.alive)
                    || threats > status.alive + 1)
                    .then_some(tile)
            });
            let events = [
                (Cue::Ready, side == 0 && prior.alive == 0),
                (Cue::Advance, status.advancing && !prior.advancing),
                (
                    Cue::Hold,
                    status.alive > 0
                        && !status.advancing
                        && !status.contact
                        && (prior.advancing || prior.alive == 0),
                ),
                (Cue::Contact, status.contact && !prior.contact),
                (Cue::Danger, status.danger && !prior.danger && aid.is_none()),
                (Cue::Loss, side == 0 && status.dead > prior.dead),
                (
                    Cue::Relief,
                    prior.contact && !status.contact && status.alive > 0,
                ),
                (Cue::Capture, status.capturing && !prior.capturing),
            ];
            for (cue, triggered) in events {
                if triggered {
                    self.queue(side, id, cue, status.tile, world.time);
                }
            }
            if let Some(tile) = aid {
                if world.time >= self.aid_after[side][id] {
                    self.aid_after[side][id] = world.time + 30.0;
                    self.queue(side, id, Cue::Support, tile, world.time);
                }
            } else {
                self.pending.retain(|report| {
                    report.side != side || report.general != id || report.cue != Cue::Support
                });
            }
            self.last[side][id] = *status;
        }
    }

    fn finish(&mut self, world: &World) {
        self.finished = true;
        self.pending.clear();
        self.current = None;
        let Some(winner) = world.winner else { return };
        for side in [winner, 1 - winner] {
            let speaker = world
                .units
                .iter()
                .enumerate()
                .filter(|(i, unit)| {
                    unit.side == side
                        && (side == winner || !unit.alive)
                        && (side == 0 || world.targetable_by(0, *i))
                })
                .filter_map(|(_, unit)| unit.general.map(|id| (unit.alive, id, unit.tile())))
                .max_by_key(|&(alive, id, _)| (alive, Reverse(id)));
            if let Some((_, id, tile)) = speaker {
                let cue = if side == winner {
                    Cue::Victory
                } else {
                    Cue::Loss
                };
                self.queue(side, id, cue, tile, world.time);
                break;
            }
        }
    }

    fn queue(&mut self, side: usize, general: usize, cue: Cue, tile: (i32, i32), created: f32) {
        if self
            .pending
            .iter()
            .any(|r| r.side == side && r.general == general && r.cue == cue)
        {
            return;
        }
        let mask = 1 << cue as usize;
        let variant = usize::from(self.variants[side][general] & mask != 0);
        let report = Report {
            side,
            general,
            cue,
            tile,
            variant,
            created,
        };
        if self.pending.len() >= MAX_WAITING {
            let Some((i, lowest)) = self
                .pending
                .iter()
                .enumerate()
                .min_by_key(|&(i, r)| (r.priority(), Reverse(i)))
            else {
                return;
            };
            if lowest.priority() >= report.priority() {
                return;
            }
            self.pending.remove(i);
        }
        self.variants[side][general] ^= mask;
        self.pending.push(report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Nation, Terrain};
    use crate::map::{center, Field};
    use crate::sim::Kind;

    #[test]
    fn enemy_radio_requires_player_visibility_and_uses_the_enemy_side() {
        let mut world = World::new(Field::Kursk, 13, 3000);
        let mut enemy = World::new_unit(1, Kind::Troop, Nation::Roster, 0, center((70, 25)));
        enemy.general = Some(0);
        world.units.push(enemy);
        world.start_battle();
        world.order(
            0,
            Order::Move {
                goal: center((58, 25)),
                attack: true,
            },
        );

        let mut radio = Radio::default();
        radio.update(&world, 0.0, false);
        assert!(radio.current().is_none());
        assert!(radio.pending.is_empty());
        world.spotted[0][0] = true;
        world.time = 2.0;
        radio.update(&world, 0.0, false);
        let report = radio.current().unwrap();
        assert_eq!(report.side, 1);
        assert_eq!(report.cue, Cue::Advance);
    }

    #[test]
    fn rescue_preempts_a_routine_report_and_points_at_the_damaged_troop() {
        let mut world = World::new(Field::Kursk, 13, 3000);
        world.map.t.fill(Terrain::Plain);
        let damaged = (13, 25);
        world.units.push(World::new_unit(
            0,
            Kind::Troop,
            Nation::Roster,
            0,
            center((8, 25)),
        ));
        world.units.push(World::new_unit(
            0,
            Kind::Troop,
            Nation::Roster,
            0,
            center(damaged),
        ));
        world.units.push(World::new_unit(
            1,
            Kind::Troop,
            Nation::Roster,
            0,
            center((70, 35)),
        ));
        assert!(world.assign_general(0, Some(0)));
        assert!(world.assign_general(1, Some(0)));
        world.start_battle();
        world.order(
            0,
            Order::Move {
                goal: center((50, 25)),
                attack: true,
            },
        );

        let mut radio = Radio::default();
        radio.update(&world, 0.0, false);
        assert_eq!(radio.current().unwrap().cue, Cue::Ready);
        radio.update(&world, 5.5, false);
        assert_eq!(radio.current().unwrap().cue, Cue::Advance);

        world.units[1].hp = 1.0;
        world.units[2].pos = center((17, 25));
        world.spotted[0][2] = true;
        world.time = 2.0;
        radio.update(&world, 0.0, false);
        let report = radio.current().unwrap();
        assert_eq!(report.cue, Cue::Support);
        assert_eq!(report.tile, damaged);
        assert_eq!(report.variant, 0);

        world.time = 33.0;
        radio.update(&world, 5.5, false);
        assert_eq!(radio.current().unwrap().cue, Cue::Support);
        assert_eq!(radio.current().unwrap().variant, 1);
        world.winner = Some(0);
        world.phase = Phase::Over;
        radio.update(&world, 0.0, false);
        assert_eq!(radio.current().unwrap().cue, Cue::Victory);
    }

    #[test]
    fn returning_command_avoids_loss_report_and_redelegation_triggers_ready() {
        let mut world = World::new(Field::Kursk, 13, 3000);
        world.units.push(World::new_unit(
            0,
            Kind::Troop,
            Nation::Roster,
            0,
            center((12, 25)),
        ));
        assert!(world.assign_general(0, Some(2)));
        world.start_battle();

        let mut radio = Radio::default();
        radio.update(&world, 0.0, false);
        assert_eq!(radio.current().unwrap().cue, Cue::Ready);
        radio.update(&world, 6.0, false);
        assert!(radio.current().is_some_and(|r| r.cue == Cue::Hold));
        radio.update(&world, 6.0, false);
        assert!(radio.current().is_none());

        assert_eq!(world.return_general_command(2), vec![0]);
        world.time = 2.0;
        radio.update(&world, 0.0, false);
        assert!(radio.current().is_none());

        assert!(world.delegate_in_battle(0, Some(2)));
        world.time = 4.0;
        radio.update(&world, 0.0, false);
        assert_eq!(radio.current().unwrap().cue, Cue::Ready);
    }
}
