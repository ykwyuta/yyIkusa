//! Player supreme-command abilities. Uses, reserve purchases and timed effects belong to one battle.
use crate::data::Nation;
use crate::map::{center, idx, inside, MH, MW, TS};
use crate::sim::{dist, Event, Kind, Phase, World};

pub const RECON_RADIUS: f32 = 8.0;
pub const SUPPLY_RADIUS: f32 = 5.0;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum CommanderSkill {
    #[default]
    EagleEye,
    EmergencySupply,
    Breakthrough,
    Reserves,
}
pub const ALL: [CommanderSkill; 4] = [
    CommanderSkill::EagleEye,
    CommanderSkill::EmergencySupply,
    CommanderSkill::Breakthrough,
    CommanderSkill::Reserves,
];

impl CommanderSkill {
    pub fn index(self) -> usize {
        ALL.iter().position(|&s| s == self).unwrap()
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::EagleEye => t!("鷹の目", "Hawk Eye", "鹰眼"),
            Self::EmergencySupply => t!("緊急補給", "Emergency Supply", "紧急补给"),
            Self::Breakthrough => t!("突破命令", "Breakthrough Order", "突破命令"),
            Self::Reserves => t!("総予備隊", "Grand Reserve", "总预备队"),
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::EagleEye => t!("1戦2回。指定地点から半径8マスを20秒間偵察し、隠れた敵も発見。", "Twice per battle. Reveal an 8-tile radius for 20s, including hidden enemies.", "每战2次。侦察指定点周围8格，持续20秒，发现隐藏敌军。"),
            Self::EmergencySupply => t!("1戦1回。指定地点から半径5マスの自軍戦闘部隊を最大HPの30%回復。", "Once per battle. Heal friendly combat units within 5 tiles by 30% of maximum HP.", "每战1次。恢复指定点周围5格内友军战斗部队30%最大生命。"),
            Self::Breakthrough => t!("1戦1回。30秒間、直轄の戦闘部隊の攻撃力1.5倍・移動速度1.25倍。", "Once per battle. Direct combat units gain 1.5x damage and 1.25x speed for 30s.", "每战1次。直属战斗部队攻击力1.5倍、移速1.25倍，持续30秒。"),
            Self::Reserves => t!("同じ予算から予備部隊を購入。1戦1回、基地の周辺に一括投入。", "Buy reserve units from your deployment budget. Deploy them near your base once per battle.", "使用部署预算购买预备队。每战1次，在基地周围集结投入。"),
        }
    }
    pub fn max_uses(self) -> u8 {
        if self == Self::EagleEye {
            2
        } else {
            1
        }
    }
    pub fn targeted(self) -> bool {
        matches!(self, Self::EagleEye | Self::EmergencySupply)
    }
    pub fn color(self) -> [f32; 4] {
        match self {
            Self::EagleEye => [0.35, 0.85, 1.0, 1.0],
            Self::EmergencySupply => [0.45, 1.0, 0.65, 1.0],
            Self::Breakthrough => [1.0, 0.48, 0.3, 1.0],
            Self::Reserves => [0.75, 0.7, 1.0, 1.0],
        }
    }
}

#[derive(Clone, Copy)]
pub struct ReserveUnit {
    pub nation: Nation,
    pub ti: usize,
}
#[derive(Default)]
pub struct CommanderState {
    pub selected: CommanderSkill,
    pub used: u8,
    pub recon: Vec<((f32, f32), f32)>,
    pub breakthrough_until: f32,
    pub reserves: Vec<ReserveUnit>,
}

impl CommanderState {
    pub fn remaining(&self) -> u8 {
        self.selected.max_uses().saturating_sub(self.used)
    }
    pub fn recon_at(&self, p: (f32, f32), time: f32) -> bool {
        self.recon
            .iter()
            .any(|&(at, until)| time < until && dist(p, at) <= RECON_RADIUS * TS)
    }
    pub fn active_remaining(&self, time: f32) -> f32 {
        let until = self
            .recon
            .iter()
            .map(|&(_, t)| t)
            .fold(self.breakthrough_until, f32::max);
        (until - time).max(0.0)
    }
    pub fn reserve_cost(&self) -> u32 {
        self.reserves
            .iter()
            .map(|u| u.nation.troops()[u.ti].cost)
            .sum()
    }
}

