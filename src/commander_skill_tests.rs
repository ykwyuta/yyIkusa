use super::*;
use crate::data::Terrain;
use crate::general_skills::Bonuses;
use crate::map::{Field, TS};
use crate::sim::{Order, DT, TIME_LIMIT};

fn world(skill: CommanderSkill) -> World {
    let mut w = World::new(Field::Kursk, 1234, 3000);
    w.map.t.fill(Terrain::Plain);
    w.units.push(World::new_unit(
        0,
        Kind::Troop,
        Nation::Roster,
        0,
        center((10, 20)),
    ));
    w.units.push(World::new_unit(
        1,
        Kind::Troop,
        Nation::Roster,
        0,
        center((60, 20)),
    ));
    assert!(w.select_commander_skill(skill));
    w
}

#[test]
fn selection_locks_and_reserves_share_the_deployment_budget_with_exact_refunds() {
    let mut w = world(CommanderSkill::Reserves);
    let cost = Nation::Roster.troops()[0].cost;
    assert!(w.buy_reserve(Nation::Roster, 0));
    assert!(w.buy_reserve(Nation::Roster, 0));
    assert_eq!(w.left(0), 3000 - 2 * cost);
    assert!(!w.buy_reserve(Nation::Roster, usize::MAX));
    assert!(w.refund_last_reserve());
    assert_eq!(w.left(0), 3000 - cost);
    w.clear_side(0);
    assert_eq!(w.spent[0], cost); // Auto-deployment must still pay for the held reserve.
    assert_eq!(w.commander_skill.reserves.len(), 1);
    assert!(w.select_commander_skill(CommanderSkill::EagleEye));
    assert_eq!(w.left(0), 3000);
    assert!(w.commander_skill.reserves.is_empty());
    w.phase = Phase::Battle;
    assert!(!w.select_commander_skill(CommanderSkill::Reserves));
    assert!(!w.buy_reserve(Nation::Roster, 0));
    assert!(!w.refund_last_reserve());
    assert_eq!(w.commander_skill.selected, CommanderSkill::EagleEye);
}

#[test]
fn recon_reveals_hidden_enemies_and_terrain_for_twenty_seconds_with_two_independent_areas() {
    let mut w = world(CommanderSkill::EagleEye);
    let at = w.units[1].pos;
    let tile = w.units[1].tile();
    w.map.t[idx(tile.0, tile.1)] = Terrain::Forest;
    w.units[1].fired = 100.0;
    w.phase = Phase::Battle;
    w.update_vis();
    assert!(!w.spotted[0][1]);
    assert!(!w.use_commander_skill(Some((-TS, -TS))));
    assert!(!w.use_commander_skill(Some((f32::NAN, 0.0))));
    assert_eq!(w.commander_skill.remaining(), 2);
    assert!(w.use_commander_skill(Some(at)));
    assert!(w.spotted[0][1] && w.vis[0][idx(tile.0, tile.1)]);
    assert!(!w
        .commander_skill
        .recon_at((at.0 + (RECON_RADIUS + 1.0) * TS, at.1), w.time));
    w.time = 5.0;
    assert!(w.use_commander_skill(Some(center((35, 20)))));
    assert_eq!(w.commander_skill.recon.len(), 2);
    assert_eq!(w.commander_skill.remaining(), 0);
    assert!(!w.use_commander_skill(Some(at)));
    w.time = 20.0;
    w.expire_commander_recon();
    assert!(!w.spotted[0][1]);
    assert_eq!(w.commander_skill.recon.len(), 1);
    assert!(w.commander_skill.recon_at(center((35, 20)), w.time));
    w.time = 25.0;
    w.expire_commander_recon();
    assert!(w.commander_skill.recon.is_empty());
}

