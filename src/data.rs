//! 国と部隊の諸元、地形の性質
//!
//! 防御・突破力はゲーム用の抽象値。射程・速度はマス単位。

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nation { Roster }

pub const NATIONS: [Nation; 1] = [Nation::Roster];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weight {
    Light,
    Medium,
    Heavy,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role { Standard }

impl Role {
    pub fn tag(self) -> &'static str {
        match self {
            Role::Standard => "",
        }
    }
}

/// 部隊の諸元
pub struct TroopSpec {
    pub name: [&'static str; 3],
    pub cost: u32,
    pub hp: f32,
    /// 正面・側面装甲（背面は側面の7割）
    pub front: f32,
    pub side: f32,
    /// 至近距離での貫通力
    pub pen: f32,
    pub dmg: f32,
    /// 装填（秒）
    pub reload: f32,
    pub range: f32,
    /// 平地での速度（マス/秒）
    pub speed: f32,
    pub sight: f32,
    pub weight: Weight,
    pub role: Role,
    /// 描画の寸法（マス）
    pub len: f32,
    pub wid: f32,
    /// 旋回砲塔があるか（無ければ車体を向けて撃つ）
    pub turret: bool,
    pub gun: &'static str,
}

impl TroopSpec {
    pub fn name(&self) -> &'static str {
        t!(self.name[0], self.name[1], self.name[2])
    }
    pub fn rear(&self) -> f32 {
        self.side * 0.7
    }
    /// 砲身の長さ（車体長に対する割合）
    pub fn barrel(&self) -> f32 {
        (0.35 + self.pen / 400.0).min(0.85)
    }
}

macro_rules! troop {
    ($ja:expr, $en:expr, $zh:expr, cost $c:expr, hp $hp:expr, armor $f:expr, $s:expr, pen $p:expr, dmg $d:expr, reload $r:expr, range $rg:expr, speed $sp:expr, sight $si:expr, $w:ident, $role:ident, size $l:expr, $wd:expr, turret $t:expr, gun $g:expr) => {
        TroopSpec { name: [$ja, $en, $zh], cost: $c, hp: $hp, front: $f, side: $s, pen: $p, dmg: $d, reload: $r, range: $rg, speed: $sp, sight: $si, weight: Weight::$w, role: Role::$role, len: $l, wid: $wd, turret: $t, gun: $g }
    };
}

static UNITS: [TroopSpec; 8] = [
    troop!("象兵", "War elephants", "象兵", cost 720, hp 240.0, armor 65.0, 40.0, pen 95.0, dmg 65.0, reload 3.5, range 1.9, speed 1.1, sight 12.0, Heavy, Standard, size 1.30, 0.85, turret true, gun "牙・踏みつけ"),
    troop!("歩兵", "Infantry", "步兵", cost 100, hp 90.0, armor 45.0, 25.0, pen 85.0, dmg 24.0, reload 1.4, range 1.6, speed 1.6, sight 11.0, Light, Standard, size 0.70, 0.45, turret true, gun "槍"),
    troop!("鉄砲隊", "Musketeers", "火枪兵", cost 240, hp 65.0, armor 20.0, 12.0, pen 115.0, dmg 38.0, reload 4.8, range 11.0, speed 1.4, sight 13.0, Light, Standard, size 0.70, 0.45, turret true, gun "火縄銃"),
    troop!("大砲", "Cannon", "大炮", cost 820, hp 105.0, armor 30.0, 18.0, pen 180.0, dmg 88.0, reload 7.0, range 18.0, speed 0.8, sight 12.0, Heavy, Standard, size 1.00, 0.65, turret true, gun "野戦砲"),
    troop!("騎馬槍隊", "Lancer cavalry", "骑马枪兵", cost 350, hp 125.0, armor 60.0, 35.0, pen 110.0, dmg 40.0, reload 2.1, range 1.8, speed 3.1, sight 14.0, Medium, Standard, size 1.00, 0.55, turret true, gun "騎槍"),
    troop!("騎馬鉄砲隊", "Mounted musketeers", "骑马火枪兵", cost 430, hp 90.0, armor 30.0, 18.0, pen 105.0, dmg 30.0, reload 4.2, range 9.0, speed 2.8, sight 14.0, Medium, Standard, size 1.00, 0.55, turret true, gun "騎馬銃"),
    troop!("騎馬弓隊", "Horse archers", "骑马弓兵", cost 300, hp 80.0, armor 25.0, 15.0, pen 65.0, dmg 17.0, reload 1.8, range 8.0, speed 3.2, sight 15.0, Light, Standard, size 1.00, 0.55, turret true, gun "騎弓"),
    troop!("弓隊", "Archers", "弓兵", cost 150, hp 60.0, armor 18.0, 10.0, pen 60.0, dmg 19.0, reload 2.0, range 10.0, speed 1.5, sight 12.0, Light, Standard, size 0.70, 0.45, turret true, gun "弓"),
];

