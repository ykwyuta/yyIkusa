use super::*;

fn traffic_world(narrow: bool) -> World {
    let mut w = World::new(Field::Kursk, 1234, 6000);
    w.map.t.fill(if narrow {
        Terrain::Water
    } else {
        Terrain::Plain
    });
    for x in 0..MW {
        w.map.t[idx(x, 20)] = Terrain::Road;
    }
    w.phase = Phase::Battle;
    w
}

fn advance(w: &mut World, seconds: f32) {
    for _ in 0..(seconds / DT) as usize {
        w.time += DT;
        for i in 0..w.units.len() {
            w.update_unit(i);
        }
        w.separate();
        for u in &w.units {
            assert!(w.map.at(u.tile().0, u.tile().1).passable());
        }
    }
}

#[test]
fn opposing_friendly_columns_pass_on_a_single_tile_road() {
    let mut w = traffic_world(true);
    for k in 0..5 {
        w.units.push(World::new_unit(
            0,
            Kind::Troop,
            crate::data::Nation::Roster,
            0,
            center((8 + k, 20)),
        ));
        let i = w.units.len() - 1;
        w.order(
            i,
            Order::Move {
                goal: center((28 + k, 20)),
                attack: false,
            },
        );
        w.units.push(World::new_unit(
            0,
            Kind::Troop,
            crate::data::Nation::Roster,
            0,
            center((32 - k, 20)),
        ));
        let i = w.units.len() - 1;
        w.order(
            i,
            Order::Move {
                goal: center((12 - k, 20)),
                attack: false,
            },
        );
    }
    advance(&mut w, 30.0);
    let arrived = w
        .units
        .iter()
        .enumerate()
        .filter(|(i, u)| {
            if i % 2 == 0 {
                u.pos.0 > center((26, 20)).0
            } else {
                u.pos.0 < center((14, 20)).0
            }
        })
        .count();
    assert!(
        arrived >= 8,
        "Only {arrived}/10 units passed; positions: {:?}",
        w.units.iter().map(|u| u.tile()).collect::<Vec<_>>()
    );
}

#[test]
fn moving_units_get_past_a_dense_parked_group() {
    let mut w = traffic_world(false);
    for y in 18..=22 {
        for x in 12..=14 {
            w.units.push(World::new_unit(
                0,
                Kind::Troop,
                crate::data::Nation::Roster,
                0,
                center((x, y)),
            ));
        }
    }
    let first = w.units.len();
    for k in 0..8 {
        w.units.push(World::new_unit(
            0,
            Kind::Troop,
            crate::data::Nation::Roster,
            0,
            center((7 - k % 2, 18 + k / 2)),
        ));
        let i = w.units.len() - 1;
        w.order(
            i,
            Order::Move {
                goal: center((22, 18 + k / 2)),
                attack: false,
            },
        );
    }
    advance(&mut w, 22.0);
    let passed = w.units[first..]
        .iter()
        .filter(|u| u.pos.0 > center((18, 20)).0)
        .count();
    assert!(
        passed >= 7,
        "Only {passed}/8 passed the parked group: {:?}",
        w.units[first..]
            .iter()
            .map(|u| u.tile())
            .collect::<Vec<_>>()
    );
}

#[test]
fn route_prefers_a_clear_lane_around_parked_friends() {
    let mut w = traffic_world(false);
    for x in 12..=16 {
        w.units.push(World::new_unit(
            0,
            Kind::Troop,
            crate::data::Nation::Roster,
            0,
            center((x, 20)),
        ));
    }
    w.units.push(World::new_unit(
        0,
        Kind::Troop,
        crate::data::Nation::Roster,
        0,
        center((8, 20)),
    ));
    let i = w.units.len() - 1;
    w.order(
        i,
        Order::Move {
            goal: center((22, 20)),
            attack: false,
        },
    );
    w.update_unit(i);
    assert!(!w.units[i].path.is_empty());
    assert!(
        w.units[i]
            .path
            .iter()
            .all(|&(x, y)| !(12..=16).contains(&x) || y != 20),
        "Route should bypass the parked column: {:?}",
        w.units[i].path
    );
}

#[test]
fn narrow_road_allows_a_small_passing_manoeuvre() {
    let mut w = traffic_world(true);
    w.units.push(World::new_unit(
        0,
        Kind::Troop,
        crate::data::Nation::Roster,
        0,
        center((10, 20)),
    ));
    w.units.push(World::new_unit(
        0,
        Kind::Troop,
        crate::data::Nation::Roster,
        0,
        center((11, 20)),
    ));
    let angle = w
        .avoid_troop(0, 0.0)
        .expect("Should find a passing lane on the road");
    assert!(angle < 0.0, "Use a consistent passing side");
    let straight = w.units[0].pos;
    assert!(w.free_for(
        0,
        (
            straight.0 + angle.cos() * TS * 0.5,
            straight.1 + angle.sin() * TS * 0.5
        )
    ));
}