#[test]
fn supply_heals_direct_and_delegated_combat_units_and_excludes_dead_distant_and_noncombat_units() {
    let mut w = world(CommanderSkill::EmergencySupply);
    let at = w.units[0].pos;
    w.units[0].hp = w.units[0].spec().hp * 0.85;
    let mut ally = World::new_unit(0, Kind::Troop, Nation::Roster, 0, (at.0 + TS, at.1));
    ally.general = Some(0);
    ally.hp = ally.spec().hp * 0.5;
    w.units.push(ally);
    let mut dead = World::new_unit(0, Kind::Troop, Nation::Roster, 0, at);
    dead.alive = false;
    dead.hp = 0.0;
    w.units.push(dead);
    let mut scout = World::new_unit(0, Kind::Hawk, Nation::Roster, 0, at);
    scout.hp = 0.3;
    w.units.push(scout);
    let mut decoy = World::new_unit(0, Kind::Decoy, Nation::Roster, 0, at);
    decoy.hp = 0.3;
    w.units.push(decoy);
    let mut distant = World::new_unit(0, Kind::Troop, Nation::Roster, 0, center((30, 20)));
    distant.hp = distant.spec().hp * 0.2;
    w.units.push(distant);
    let old: Vec<_> = w.units.iter().map(|u| u.hp).collect();
    w.phase = Phase::Battle;
    assert!(!w.use_commander_skill(Some(center((45, 40)))));
    assert_eq!(w.commander_skill.remaining(), 1);
    assert!(w.use_commander_skill(Some(at)));
    assert_eq!(w.units[0].hp, w.units[0].spec().hp);
    assert!((w.units[2].hp / w.units[2].spec().hp - 0.8).abs() < 0.0001);
    for i in [1, 3, 4, 5, 6] {
        assert_eq!(w.units[i].hp, old[i]);
    }
    assert!(!w.use_commander_skill(Some(at)));
}

#[test]
fn breakthrough_follows_direct_control_and_expires_at_thirty_seconds() {
    let mut w = world(CommanderSkill::Breakthrough);
    let mut delegated = World::new_unit(0, Kind::Troop, Nation::Roster, 0, center((12, 20)));
    delegated.general = Some(0);
    w.units.push(delegated);
    w.phase = Phase::Battle;
    let delegated_bonus = w.command_bonuses(&w.units[2]);
    assert!(w.use_commander_skill(None));
    let bonus = w.command_bonuses(&w.units[0]);
    assert_eq!((bonus.damage, bonus.speed), (1.5, 1.25));
    assert_eq!(w.command_bonuses(&w.units[1]), Bonuses::NONE);
    assert_eq!(w.command_bonuses(&w.units[2]), delegated_bonus);
    w.delegate_in_battle(0, Some(0));
    assert_eq!(w.command_bonuses(&w.units[0]), delegated_bonus);
    w.delegate_in_battle(0, None);
    w.time = 29.99;
    assert_eq!(w.command_bonuses(&w.units[0]).damage, 1.5);
    w.time = 30.0;
    assert_eq!(w.command_bonuses(&w.units[0]), Bonuses::NONE);
    assert!(!w.use_commander_skill(None));
}

#[test]
fn breakthrough_changes_actual_movement_and_projectile_damage() {
    let mut plain = world(CommanderSkill::Breakthrough);
    let mut boosted = world(CommanderSkill::Breakthrough);
    for w in [&mut plain, &mut boosted] {
        w.phase = Phase::Battle;
        w.order(
            0,
            Order::Move {
                goal: center((25, 20)),
                attack: false,
            },
        );
    }
    let at = plain.units[0].pos;
    assert!(boosted.use_commander_skill(None));
    plain.step();
    boosted.step();
    assert!((dist(at, boosted.units[0].pos) / dist(at, plain.units[0].pos) - 1.25).abs() < 0.001);
    fn damage(boost: bool) -> f32 {
        let mut w = world(CommanderSkill::Breakthrough);
        w.phase = Phase::Battle;
        w.units[1].pos = center((11, 20));
        w.skill[0] = 100.0;
        if boost {
            assert!(w.use_commander_skill(None));
        }
        w.step();
        w.shells
            .iter()
            .find(|s| s.shooter == 0)
            .unwrap()
            .hit
            .unwrap()
            .1
    }
    assert!((damage(true) / damage(false) - 1.5).abs() < 0.0001);
}

