//! Named command skills. Bonuses are derived from the current commander, never stacked on specs.

use crate::generals::GENERALS;
use crate::map::TS;
use crate::sim::{dist, Kind, Phase, Unit, World};

pub const SKILL_DURATION: f32 = 300.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bonuses {
    pub damage: f32,
    pub speed: f32,
    pub range: f32,
    pub fire_rate: f32,
    pub armor: f32,
    pub penetration: f32,
    pub sight: f32,
    /// Fraction of maximum HP recovered per simulation second.
    pub repair: f32,
}

impl Bonuses {
    pub const NONE: Self = Self {
        damage: 1.0,
        speed: 1.0,
        range: 1.0,
        fire_rate: 1.0,
        armor: 1.0,
        penetration: 1.0,
        sight: 1.0,
        repair: 0.0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Passive,
    Once,
    Revenge,
}

pub struct Skill {
    pub names: [&'static str; 3],
    pub trigger: Trigger,
    pub bonuses: Bonuses,
}

impl Skill {
    pub fn name(&self) -> &'static str {
        t!(self.names[0], self.names[1], self.names[2])
    }

    pub fn description(&self) -> String {
        if self.trigger == Trigger::Revenge {
            return t!(
                "配下が撃破されるたび、撃破した敵1体を道連れにする。敵が既に死亡している場合は最寄りの敵戦闘部隊。",
                "Each lost unit takes its killer with it; if already dead, the nearest enemy combat unit instead.",
                "每名麾下阵亡时带走击杀者；若其已死，则带走最近的敌方战斗部队。"
            ).to_string();
        }
        let b = self.bonuses;
        let mut effects = Vec::new();
        for (label, value) in [
            (t!("攻撃力", "Damage", "攻击力"), b.damage),
            (t!("移動速度", "Speed", "移速"), b.speed),
            (t!("射程", "Range", "射程"), b.range),
            (t!("攻撃速度", "Fire rate", "攻速"), b.fire_rate),
            (t!("装甲", "Armor", "护甲"), b.armor),
            (t!("貫通力", "Penetration", "穿透"), b.penetration),
            (t!("視界", "Sight", "视野"), b.sight),
        ] {
            if value != 1.0 {
                effects.push(format!("{} ×{:.2}", label, value));
            }
        }
        if b.repair > 0.0 {
            effects.push(tf!(
                "毎秒HP {:.2}%回復",
                "Heal {:.2}% HP/s",
                "每秒恢复 {:.2}%生命",
                b.repair * 100.0
            ));
        }
        let prefix = if self.trigger == Trigger::Once {
            t!(
                "1戦1回・5分間：",
                "Once per battle, 5 min: ",
                "每战一次，5分钟："
            )
        } else {
            t!(
                "常時・指揮下のみ：",
                "Passive, commanded units: ",
                "常驻，仅麾下："
            )
        };
        format!("{}{}", prefix, effects.join(" / "))
    }
}

impl World {
    pub fn command_bonuses(&self, unit: &Unit) -> Bonuses {
        if self.phase != Phase::Battle || !unit.alive || unit.kind != Kind::Troop {
            return Bonuses::NONE;
        }
        if unit.manually_controlled() && self.time < self.commander_skill.breakthrough_until {
            return Bonuses { damage: 1.5, speed: 1.25, ..Bonuses::NONE };
        }
        let Some(id) = unit.general.filter(|&id| id < GENERALS.len()) else {
            return Bonuses::NONE;
        };
        let skill = &GENERALS[id].skill;
        if skill.trigger == Trigger::Once
            && !self.skill_remaining(unit.side, id).is_some_and(|t| t > 0.0)
        {
            return Bonuses::NONE;
        }
        skill.bonuses
    }

    /// None = unused; Some(0) = spent. Returning or reassigning command never resets this clock.
    pub fn skill_remaining(&self, side: usize, id: usize) -> Option<f32> {
        self.skill_started
            .get(side)?
            .get(id)?
            .map(|start| (SKILL_DURATION - (self.time - start)).max(0.0))
    }

    pub fn can_activate_skill(&self, side: usize, id: usize) -> bool {
        self.phase == Phase::Battle
            && side < 2
            && id < GENERALS.len()
            && GENERALS[id].skill.trigger == Trigger::Once
            && self.skill_started[side][id].is_none()
            && self.units.iter().any(|u| {
                u.alive && u.side == side && u.kind == Kind::Troop && u.general == Some(id)
            })
    }

    pub fn activate_skill(&mut self, side: usize, id: usize) -> bool {
        if !self.can_activate_skill(side, id) {
            return false;
        }
        self.skill_started[side][id] = Some(self.time);
        self.announce_skill(side, id);
        self.events.push(crate::sim::Event {
            side,
            text: tf!(
                "{}：{} 発動（5分間）",
                "{}: {} activated (5 min)",
                "{}：{}发动（5分钟）",
                GENERALS[id].name(),
                GENERALS[id].skill.name()
            ),
            pos: self
                .units
                .iter()
                .find(|u| u.alive && u.side == side && u.general == Some(id))
                .map(|u| u.pos),
        });
        true
    }