impl Nation {
    pub fn flag(self) -> [[f32;4];3] { [[0.6,0.2,0.15,1.0],[0.95,0.83,0.5,1.0],[0.6,0.2,0.15,1.0]] }
    pub fn name(self) -> &'static str { t!("兵種", "Troops", "兵种") }
    pub fn blurb(self) -> &'static str { t!("8兵種を自由に編成。歩兵と象兵で前線を支え、弓・鉄砲・大砲と騎兵で攻める", "Combine eight troop types: melee lines, ranged support and mobile cavalry.", "自由组合八种部队：近战前线、远程支援与机动骑兵") }
    pub fn troops(self) -> &'static [TroopSpec] { &UNITS }

}

/// 予算規模
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Budget {
    Small,
    Medium,
    Large,
}

pub const BUDGETS: [Budget; 3] = [Budget::Small, Budget::Medium, Budget::Large];

impl Budget {
    pub fn points(self) -> u32 {
        match self {
            Budget::Small => 3000,
            Budget::Medium => 6000,
            Budget::Large => 11000,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Budget::Small => t!("小", "Small", "小"),
            Budget::Medium => t!("中", "Medium", "中"),
            Budget::Large => t!("大", "Large", "大"),
        }
    }
}

/// 全ての国の部隊（国, 番号）
pub fn all_troops() -> impl Iterator<Item = (Nation, usize)> {
    NATIONS.into_iter().flat_map(|n| (0..n.troops().len()).map(move |i| (n, i)))
}

pub const DECOY_COST: u32 = 30;
pub const FENCE_COST: u32 = 20;
pub const FENCE_HP: f32 = 220.0;

/// 地形
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Terrain {
    Plain,
    Road,
    Forest,
    Hill,
    Swamp,
    Town,
    Water,
    Ford,
    Bridge,
    /// 山岳（部隊は通れず、視線を遮る）
    Mountain,
    /// 砂地（やや遅い）
    Sand,
    /// 生垣（遅く、視線を遮る）
    Bocage,
}

impl Terrain {
    /// 進撃速度の倍率（重い部隊ほど悪路で遅い）。0 は通れない
    pub fn speed(self, w: Weight) -> f32 {
        let (l, m, h) = match self {
            Terrain::Plain => (1.0, 1.0, 1.0),
            Terrain::Road | Terrain::Bridge => (1.35, 1.35, 1.3),
            Terrain::Forest => (0.55, 0.45, 0.32),
            Terrain::Hill => (0.8, 0.68, 0.52),
            Terrain::Swamp => (0.42, 0.3, 0.17),
            Terrain::Town => (0.7, 0.62, 0.5),
            Terrain::Ford => (0.42, 0.36, 0.28),
            Terrain::Sand => (0.85, 0.78, 0.66),
            Terrain::Bocage => (0.38, 0.32, 0.24),
            Terrain::Water | Terrain::Mountain => (0.0, 0.0, 0.0),
        };
        match w {
            Weight::Light => l,
            Weight::Medium => m,
            Weight::Heavy => h,
        }
    }
    pub fn passable(self) -> bool {
        !matches!(self, Terrain::Water | Terrain::Mountain)
    }
    /// 中にいる部隊を隠す（見つけにくく、当たりにくい）
    pub fn cover(self) -> bool {
        matches!(self, Terrain::Forest | Terrain::Town | Terrain::Bocage)
    }
    /// 視線を遮る度合い
    pub fn block(self) -> u32 {
        match self {
            Terrain::Forest => 2,
            Terrain::Town => 2,
            Terrain::Bocage => 2,
            Terrain::Mountain => 9,
            _ => 0,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Terrain::Plain => t!("平地", "Open ground", "平地"),
            Terrain::Road => t!("道路", "Road", "道路"),
            Terrain::Forest => t!("森", "Forest", "森林"),
            Terrain::Hill => t!("丘", "Hill", "丘陵"),
            Terrain::Swamp => t!("湿地", "Marsh", "沼泽"),
            Terrain::Town => t!("村落", "Village", "村落"),
            Terrain::Water => t!("川", "River", "河流"),
            Terrain::Ford => t!("浅瀬", "Ford", "浅滩"),
            Terrain::Bridge => t!("橋", "Bridge", "桥"),
            Terrain::Mountain => t!("山岳", "Mountains", "山岳"),
            Terrain::Sand => t!("砂地", "Sand", "沙地"),
            Terrain::Bocage => t!("生垣", "Hedgerow", "树篱"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_troop_roles_have_distinct_strengths() {
        let u=Nation::Roster.troops();
        assert_eq!(all_troops().count(),8);
        assert!(u[0].hp > u[1].hp * 2.0);
        assert!(u[3].range > u[2].range && u[2].pen > u[7].pen);
        assert!(u[4].speed > u[1].speed && u[6].speed > u[7].speed);
        assert!(u[1].range < 2.0 && u[4].range < 2.0);
        for (i,s) in u.iter().enumerate() {
            assert!(s.cost>0 && s.reload>0.0 && s.hp>0.0);
            for t in &u[i+1..] { assert_ne!(s.name,t.name); }
        }
    }

    #[test]
    fn terrain_slows_heavier_troops_more() {
        for t in [Terrain::Forest, Terrain::Swamp, Terrain::Hill, Terrain::Town] {
            assert!(t.speed(Weight::Heavy) < t.speed(Weight::Light));
            assert!(t.speed(Weight::Medium) < Terrain::Plain.speed(Weight::Medium));
        }
        assert_eq!(Terrain::Water.speed(Weight::Light), 0.0);
    }
}