#[test]
fn reserve_deployment_is_atomic_and_remains_usable_after_frontline_annihilation() {
    let mut w = world(CommanderSkill::Reserves);
    for _ in 0..3 {
        assert!(w.buy_reserve(Nation::Roster, 0));
    }
    let paid = w.spent[0];
    let base = w.map.bases[0];
    w.map.t.fill(Terrain::Water);
    w.map.t[idx(base.0, base.1)] = Terrain::Plain;
    w.units[0].pos = center(base);
    w.phase = Phase::Battle;
    assert!(!w.use_commander_skill(None));
    assert_eq!(
        (
            w.commander_skill.used,
            w.commander_skill.reserves.len(),
            w.spent[0]
        ),
        (0, 3, paid)
    );
    assert!(w.skill_cues.is_empty());
    w.map.t.fill(Terrain::Plain);
    w.units[0].alive = false;
    w.units[0].hp = 0.0;
    w.step();
    assert_eq!(w.phase, Phase::Battle);
    let before = w.units.len();
    assert!(w.use_commander_skill(None));
    assert_eq!(w.units.len(), before + 3);
    assert!(w.commander_skill.reserves.is_empty());
    assert_eq!(w.spent[0], paid);
    assert!(!w.use_commander_skill(None));
    for i in before..w.units.len() {
        let u = &w.units[i];
        assert!(u.alive && u.general.is_none() && u.manually_controlled());
        assert!(dist(u.pos, w.base_pos(0)) <= 8.0 * TS);
        for j in i + 1..w.units.len() {
            assert!(dist(u.pos, w.units[j].pos) >= TS * 0.85);
        }
    }
    for u in &mut w.units {
        if u.side == 0 {
            u.alive = false;
            u.hp = 0.0;
        }
    }
    w.step();
    assert_eq!(w.winner, Some(1));
}

#[test]
fn held_reserves_count_toward_time_limit_strength() {
    let mut w = world(CommanderSkill::Reserves);
    assert!(w.buy_reserve(Nation::Roster, 0));
    w.units[0].alive = false;
    w.units[0].hp = 0.0;
    w.units[1].hp = 0.1;
    w.phase = Phase::Battle;
    w.time = TIME_LIMIT - DT * 0.5;
    w.step();
    assert_eq!(w.phase, Phase::Over);
    assert_eq!(w.winner, Some(0));
}

#[test]
fn successful_player_skills_emit_their_own_priority_cutin_and_have_distinct_large_art() {
    use std::hash::{Hash, Hasher};
    let mut hashes = std::collections::HashSet::new();
    for skill in ALL {
        let mut w = world(skill);
        if skill == CommanderSkill::Reserves {
            assert!(w.buy_reserve(Nation::Roster, 0));
        }
        if skill == CommanderSkill::EmergencySupply {
            w.units[0].hp *= 0.5;
        }
        assert!(!w.use_commander_skill(Some(w.units[0].pos)));
        w.phase = Phase::Battle;
        assert!(w.use_commander_skill(Some(w.units[0].pos)));
        assert_eq!(
            w.skill_cues,
            vec![crate::skill_cutin::SkillCue::commander(skill)]
        );
        let (width, height, pixels) = art(skill.index()).unwrap();
        assert_eq!((width, height), (768, 512));
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        pixels.hash(&mut hash);
        assert!(hashes.insert(hash.finish()));
        let mut queue = crate::skill_cutin::Cutins::default();
        queue.push(crate::skill_cutin::SkillCue {
            side: 0,
            general: 0,
        });
        queue.push(w.skill_cues[0]);
        assert_eq!(queue.current().unwrap().0, w.skill_cues[0]);
    }
    assert!(art(4).is_none());
}

#[test]
fn consecutive_deliberate_recon_casts_each_restart_the_player_cutin() {
    let mut queue = crate::skill_cutin::Cutins::default();
    let cue = crate::skill_cutin::SkillCue::commander(CommanderSkill::EagleEye);
    queue.push(cue);
    queue.update(0.2);
    queue.push(cue);
    assert_eq!(queue.current(), Some((cue, 0.0)));
}