    /// Hidden enemy commanders must never be exposed by cinematic announcements.
    pub(crate) fn announce_skill(&mut self, side: usize, id: usize) {
        if side == 0
            || self.units.iter().enumerate().any(|(i, u)| {
                u.side == side
                    && u.general == Some(id)
                    && self.spotted[0].get(i).copied().unwrap_or(false)
            })
        {
            let cue = crate::skill_cutin::SkillCue { side, general: id };
            if !self.skill_cues.contains(&cue) {
                self.skill_cues.push(cue);
            }
        }
    }

    pub(crate) fn announce_passive_skills(&mut self) {
        for side in 0..2 {
            for (id, general) in GENERALS.iter().enumerate() {
                if general.skill.trigger != Trigger::Passive || self.passive_announced[side][id] {
                    continue;
                }
                let eligible = self.units.iter().enumerate().any(|(i, u)| {
                    u.alive
                        && u.side == side
                        && u.kind == Kind::Troop
                        && u.general == Some(id)
                        && (side == 0 || self.spotted[0].get(i).copied().unwrap_or(false))
                });
                if eligible {
                    self.announce_skill(side, id);
                    self.passive_announced[side][id] = true;
                }
            }
        }
    }

    pub(crate) fn update_enemy_skills(&mut self) {
        for id in 0..GENERALS.len() {
            if self.can_activate_skill(1, id)
                && self.units.iter().any(|u| {
                    u.alive
                        && u.side == 1
                        && u.general == Some(id)
                        && self.units.iter().enumerate().any(|(j, e)| {
                            self.targetable_by(1, j)
                                && dist(u.pos, e.pos) <= self.effective_range(u) + 2.0 * TS
                        })
                })
            {
                self.activate_skill(1, id);
            }
        }
    }

    pub(crate) fn effective_range(&self, unit: &Unit) -> f32 {
        unit.spec().range * TS * self.command_bonuses(unit).range
    }

    pub(crate) fn effective_armor(&self, unit: &Unit, from: (f32, f32)) -> f32 {
        unit.armor_toward(from) * self.command_bonuses(unit).armor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cutin_world() -> World {
        let mut w = World::new(crate::map::Field::Kursk, 1234, 3000);
        w.units.push(World::new_unit(
            0,
            Kind::Troop,
            crate::data::Nation::Roster,
            0,
            crate::map::center((10, 20)),
        ));
        w.units.push(World::new_unit(
            1,
            Kind::Troop,
            crate::data::Nation::Roster,
            0,
            crate::map::center((11, 20)),
        ));
        w.phase = Phase::Battle;
        w.spotted = [vec![true, false], vec![false, true]];
        w
    }

    #[test]
    fn cutin_passives_announce_once_per_battle_and_never_expose_hidden_enemies() {
        let mut w = cutin_world();
        let id = GENERALS
            .iter()
            .position(|g| g.skill.trigger == Trigger::Passive)
            .unwrap();
        w.units[0].general = Some(id);
        w.units[1].general = Some(id);
        w.announce_passive_skills();
        assert_eq!(
            w.skill_cues,
            vec![crate::skill_cutin::SkillCue {
                side: 0,
                general: id
            }]
        );
        w.skill_cues.clear();
        w.delegate_in_battle(0, None);
        w.delegate_in_battle(0, Some(id));
        w.announce_passive_skills();
        assert!(w.skill_cues.is_empty());
        w.spotted[0][1] = true;
        w.announce_passive_skills();
        assert_eq!(
            w.skill_cues,
            vec![crate::skill_cutin::SkillCue {
                side: 1,
                general: id
            }]
        );
        w.skill_cues.clear();
        w.announce_passive_skills();
        assert!(w.skill_cues.is_empty());
    }

    #[test]
    fn cutin_active_announcements_require_successful_activation_and_keep_enemy_visibility() {
        let mut w = cutin_world();
        let id = GENERALS
            .iter()
            .position(|g| g.skill.trigger == Trigger::Once)
            .unwrap();
        w.units[0].general = Some(id);
        w.units[1].general = Some(id);
        assert!(w.activate_skill(0, id));
        assert!(!w.activate_skill(0, id));
        assert_eq!(w.skill_cues.len(), 1);
        assert!(w.activate_skill(1, id));
        assert_eq!(w.skill_cues.len(), 1); // Hidden enemy skill still works, without revealing its commander.
    }

    #[test]
    fn every_general_has_a_named_distinct_and_usable_skill() {
        let mut names = std::collections::HashSet::new();
        let mut effects = std::collections::HashSet::new();
        for g in &GENERALS {
            for name in g.skill.names {
                assert!(!name.is_empty());
            }
            assert!(names.insert(g.skill.names[0]), "Duplicate skill name");
            assert!(
                effects.insert(format!("{:?}:{:?}", g.skill.trigger, g.skill.bonuses)),
                "Duplicate skill effects"
            );
            assert!(!g.skill.description().is_empty());
            assert!(g.skill.trigger == Trigger::Revenge || g.skill.bonuses != Bonuses::NONE);
            let b = g.skill.bonuses;
            for m in [
                b.damage,
                b.speed,
                b.range,
                b.fire_rate,
                b.armor,
                b.penetration,
                b.sight,
            ] {
                assert!(m.is_finite() && m >= 1.0);
            }
            assert!(b.repair >= 0.0 && b.repair < 0.01);
        }
    }
}
