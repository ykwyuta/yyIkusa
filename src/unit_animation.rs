//! Eight screen-space facings and distance-driven walk cycles.
use crate::sim::{Kind, Unit};

pub const ATLAS_COLS: usize = 16;
pub const ATLAS_ROWS: usize = 14;
pub const MOTION_START: usize = 16;
pub const SOURCE_DIRECTIONS: usize = 5;
pub const FRAMES: usize = 5;

/// Screen +Y is down. West-facing views mirror their east-facing counterparts.
pub fn facing(angle: f32) -> (usize,bool) {
    let octant=((angle.rem_euclid(std::f32::consts::TAU)+std::f32::consts::FRAC_PI_8)
        /std::f32::consts::FRAC_PI_4).floor() as usize % 8;
    [(0,false),(1,false),(2,false),(1,true),(0,true),(3,true),(4,false),(3,false)][octant]
}

pub fn pose(unit: &Unit) -> (usize,bool) {
    let angle=if unit.moving {unit.motion_heading}
        else if unit.alive && unit.target.is_some() {unit.turret}
        else if unit.walk_distance>0.0 {unit.motion_heading}
        else {unit.hull};
    let (direction,flip)=facing(angle);
    let frame=if unit.alive && unit.kind==Kind::Troop && unit.moving {
        let stride=match unit.ti {0=>2.0,4..=6=>2.2,_=>1.1};
        1+((unit.walk_distance/stride).fract()*4.0).floor() as usize
    }else{0};
    (MOTION_START+unit.ti.min(7)*SOURCE_DIRECTIONS*FRAMES+direction*FRAMES+frame,flip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Nation;
    use crate::sim::{World,Target};

    #[test]
    fn all_eight_screen_directions_use_correct_front_back_and_mirroring() {
        let expected=[(0,false),(1,false),(2,false),(1,true),(0,true),(3,true),(4,false),(3,false)];
        for (i,want) in expected.into_iter().enumerate() {
            let a=i as f32*std::f32::consts::FRAC_PI_4;
            assert_eq!(facing(a),want);
            assert_eq!(facing(a-std::f32::consts::TAU),want);
        }
    }

    #[test]
    fn gait_tracks_distance_and_freezes_on_stop_death_or_decoy() {
        let mut u=World::new_unit(0,Kind::Troop,Nation::Roster,1,(0.0,0.0));
        u.moving=true;
        let base=pose(&u).0;
        for frame in 0..4 {
            u.walk_distance=(frame as f32+0.1)*1.1/4.0;
            assert_eq!(pose(&u).0,base+frame);
        }
        u.moving=false;
        assert_eq!((pose(&u).0-MOTION_START)%FRAMES,0);
        u.moving=true;u.alive=false;
        assert_eq!((pose(&u).0-MOTION_START)%FRAMES,0);
        u.alive=true;u.kind=Kind::Decoy;
        assert_eq!((pose(&u).0-MOTION_START)%FRAMES,0);
    }

    #[test]
    fn actual_travel_overrides_attack_aim_while_walking() {
        let mut u=World::new_unit(0,Kind::Troop,Nation::Roster,2,(0.0,0.0));
        u.moving=true;u.motion_heading=std::f32::consts::FRAC_PI_2;
        u.turret=-std::f32::consts::FRAC_PI_2;u.target=Some(Target::Unit(1));
        assert_eq!((pose(&u).0-MOTION_START)%25/5,2); // south/front
        u.moving=false;
        assert_eq!((pose(&u).0-MOTION_START)%25/5,4); // north/back
        u.target=None;u.hull=0.0;u.walk_distance=1.0;
        assert_eq!((pose(&u).0-MOTION_START)%25/5,2); // stopped: keep last travel heading
    }
}