impl World {
    pub fn select_commander_skill(&mut self, skill: CommanderSkill) -> bool {
        if self.phase != Phase::Prep {
            return false;
        }
        if self.commander_skill.selected != skill {
            self.clear_reserves();
            self.commander_skill = CommanderState {
                selected: skill,
                ..CommanderState::default()
            };
        }
        true
    }
    pub fn buy_reserve(&mut self, nation: Nation, ti: usize) -> bool {
        if self.phase != Phase::Prep || self.commander_skill.selected != CommanderSkill::Reserves {
            return false;
        }
        let Some(spec) = nation.troops().get(ti) else {
            return false;
        };
        if spec.cost > self.left(0) {
            return false;
        }
        self.spent[0] += spec.cost;
        self.commander_skill
            .reserves
            .push(ReserveUnit { nation, ti });
        true
    }
    pub fn refund_last_reserve(&mut self) -> bool {
        if self.phase != Phase::Prep {
            return false;
        }
        let Some(u) = self.commander_skill.reserves.pop() else {
            return false;
        };
        self.spent[0] -= u.nation.troops()[u.ti].cost;
        true
    }
    pub fn clear_reserves(&mut self) {
        if self.phase == Phase::Prep {
            self.spent[0] -= self.commander_skill.reserve_cost();
            self.commander_skill.reserves.clear();
        }
    }
    pub fn commander_ready(&self) -> bool {
        self.phase == Phase::Battle
            && self.commander_skill.remaining() > 0
            && match self.commander_skill.selected {
                CommanderSkill::Reserves => !self.commander_skill.reserves.is_empty(),
                CommanderSkill::Breakthrough => self
                    .units
                    .iter()
                    .any(|u| u.kind == Kind::Troop && u.manually_controlled()),
                _ => true,
            }
    }
    pub(crate) fn expire_commander_recon(&mut self) {
        let before = self.commander_skill.recon.len();
        self.commander_skill
            .recon
            .retain(|&(_, until)| self.time < until);
        if self.commander_skill.recon.len() != before {
            self.update_vis();
        }
    }
    pub fn use_commander_skill(&mut self, target: Option<(f32, f32)>) -> bool {
        if !self.commander_ready() {
            return false;
        }
        let skill = self.commander_skill.selected;
        let p = if skill.targeted() {
            let Some(p) = target else { return false };
            if !p.0.is_finite()
                || !p.1.is_finite()
                || p.0 < 0.0
                || p.1 < 0.0
                || p.0 >= MW as f32 * TS
                || p.1 >= MH as f32 * TS
            {
                return false;
            }
            p
        } else {
            self.base_pos(0)
        };
        match skill {
            CommanderSkill::EagleEye => {
                self.commander_skill.recon.push((p, self.time + 20.0));
            }
            CommanderSkill::EmergencySupply => {
                let mut healed = false;
                for u in &mut self.units {
                    if u.alive
                        && u.side == 0
                        && u.kind == Kind::Troop
                        && u.hp < u.spec().hp
                        && dist(u.pos, p) <= SUPPLY_RADIUS * TS
                    {
                        u.hp = (u.hp + u.spec().hp * 0.30).min(u.spec().hp);
                        healed = true;
                    }
                }
                if !healed {
                    return false;
                }
            }
            CommanderSkill::Breakthrough => {
                self.commander_skill.breakthrough_until = self.time + 30.0
            }
            CommanderSkill::Reserves => {
                // Plan all slots first. A blocked base must never spend a use or lose purchased units.
                let base = self.map.bases[0];
                let reachable = self.map.flood(base);
                let mut candidates = Vec::new();
                for y in base.1 - 8..=base.1 + 8 {
                    for x in base.0 - 8..=base.0 + 8 {
                        if !inside(x, y)
                            || !reachable[idx(x, y)]
                            || !self.map.at(x, y).passable()
                            || self.fences_at[idx(x, y)] != usize::MAX
                        {
                            continue;
                        }
                        let at = center((x, y));
                        if dist(at, self.base_pos(0)) > 8.0 * TS
                            || self.units.iter().any(|u| {
                                u.alive && u.kind != Kind::Hawk && dist(u.pos, at) < TS * 0.85
                            })
                        {
                            continue;
                        }
                        candidates.push(at);
                    }
                }
                candidates.sort_by(|&a, &b| {
                    dist(a, self.base_pos(0)).total_cmp(&dist(b, self.base_pos(0)))
                });
                if candidates.len() < self.commander_skill.reserves.len() {
                    return false;
                }
                for (reserve, at) in self.commander_skill.reserves.drain(..).zip(candidates) {
                    self.units.push(Self::new_unit(
                        0,
                        Kind::Troop,
                        reserve.nation,
                        reserve.ti,
                        at,
                    ));
                }
            }
        }
        self.commander_skill.used += 1;
        self.update_vis();
        self.skill_cues
            .push(crate::skill_cutin::SkillCue::commander(skill));
        self.events.push(Event {
            side: 0,
            text: tf!(
                "総司令官：{} 発動",
                "Supreme command: {} activated",
                "总指挥：{}发动",
                skill.name()
            ),
            pos: Some(p),
        });
        true
    }
}

const ART: [&[u8]; 4] = [
    include_bytes!("../assets/commander/eagle_eye.yyci"),
    include_bytes!("../assets/commander/emergency_supply.yyci"),
    include_bytes!("../assets/commander/breakthrough.yyci"),
    include_bytes!("../assets/commander/reserves.yyci"),
];
pub fn art(id: usize) -> Option<(u32, u32, Vec<u32>)> {
    crate::skill_cutin::decode(ART.get(id)?)
}

#[cfg(test)]
#[path = "commander_skill_tests.rs"]
mod tests;
