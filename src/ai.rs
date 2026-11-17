//! コンピュータの陣営：準備（部隊の購入と配置、デコイ・柵）と戦闘中の指揮
//!
//! 戦闘中の AI は自分の陣営から見えている情報だけを使う。

use crate::data::{all_troops, Nation, Terrain, DECOY_COST, FENCE_COST};
use crate::map::{center, in_zone, inside, tile_of, MH, MW, TS};
use crate::rng::Rng;
use crate::sim::{dist, Kind, Order, Phase, Target, World};

/// 編成の方針
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Doctrine {
    /// 高価な重部隊を少数
    Heavy,
    /// 安価な部隊を大量に
    Swarm,
    /// 混成
    Mixed,
}

/// 準備：予算で部隊を買って置き、デコイと柵を置く
pub fn prepare(w: &mut World, side: usize, seed: u32) {
    let mut rng = Rng::new(seed as u64 ^ (side as u64 * 0x9e37));
    let doctrine = match rng.below(3) {
        0 => Doctrine::Heavy,
        1 => Doctrine::Swarm,
        _ => Doctrine::Mixed,
    };
    let budget = w.left(side);
    // 防御物に使う割合
    let defence = (budget as f32 * rng.range(0.08, 0.16)) as u32;
    let mut troop_budget = budget - defence;
    // 部隊を選ぶ
    let all: Vec<(Nation, usize)> = all_troops().collect();
    let mut picks: Vec<(Nation, usize)> = Vec::new();
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > 500 {
            break;
        }
        let afford: Vec<(Nation, usize)> = all.iter().copied().filter(|&(n, i)| n.troops()[i].cost <= troop_budget).collect();
        if afford.is_empty() {
            break;
        }
        let weight = |n: Nation, i: usize| -> f32 {
            let s = &n.troops()[i];
            let c = s.cost as f32;
            match doctrine {
                Doctrine::Heavy => c * c / 1000.0,
                Doctrine::Swarm => 30_000.0 / c,
                Doctrine::Mixed => 100.0 + if (250.0..500.0).contains(&c) { 200.0 } else { 0.0 },
            }
        };
        let total: f32 = afford.iter().map(|&(n, i)| weight(n, i)).sum();
        let mut r = rng.f32() * total;
        let mut pick = afford[0];
        for &(n, i) in &afford {
            r -= weight(n, i);
            if r <= 0.0 {
                pick = (n, i);
                break;
            }
        }
        troop_budget -= pick.0.troops()[pick.1].cost;
        picks.push(pick);
    }
    // 配置：基地の前方に横へ広げる
    let base = w.map.bases[side];
    let fwd = if side == 0 { 1 } else { -1 };
    let mut spots: Vec<(i32, i32)> = Vec::new();
    for x in 0..MW {
        if !in_zone(side, x, false) {
            continue;
        }
        for y in 2..MH - 2 {
            spots.push((x, y));
        }
    }
    // 基地からの前進距離と横の広がりで並べる
    let depth = rng.range(6.0, 13.0);
    spots.sort_by(|a, b| {
        let score = |t: &(i32, i32)| {
            let dx = ((t.0 - base.0) * fwd) as f32;
            let dy = (t.1 - base.1) as f32;
            let tcover = if w.map.at(t.0, t.1).cover() { -2.0 } else { 0.0 };
            (dx - depth).abs() * 1.2 + dy.abs() * 0.35 + tcover + crate::rng::hash2(t.0, t.1, seed) * 3.0
        };
        score(a).partial_cmp(&score(b)).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut si = 0;
    for (k, &(n, i)) in picks.iter().enumerate() {
        // 少し間隔をあける
        while si < spots.len() {
            let p = center(spots[si]);
            si += 1 + (k % 2);
            if w.place_troop(side, n, i, p) {
                break;
            }
        }
    }
    // 柵：基地へ向かう道と開けた進入路をふさぐ
    let fences_n = (defence as f32 * 0.55) as u32 / FENCE_COST;
    let r = rng.range(5.0, 8.0);
    let mut cands: Vec<((i32, i32), f32)> = Vec::new();
    for y in 0..MH {
        for x in 0..MW {
            if !in_zone(side, x, true) {
                continue;
            }
            let dx = ((x - base.0) * fwd) as f32;
            let dy = (y - base.1) as f32;
            let d = (dx * dx + dy * dy).sqrt();
            if dx < 1.0 || (d - r).abs() > 1.0 {
                continue;
            }
            let t = w.map.at(x, y);
            let pri = match t {
                Terrain::Road | Terrain::Bridge => 3.0,
                Terrain::Plain | Terrain::Sand => 2.0,
                _ => 1.0,
            } + rng.f32();
            cands.push(((x, y), pri));
        }
    }
    cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut placed = 0;
    for (t, _) in cands {
        if placed >= fences_n {
            break;
        }
        if w.place_fences(side, center(t)) {
            placed += 1;
        }
    }
    // デコイ：前線寄りの目立つ場所に
    let decoys = w.left(side) / DECOY_COST;
    let mut tries = 0;
    let mut made = 0;
    while made < decoys && tries < 400 {
        tries += 1;
        let x = if side == 0 { rng.below((MW / 2 - 4) as u32) as i32 + 8 } else { MW - 9 - rng.below((MW / 2 - 4) as u32) as i32 };
        let y = 3 + rng.below((MH - 6) as u32) as i32;
        if !inside(x, y) {
            continue;
        }
        let (n, i) = all[rng.below(all.len() as u32) as usize];
        if w.place_decoy(side, n, i, center((x, y))) {
            made += 1;
        }
    }
    // 余りでさらに部隊を
    while let Some(&(n, i)) = all.iter().filter(|&&(n, i)| n.troops()[i].cost <= w.left(side)).min_by_key(|&&(n, i)| n.troops()[i].cost) {
        let mut ok = false;
        while si < spots.len() {
            let p = center(spots[si]);
            si += 1;
            if w.place_troop(side, n, i, p) {
                ok = true;
                break;
            }
        }
        if !ok {
            break;
        }
    }
}

/// 戦闘中の指揮
pub struct Commander {
    pub side: usize,
    think: f32,
    rng: Rng,
    /// 進撃を始めるまでの秒数
    wait: f32,
    /// 迂回路（0=正面 1=北 2=南）
    route: u8,
    /// 防御に残す部隊
    defenders: Vec<usize>,
}

impl Commander {
    pub fn new(side: usize, seed: u32, aggression: f32) -> Commander {
        let mut rng = Rng::new(seed as u64 ^ 0xa11);
        let wait = rng.range(8.0, 25.0) / aggression.max(0.3);
        let route = rng.below(3) as u8;
        Commander { side, think: 0.0, rng, wait, route, defenders: Vec::new() }
    }

    pub fn update(&mut self, w: &mut World, dt: f32) {
        if w.phase != Phase::Battle {
            return;
        }
        self.think -= dt;
        self.wait -= dt;
        if self.think > 0.0 {
            return;
        }
        self.think = 1.5;
        let side = self.side;
        let foe = 1 - side;
        let my_base = w.base_pos(side);
        let foe_base = w.base_pos(foe);
        w.set_hawk_patrol(side, true);
        let troops: Vec<usize> = (0..w.units.len()).filter(|&i| w.units[i].alive && w.units[i].side == side && w.units[i].kind == Kind::Troop && w.units[i].general.is_none()).collect();
        if troops.is_empty() {
            return;
        }
        // 見えている敵
        let known: Vec<usize> = (0..w.units.len()).filter(|&j| w.targetable_by(side, j)).collect();
        // 基地への脅威
        let threat: Vec<usize> = known.iter().copied().filter(|&j| dist(w.units[j].pos, my_base) < 15.0 * TS).collect();
        // 防御役を決める（最初に一度、基地に近い部隊を）
        self.defenders.retain(|&i| w.units[i].alive);
        if self.defenders.is_empty() && troops.len() >= 3 {
            let n = ((troops.len() as f32) * 0.3).round().max(1.0) as usize;
            let mut by: Vec<usize> = troops.clone();
            by.sort_by(|&a, &b| dist(w.units[a].pos, my_base).partial_cmp(&dist(w.units[b].pos, my_base)).unwrap_or(std::cmp::Ordering::Equal));
            self.defenders = by.into_iter().take(n).collect();
        }
        // 防御：脅威があれば迎え撃つ、無ければ基地の周りに
        for &i in &self.defenders {
            if let Some(&t) = threat.iter().min_by(|&&a, &&b| dist(w.units[a].pos, w.units[i].pos).partial_cmp(&dist(w.units[b].pos, w.units[i].pos)).unwrap_or(std::cmp::Ordering::Equal)) {
                if !matches!(w.units[i].order, Order::Attack(Target::Unit(x)) if x == t) {
                    w.order(i, Order::Attack(Target::Unit(t)));
                }
            } else if dist(w.units[i].pos, my_base) > 5.0 * TS && w.units[i].order == Order::Idle {
                let a = self.rng.range(-1.2, 1.2) + if side == 0 { 0.0 } else { std::f32::consts::PI };
                let g = (my_base.0 + a.cos() * 2.5 * TS, my_base.1 + a.sin() * 2.5 * TS);
                w.order(i, Order::Move { goal: g, attack: true });
            }
        }
        let attackers: Vec<usize> = troops.iter().copied().filter(|i| !self.defenders.contains(i)).collect();
        if attackers.is_empty() {
            return;
        }
        // 基地が危なければ攻撃役も戻る
        if threat.len() >= 2 || w.capture[side] > 0.2 {
            for &i in &attackers {
                if dist(w.units[i].pos, my_base) < 22.0 * TS {
                    let t = threat.first().copied();
                    match t {
                        Some(t) => w.order(i, Order::Attack(Target::Unit(t))),
                        None => w.order(i, Order::Move { goal: my_base, attack: true }),
                    }
                }
            }
            return;
        }
        if self.wait > 0.0 {
            return;
        }
        // 攻撃役の重心
        let n = attackers.len() as f32;
        let cg = attackers.iter().fold((0.0, 0.0), |a, &i| (a.0 + w.units[i].pos.0 / n, a.1 + w.units[i].pos.1 / n));
        // 目標：近くに見えている敵がいればそこ、いなければ敵基地
        let near = known.iter().copied().filter(|&j| dist(w.units[j].pos, cg) < 18.0 * TS).min_by(|&a, &b| dist(w.units[a].pos, cg).partial_cmp(&dist(w.units[b].pos, cg)).unwrap_or(std::cmp::Ordering::Equal));
        let mut goal = match near {
            Some(j) => w.units[j].pos,
            None => foe_base,
        };
        // 迂回路：中央より手前では北か南へ寄せる
        let cx_frac = cg.0 / (MW as f32 * TS);
        let before_mid = if side == 0 { cx_frac < 0.5 } else { cx_frac > 0.5 };
        if near.is_none() && before_mid && self.route != 0 {
            let y = if self.route == 1 { MH as f32 * 0.22 } else { MH as f32 * 0.78 };
            let p = (MW as f32 * 0.5 * TS, y * TS);
            let t = tile_of(p);
            if w.map.at(t.0, t.1).passable() {
                goal = p;
            }
        }
        // 敵基地付近では基地の中心へ（占領）
        for &i in &attackers {
            let u = &w.units[i];
            let mut g = goal;
            if dist(u.pos, foe_base) < 10.0 * TS && known.iter().all(|&j| dist(w.units[j].pos, foe_base) > 6.0 * TS) {
                g = foe_base;
            }
            // 速い部隊は重心より先に出過ぎない
            let ahead = dist(u.pos, g) + 6.0 * TS < dist(cg, g);
            if ahead && near.is_none() {
                if u.order != Order::Idle {
                    w.order(i, Order::Idle);
                }
                continue;
            }
            let jitter = (self.rng.range(-1.5, 1.5) * TS, self.rng.range(-1.5, 1.5) * TS);
            let gg = (g.0 + jitter.0, g.1 + jitter.1);
            let need = match u.order {
                Order::Move { goal: og, .. } => dist(og, g) > 3.0 * TS,
                Order::Attack(_) => near.is_none(),
                Order::Idle => true,
            };
            if need {
                w.order(i, Order::Move { goal: gg, attack: true });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::FIELDS;
    use crate::sim::Phase;

    #[test]
    fn ai_spends_budget_and_places_defences() {
        for (k, f) in FIELDS.into_iter().enumerate() {
            let mut w = World::new(f, 100 + k as u32, 3000);
            prepare(&mut w, 1, 7 + k as u32);
            prepare(&mut w, 0, 9 + k as u32);
            for s in 0..2 {
                assert!(w.troop_count(s) >= 2, "{:?}", f);
                assert!(w.left(s) < 100, "{:?} left {}", f, w.left(s));
            }
            assert!(w.units.iter().any(|u| u.kind == Kind::Decoy));
            assert!(!w.fences.is_empty());
        }
    }

    /// AI 同士で最後まで戦える（勝敗がつく）
    #[test]
    fn ai_vs_ai_reaches_a_result() {
        for (k, f) in FIELDS.into_iter().chain(FIELDS).enumerate() {
            let mut w = World::new(f, 500 + k as u32, 2000);
            prepare(&mut w, 0, 1 + k as u32);
            prepare(&mut w, 1, 2 + k as u32);
            let mut a = Commander::new(0, 3, 1.0);
            let mut b = Commander::new(1, 4, 1.0);
            w.start_battle();
            let mut steps = 0;
            while w.phase == Phase::Battle {
                a.update(&mut w, crate::sim::DT);
                b.update(&mut w, crate::sim::DT);
                w.step();
                steps += 1;
            }
            assert_eq!(w.phase, Phase::Over, "{:?}", f);
            let kills: u32 = w.units.iter().map(|u| u.kills).sum();
            eprintln!("{:?}: winner {:?} by {:?} after {:.0}s, kills {}", f, w.winner, w.reason, steps as f32 * crate::sim::DT, kills);
        }
    }
}

#[cfg(test)]
mod bias {
    use super::*;
    use crate::map::FIELDS;
    use crate::sim::Phase;
    #[test]
    #[ignore]
    fn side_bias() {
        let mut wins = [0, 0, 0];
        for k in 0..36u32 {
            let f = FIELDS[(k % 6) as usize];
            let mut w = World::new(f, 900 + k / 2, 2000);
            let (s0, s1) = if k % 2 == 0 { (11 + k, 12 + k) } else { (12 + k - 1, 11 + k - 1) };
            prepare(&mut w, 0, s0);
            prepare(&mut w, 1, s1);
            let mut a = Commander::new(0, s0, 1.0);
            let mut b = Commander::new(1, s1, 1.0);
            w.start_battle();
            while w.phase == Phase::Battle {
                a.update(&mut w, crate::sim::DT);
                b.update(&mut w, crate::sim::DT);
                w.step();
            }
            wins[w.winner.unwrap_or(2)] += 1;
            eprintln!("{k} {:?} {:?} {:?} {:.0}", f, w.winner, w.reason, w.time);
        }
        eprintln!("{:?}", wins);
    }
}
