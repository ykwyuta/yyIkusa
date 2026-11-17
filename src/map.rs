//! 戦場の地図：史実の部隊戦の舞台を模した地形の生成、経路探索、視線
//!
//! 双方が公平になるよう、地図は左右対称に作る（左半分を作って鏡に映す）。

use crate::data::{Terrain, Weight};
use crate::rng::{fbm, hash2, Rng};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[path = "map_art.rs"]
mod art;

pub const MW: i32 = 84;
pub const MH: i32 = 52;
/// 1マスの大きさ（世界座標）
pub const TS: f32 = 32.0;

/// 戦場
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Kursk,
    Ardennes,
    Alps,
    Alamein,
    Normandy,
    Stalingrad,
}

pub const FIELDS: [Field; 6] = [Field::Kursk, Field::Ardennes, Field::Alps, Field::Alamein, Field::Normandy, Field::Stalingrad];

impl Field {
    pub fn name(self) -> &'static str {
        match self {
            Field::Kursk => t!("草原の会戦", "Steppe", "草原"),
            Field::Ardennes => t!("深い森", "Deep forest", "森林"),
            Field::Alps => t!("山岳の峠", "Mountain passes", "山口"),
            Field::Alamein => t!("砂漠の尾根", "Desert ridges", "沙漠"),
            Field::Normandy => t!("生垣の農村", "Hedgerow villages", "村庄"),
            Field::Stalingrad => t!("城下町", "Castle town", "城下町"),
        }
    }
    pub fn desc(self) -> &'static str {
        match self {
            Field::Kursk => t!(
                "開けた草原と丘、点在する村。北を川が流れる。大砲の長射程が活きる",
                "Open steppe, rolling hills and scattered villages, with a river to the north. Long guns rule here.",
                "开阔草原、丘陵与零星村落，北有河流。长射程大炮有优势"
            ),
            Field::Ardennes => t!(
                "深い森と細い道、中央を川が分ける。森は視界を遮り、象兵と大砲は動きが鈍る",
                "Deep forest and narrow roads, split by a river. Woods block sight and bog down heavy troops.",
                "密林与窄路，河流纵贯中央。森林遮挡视线，象兵与大炮行动迟缓"
            ),
            Field::Alps => t!(
                "山岳は通れない。谷と峠道を巡る争い。湖と森が点在する",
                "Impassable mountains: the fight is for valleys and passes, among lakes and woods.",
                "山岳无法通行，争夺山谷与山口，湖泊和森林点缀其间"
            ),
            Field::Alamein => t!(
                "砂漠で遮る物がほとんどない。尾根を押さえよ。南には塩湿地",
                "Desert with almost no cover. Hold the ridges. Salt marsh lies to the south.",
                "几乎无遮蔽的沙漠。控制山脊，南部是盐沼"
            ),
            Field::Normandy => t!(
                "生垣で区切られた畑が続く。視界が短く、待ち伏せが有利",
                "Fields boxed in by hedgerows. Short sight lines favour ambush.",
                "树篱分隔的田野。视野短，利于伏击"
            ),
            Field::Stalingrad => t!(
                "瓦礫の市街地と大通り。近距離の撃ち合いになる",
                "Ruined city blocks and wide avenues. Expect close-range fights.",
                "废墟街区与大道，近距离交火"
            ),
        }
    }
}

pub struct Map {
    pub field: Field,
    pub t: Vec<Terrain>,
    /// 高さ（陰影用、0..1）
    pub elev: Vec<f32>,
    /// 各陣営の基地（マス）
    pub bases: [(i32, i32); 2],
    pub seed: u32,
}

pub fn idx(x: i32, y: i32) -> usize {
    (y * MW + x) as usize
}

pub fn inside(x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && x < MW && y < MH
}

/// 世界座標 → マス
pub fn tile_of(p: (f32, f32)) -> (i32, i32) {
    (((p.0 / TS).floor() as i32).clamp(0, MW - 1), ((p.1 / TS).floor() as i32).clamp(0, MH - 1))
}

/// マスの中心（世界座標）
pub fn center(t: (i32, i32)) -> (f32, f32) {
    ((t.0 as f32 + 0.5) * TS, (t.1 as f32 + 0.5) * TS)
}

/// 部隊を置ける範囲（基地側の端からのマス数）
pub const DEPLOY: i32 = 20;
/// デコイ・柵を置ける範囲（中央からこのマス数だけ手前まで）
pub const HALF_MARGIN: i32 = 3;

/// 陣営の配置範囲に入っているか
pub fn in_zone(side: usize, x: i32, wide: bool) -> bool {
    let lim = if wide { MW / 2 - HALF_MARGIN } else { DEPLOY };
    if side == 0 {
        x < lim
    } else {
        x >= MW - lim
    }
}

impl Map {
    pub fn at(&self, x: i32, y: i32) -> Terrain {
        if inside(x, y) {
            self.t[idx(x, y)]
        } else {
            Terrain::Water
        }
    }

    pub fn generate(field: Field, seed: u32) -> Map {
        let mut g = Gen { field, seed, rng: Rng::new(seed as u64 ^ 0x5eed), t: vec![Terrain::Plain; (MW * MH) as usize], elev: vec![0.0; (MW * MH) as usize] };
        let by = MH / 2 + g.rng.below(9) as i32 - 4;
        let base = (5, by);
        g.terrain();
        g.roads(base);
        g.features(base);
        // 基地の周りは開けておく
        for y in base.1 - 3..=base.1 + 3 {
            for x in base.0 - 3..=base.0 + 3 {
                if inside(x, y) && (x - base.0).pow(2) + (y - base.1).pow(2) <= 10 {
                    let i = idx(x, y);
                    if !matches!(g.t[i], Terrain::Road | Terrain::Bridge) {
                        g.t[i] = if field == Field::Alamein { Terrain::Sand } else { Terrain::Plain };
                    }
                }
            }
        }
        // 左半分を右へ鏡写しにする
        for y in 0..MH {
            for x in MW / 2..MW {
                let (s, d) = (idx(MW - 1 - x, y), idx(x, y));
                g.t[d] = g.t[s];
                g.elev[d] = g.elev[s];
            }
        }
        let mut m = Map { field, t: g.t, elev: g.elev, bases: [base, (MW - 1 - base.0, base.1)], seed };
        m.ensure_connected();
        m
    }

    /// 基地どうしがつながっていなければ、まっすぐ道を通す
    fn ensure_connected(&mut self) {
        let reach = self.flood(self.bases[0]);
        if reach[idx(self.bases[1].0, self.bases[1].1)] {
            return;
        }
        let y = self.bases[0].1;
        for x in 0..MW {
            let i = idx(x, y);
            self.t[i] = if matches!(self.t[i], Terrain::Water) { Terrain::Bridge } else { Terrain::Road };
        }
    }

    pub fn flood(&self, from: (i32, i32)) -> Vec<bool> {
        let mut seen = vec![false; (MW * MH) as usize];
        let mut st = vec![from];
        seen[idx(from.0, from.1)] = true;
        while let Some((x, y)) = st.pop() {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if inside(nx, ny) && !seen[idx(nx, ny)] && self.at(nx, ny).passable() {
                    seen[idx(nx, ny)] = true;
                    st.push((nx, ny));
                }
            }
        }
        seen
    }

    /// 経路探索（8方向）。cost は追加の費用（None は通れない）。
    /// 着けなければ、最も近づけるマスまでの経路を返す。経路には出発マスを含めない
    pub fn path(&self, from: (i32, i32), to: (i32, i32), w: Weight, extra: &dyn Fn(i32, i32) -> Option<f32>) -> Vec<(i32, i32)> {
        let n = (MW * MH) as usize;
        let mut g = vec![f32::INFINITY; n];
        let mut prev = vec![u32::MAX; n];
        let mut closed = vec![false; n];
        let h = |x: i32, y: i32| {
            let (dx, dy) = ((x - to.0).abs() as f32, (y - to.1).abs() as f32);
            (dx.max(dy) + 0.414 * dx.min(dy)) / 1.35
        };
        let si = idx(from.0, from.1);
        g[si] = 0.0;
        let mut heap = BinaryHeap::new();
        heap.push(Reverse(((h(from.0, from.1) * 100.0) as u32, si as u32)));
        let mut best = (h(from.0, from.1), si);
        let mut found = false;
        let mut expanded = 0;
        while let Some(Reverse((_, ci))) = heap.pop() {
            let ci = ci as usize;
            if closed[ci] {
                continue;
            }
            closed[ci] = true;
            expanded += 1;
            let (cx, cy) = ((ci as i32) % MW, (ci as i32) / MW);
            if (cx, cy) == to {
                found = true;
                best = (0.0, ci);
                break;
            }
            let hc = h(cx, cy);
            if hc < best.0 {
                best = (hc, ci);
            }
            if expanded > 6000 {
                break;
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (nx, ny) = (cx + dx, cy + dy);
                if !inside(nx, ny) {
                    continue;
                }
                let ni = idx(nx, ny);
                if closed[ni] {
                    continue;
                }
                let sp = self.t[ni].speed(w);
                if sp <= 0.0 {
                    continue;
                }
                let Some(add) = extra(nx, ny) else { continue };
                if dx != 0 && dy != 0 {
                    // 角をすり抜けない
                    if !self.at(cx + dx, cy).passable() || !self.at(cx, cy + dy).passable() || extra(cx + dx, cy).is_none() || extra(cx, cy + dy).is_none() {
                        continue;
                    }
                }
                let step = if dx != 0 && dy != 0 { 1.414 } else { 1.0 };
                let ng = g[ci] + step / sp + add;
                if ng < g[ni] {
                    g[ni] = ng;
                    prev[ni] = ci as u32;
                    heap.push(Reverse((((ng + h(nx, ny)) * 100.0) as u32, ni as u32)));
                }
            }
        }
        let _ = found;
        let mut out = Vec::new();
        let mut c = best.1;
        while c != si && c != u32::MAX as usize {
            out.push(((c as i32) % MW, (c as i32) / MW));
            c = prev[c] as usize;
        }
        out.reverse();
        out
    }

    /// 視線が通るか（隊端のマスは数えない）。丘の上からは森・村落越しにも見通しやすい
    pub fn los(&self, a: (i32, i32), b: (i32, i32)) -> bool {
        let high = self.at(a.0, a.1) == Terrain::Hill;
        let (mut x, mut y) = a;
        let (dx, dy) = ((b.0 - a.0).abs(), -(b.1 - a.1).abs());
        let (sx, sy) = (if a.0 < b.0 { 1 } else { -1 }, if a.1 < b.1 { 1 } else { -1 });
        let mut err = dx + dy;
        let mut block = 0;
        loop {
            if (x, y) == b {
                return true;
            }
            if (x, y) != a {
                let t = self.at(x, y);
                let mut k = t.block();
                if high && k < 9 {
                    k /= 2;
                }
                if t == Terrain::Hill && !high && self.at(b.0, b.1) != Terrain::Hill {
                    k += 2;
                }
                block += k;
                if block >= 4 {
                    return false;
                }
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// 地形の絵（1マス P 画素、RGBA）。境目は雑音でずらして自然な形にする
    pub fn render_pixels(&self) -> (u32, u32, Vec<u32>) {
        const P: i32 = 20;
        let (w, h) = ((MW * P) as u32, (MH * P) as u32);
        let mut px = vec![0u32; (w * h) as usize];
        let s = self.seed;
        let pf = P as f32;
        for py in 0..h as i32 {
            for pxx in 0..w as i32 {
                let (fx, fy) = (pxx as f32 / pf, py as f32 / pf);
                let real = self.at(pxx / P, py / P);
                // 境目をずらす（橋はまっすぐ）
                let ox = (fbm(fx * 1.3, fy * 1.3, s ^ 21, 3) - 0.5) * 0.9;
                let oy = (fbm(fx * 1.3, fy * 1.3, s ^ 22, 3) - 0.5) * 0.9;
                let (sx, sy) = ((fx + ox).floor() as i32, (fy + oy).floor() as i32);
                let mut t = if inside(sx, sy) { self.at(sx, sy) } else { real };
                if real == Terrain::Bridge || (t == Terrain::Bridge && real != Terrain::Bridge) {
                    t = real;
                }
                let (tx, ty) = (pxx / P, py / P);
                let n = fbm(fx * 0.9, fy * 0.9, s ^ 77, 3);
                let fine = hash2(pxx, py, s);
                let mut c = base_color(t, self.field);
                let v = 0.9 + n * 0.2 + (fine - 0.5) * 0.05;
                c = [c[0] * v, c[1] * v, c[2] * v];
                let (lx, ly) = ((fx - tx as f32), (fy - ty as f32));
                match t {
                    Terrain::Forest | Terrain::Bocage => {
                        let litter = (fine - 0.5) * 0.06 + (n - 0.5) * 0.12;
                        c = [c[0] + litter, c[1] + litter * 0.65, c[2] + litter * 0.55];
                    }
                    Terrain::Town => {
                        let stone = (hash2(pxx / 2, py / 2, s ^ 31) - 0.5) * 0.16;
                        c = c.map(|channel| channel + stone);
                    }
                    Terrain::Water | Terrain::Ford => {
                        let wave = ((fx * 3.0 + fy * 1.3 + n * 6.0).sin() * 0.5 + 0.5) * 0.07;
                        c = [c[0] + wave, c[1] + wave, c[2] + wave];
                    }
                    Terrain::Bridge => {
                        let along_x = art::bridge_along_x(self, tx, ty);
                        let edge = if along_x { (ly - 0.5).abs() > 0.38 } else { (lx - 0.5).abs() > 0.38 };
                        if edge {
                            c = [0.28, 0.24, 0.20];
                        } else if (if along_x { pxx } else { py }) % 3 == 0 {
                            c = [c[0] * 0.8, c[1] * 0.8, c[2] * 0.8];
                        }
                    }
                    Terrain::Mountain => {
                        let e = self.elev[idx(tx, ty)];
                        let rock = fbm(fx * 2.5, fy * 2.5, s ^ 12, 3);
                        c = [c[0] * (0.75 + rock * 0.5), c[1] * (0.75 + rock * 0.5), c[2] * (0.75 + rock * 0.5)];
                        if self.field == Field::Alps {
                            let snow = ((e + n * 0.25 - 0.77) * 8.0).clamp(0.0, 1.0);
                            let light = 0.63 + rock * 0.25;
                            let crust = [light, light + 0.03, light + 0.07];
                            for i in 0..3 {
                                c[i] += (crust[i] - c[i]) * snow;
                            }
                        }
                    }
                    Terrain::Swamp => {
                        if fbm(fx * 3.0, fy * 3.0, s ^ 3, 2) > 0.56 {
                            c = [0.28, 0.38, 0.42];
                        } else if hash2(pxx / 2, py / 2, s ^ 4) > 0.8 {
                            c = [0.45, 0.50, 0.30];
                        }
                    }
                    Terrain::Plain if self.field == Field::Kursk || self.field == Field::Normandy => {
                        // 畑の筋
                        let k = fbm(fx * 0.35, fy * 0.35, s ^ 51, 2);
                        let stripe = ((fx * 0.9 + fy * if k > 0.5 { 0.3 } else { -0.4 }) * 6.0).sin() * 0.03;
                        c = [c[0] + stripe + (k - 0.5) * 0.12, c[1] + stripe + (k - 0.5) * 0.06, c[2] + stripe];
                    }
                    Terrain::Sand => {
                        let dune = ((fx * 0.8 + fbm(fx * 0.5, fy * 0.5, s ^ 61, 2) * 4.0).sin()) * 0.04;
                        c = [c[0] + dune, c[1] + dune, c[2] + dune];
                    }
                    _ => {}
                }
                // 起伏の陰影（北西から光）
                let e = |x: f32, y: f32| {
                    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
                    if inside(xi, yi) {
                        self.elev[idx(xi, yi)]
                    } else {
                        0.0
                    }
                };
                let shade = (e(fx - 0.7, fy) - e(fx + 0.7, fy) + e(fx, fy - 0.7) - e(fx, fy + 0.7)) * 0.3;
                let shade = 1.0 + shade.clamp(-0.22, 0.22);
                let to8 = |v: f32| (v * shade).clamp(0.0, 1.0) * 255.0;
                px[(py as u32 * w + pxx as u32) as usize] = 0xff00_0000 | ((to8(c[2]) as u32) << 16) | ((to8(c[1]) as u32) << 8) | to8(c[0]) as u32;
            }
        }
        art::paint(self, &mut px, P);
        (w, h, px)
    }
}

/// 地形の色
pub fn base_color(t: Terrain, f: Field) -> [f32; 3] {
    match t {
        Terrain::Plain => match f {
            Field::Kursk => [0.58, 0.60, 0.33],
            Field::Alamein => [0.70, 0.62, 0.42],
            Field::Alps => [0.42, 0.55, 0.30],
            Field::Stalingrad => [0.50, 0.48, 0.42],
            _ => [0.44, 0.54, 0.30],
        },
        Terrain::Road => [0.62, 0.55, 0.42],
        Terrain::Forest => [0.20, 0.34, 0.17],
        Terrain::Hill => match f {
            Field::Alamein => [0.66, 0.54, 0.36],
            _ => [0.52, 0.52, 0.34],
        },
        Terrain::Swamp => [0.36, 0.42, 0.28],
        Terrain::Town => [0.40, 0.37, 0.33],
        Terrain::Water => [0.22, 0.36, 0.50],
        Terrain::Ford => [0.34, 0.48, 0.55],
        Terrain::Bridge => [0.50, 0.40, 0.30],
        Terrain::Mountain => [0.50, 0.48, 0.46],
        Terrain::Sand => [0.80, 0.70, 0.48],
        Terrain::Bocage => [0.42, 0.52, 0.28],
    }
}

/// 地図を作る途中の状態（左半分だけ作る）
struct Gen {
    field: Field,
    seed: u32,
    rng: Rng,
    t: Vec<Terrain>,
    elev: Vec<f32>,
}

impl Gen {
    fn set(&mut self, x: i32, y: i32, t: Terrain) {
        if inside(x, y) && x < MW / 2 {
            self.t[idx(x, y)] = t;
        }
    }
    fn get(&self, x: i32, y: i32) -> Terrain {
        if inside(x, y) {
            self.t[idx(x, y)]
        } else {
            Terrain::Water
        }
    }

    /// 雑音から地形の下地を作る
    fn terrain(&mut self) {
        let s = self.seed;
        // (森, 丘, 湿地) のしきい値。1 を超えると現れない
        let (forest, hill, swamp, base) = match self.field {
            Field::Kursk => (0.70, 0.60, 0.74, Terrain::Plain),
            Field::Ardennes => (0.43, 0.62, 0.78, Terrain::Plain),
            Field::Alps => (0.55, 0.50, 0.80, Terrain::Plain),
            Field::Alamein => (1.1, 0.64, 1.1, Terrain::Sand),
            Field::Normandy => (0.70, 0.70, 0.80, Terrain::Plain),
            Field::Stalingrad => (1.1, 1.1, 1.1, Terrain::Town),
        };
        for y in 0..MH {
            for x in 0..MW / 2 {
                let (fx, fy) = (x as f32, y as f32);
                let e = fbm(fx * 0.07, fy * 0.07, s ^ 1, 4);
                let f = fbm(fx * 0.11, fy * 0.11, s ^ 2, 4);
                let w = fbm(fx * 0.13, fy * 0.13, s ^ 3, 3);
                self.elev[idx(x, y)] = e;
                let mut t = base;
                if self.field == Field::Alps {
                    // 尾根状の雑音で山脈を作る
                    let r = 1.0 - (fbm(fx * 0.06, fy * 0.06, s ^ 4, 4) - 0.5).abs() * 2.0;
                    let m = r * 0.6 + e * 0.5;
                    self.elev[idx(x, y)] = m.min(1.0);
                    if m > 0.78 {
                        t = Terrain::Mountain;
                    } else if m > 0.68 {
                        t = Terrain::Hill;
                    } else if f > forest {
                        t = Terrain::Forest;
                    } else if w > 0.76 {
                        t = Terrain::Water;
                    }
                } else if self.field == Field::Stalingrad {
                    // 公園と広場、瓦礫の丘
                    if f > 0.66 {
                        t = Terrain::Plain;
                    } else if e > 0.70 {
                        t = Terrain::Hill;
                    }
                } else {
                    if e > hill {
                        t = Terrain::Hill;
                    }
                    if f > forest {
                        t = Terrain::Forest;
                    }
                    if w > swamp && e < 0.5 {
                        t = Terrain::Swamp;
                    }
                    if self.field == Field::Alamein && t == Terrain::Sand && f > 0.62 {
                        // 固い地面
                        t = Terrain::Plain;
                    }
                }
                self.t[idx(x, y)] = t;
            }
        }
    }

    /// 道を引く：基地から中央へ、主街道と脇道
    fn roads(&mut self, base: (i32, i32)) {
        let n = match self.field {
            Field::Ardennes | Field::Alps => 3,
            Field::Stalingrad => 0,
            _ => 3,
        };
        if n == 0 { return; }
        let mut ends = vec![base.1];
        let others = [(MH / 5) + self.rng.below(5) as i32, MH - MH / 5 - self.rng.below(5) as i32];
        ends.extend_from_slice(&others[..(n - 1).min(2)]);
        for (k, &ey) in ends.iter().enumerate() {
            let mut y = base.1 as f32;
            let start = if k == 0 { base.0 } else { base.0 + 2 + self.rng.below(4) as i32 };
            let mut prev = base.1;
            for x in start..MW / 2 {
                let tx = (x - start) as f32 / (MW / 2 - start) as f32;
                let goal = base.1 as f32 + (ey - base.1) as f32 * (tx * 1.6).min(1.0);
                // 中央付近では横にまっすぐ（鏡写しでつながるように）
                let wobble = if x >= MW / 2 - 3 { 0.0 } else { (self.rng.f32() - 0.5) * 0.9 };
                y += (goal - y) * 0.25 + wobble;
                let yi = (y.round() as i32).clamp(1, MH - 2);
                // 縦のつながりを道で埋める
                for yy in prev.min(yi)..=prev.max(yi) {
                    self.road_tile(x, yy);
                }
                prev = yi;
            }
            // 脇道は基地から縦に伸ばしてつなぐ
            if k > 0 {
                let (a, b) = (base.1.min(ey), base.1.max(ey));
                for yy in a..=b {
                    self.road_tile(start, yy);
                }
            }
        }
        if self.field == Field::Stalingrad {
            // 碁盤の目の大通り
            for y in (3..MH).step_by(7) {
                for x in 0..MW / 2 {
                    self.road_tile(x, y);
                }
            }
            for x in (4..MW / 2).step_by(8) {
                for y in 0..MH {
                    self.road_tile(x, y);
                }
            }
            for x in 0..MW / 2 {
                self.road_tile(x, base.1);
            }
        }
        if self.field == Field::Alamein {
            // 海岸道路と北の海
            for x in 0..MW / 2 {
                for y in 0..3 {
                    self.set(x, y, Terrain::Water);
                }
                self.road_tile(x, 4);
            }
        }
    }

    fn road_tile(&mut self, x: i32, y: i32) {
        if !inside(x, y) || x >= MW / 2 {
            return;
        }
        let t = self.get(x, y);
        let nt = match t {
            Terrain::Water | Terrain::Ford => Terrain::Bridge,
            Terrain::Bridge => Terrain::Bridge,
            _ => Terrain::Road,
        };
        self.set(x, y, nt);
    }

    /// 川・村・生垣など戦場ごとの特徴
    fn features(&mut self, base: (i32, i32)) {
        match self.field {
            Field::Kursk => {
                // 北を東西に流れる川（プショール川）、浅瀬が2か所
                self.river_ew(MH / 6 + 1, 2);
                self.villages(5, 2);
                // 浅い谷筋（バルカ）
                for _ in 0..2 {
                    let x0 = 12 + self.rng.below(24) as i32;
                    let mut y = MH as f32 * 0.55 + self.rng.range(-8.0, 8.0);
                    for x in x0..(x0 + 8).min(MW / 2) {
                        y += self.rng.range(-0.8, 0.8);
                        if !matches!(self.get(x, y as i32), Terrain::Road | Terrain::Bridge) {
                            self.set(x, y as i32, Terrain::Swamp);
                        }
                    }
                }
            }
            Field::Ardennes => {
                self.river_ns();
                self.villages(4, 1);
            }
            Field::Alps => {
                self.villages(3, 1);
            }
            Field::Alamein => {
                // 南の塩湿地（カッターラ低地）
                for y in MH - 7..MH {
                    for x in 0..MW / 2 {
                        let k = fbm(x as f32 * 0.2, y as f32 * 0.2, self.seed ^ 8, 3);
                        if k + (y - (MH - 7)) as f32 * 0.08 > 0.6 && !matches!(self.get(x, y), Terrain::Road | Terrain::Bridge) {
                            self.set(x, y, Terrain::Swamp);
                        }
                    }
                }
                self.villages(1, 1);
            }
            Field::Normandy => {
                self.bocage(base);
                self.villages(4, 1);
            }
            Field::Stalingrad => {
                // 工場地帯の間を流れる涸れ谷
                let y0 = MH / 3;
                for x in 0..MW / 2 {
                    let y = y0 + ((x as f32 * 0.3).sin() * 2.0) as i32;
                    if !matches!(self.get(x, y), Terrain::Road | Terrain::Bridge) {
                        self.set(x, y, Terrain::Swamp);
                    }
                }
            }
        }
    }

    /// 東西に流れる川（左右対称なので中央でつながる）
    fn river_ew(&mut self, y0: i32, fords: usize) {
        let mut ford_x: Vec<i32> = (0..fords).map(|_| 10 + self.rng.below((MW / 2 - 14) as u32) as i32).collect();
        ford_x.sort();
        for x in 0..MW / 2 {
            let y = y0 as f32 + (x as f32 * 0.21).sin() * 2.2 + fbm(x as f32 * 0.15, 0.0, self.seed ^ 6, 2) * 3.0;
            let y = y as i32;
            for yy in y..y + 2 {
                let t = self.get(x, yy);
                let nt = if matches!(t, Terrain::Road | Terrain::Bridge) {
                    Terrain::Bridge
                } else if ford_x.iter().any(|&f| (f - x).abs() <= 1) {
                    Terrain::Ford
                } else {
                    Terrain::Water
                };
                self.set(x, yy, nt);
            }
        }
    }

    /// 中央を南北に流れる川
    fn river_ns(&mut self) {
        let ford_y = 6 + self.rng.below((MH - 12) as u32) as i32;
        for y in 0..MH {
            let half = 1.0 + fbm(0.0, y as f32 * 0.2, self.seed ^ 7, 2) * 1.6;
            for x in (MW / 2 - 3)..MW / 2 {
                if (MW / 2) as f32 - 0.5 - x as f32 <= half {
                    let t = self.get(x, y);
                    let nt = if matches!(t, Terrain::Road | Terrain::Bridge) {
                        Terrain::Bridge
                    } else if (y - ford_y).abs() <= 1 {
                        Terrain::Ford
                    } else {
                        Terrain::Water
                    };
                    self.set(x, y, nt);
                }
            }
        }
    }

    /// 道沿いに村を置く
    fn villages(&mut self, n: usize, r: i32) {
        let roads: Vec<(i32, i32)> = (0..MH).flat_map(|y| (8..MW / 2 - 2).map(move |x| (x, y))).filter(|&(x, y)| self.get(x, y) == Terrain::Road).collect();
        if roads.is_empty() {
            return;
        }
        for _ in 0..n {
            let (cx, cy) = roads[self.rng.below(roads.len() as u32) as usize];
            let rr = r + self.rng.below(2) as i32;
            for y in cy - rr..=cy + rr {
                for x in cx - rr - 1..=cx + rr + 1 {
                    if inside(x, y) && matches!(self.get(x, y), Terrain::Plain | Terrain::Forest | Terrain::Hill | Terrain::Sand | Terrain::Bocage) && self.rng.chance(0.8) {
                        self.set(x, y, Terrain::Town);
                    }
                }
            }
        }
    }

    /// 生垣で畑を区切る（切れ目を残す）
    fn bocage(&mut self, base: (i32, i32)) {
        let (cw, ch) = (6, 5);
        let ox = self.rng.below(cw as u32) as i32;
        let oy = self.rng.below(ch as u32) as i32;
        for y in 0..MH {
            for x in 0..MW / 2 {
                let on_v = (x + ox) % cw == 0;
                let on_h = (y + oy) % ch == 0;
                if !(on_v || on_h) {
                    continue;
                }
                if (x - base.0).abs() < 4 && (y - base.1).abs() < 4 {
                    continue;
                }
                // 区画の辺ごとに切れ目を作る
                let seg = if on_v { hash2((x + ox) / cw, (y + oy) / ch * 7 + (y + oy) % ch, self.seed) } else { hash2((x + ox) / cw * 5 + (x + ox) % cw, (y + oy) / ch, self.seed ^ 1) };
                let gate = if on_v { (y + oy) % ch == 2 } else { (x + ox) % cw == 3 };
                if gate && hash2(x / cw, y / ch, self.seed ^ 13) > 0.25 {
                    continue;
                }
                if seg < 0.12 {
                    continue;
                }
                if matches!(self.get(x, y), Terrain::Plain | Terrain::Hill) {
                    self.set(x, y, Terrain::Bocage);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_is_symmetric_and_connected() {
        for f in FIELDS {
            for seed in 1..6 {
                let m = Map::generate(f, seed * 7919);
                let r = m.flood(m.bases[0]);
                assert!(r[idx(m.bases[1].0, m.bases[1].1)], "{:?} {}", f, seed);
                for y in 0..MH {
                    for x in 0..MW {
                        assert_eq!(m.at(x, y), m.at(MW - 1 - x, y));
                    }
                }
                let p = m.path(m.bases[0], m.bases[1], Weight::Heavy, &|_, _| Some(0.0));
                assert_eq!(p.last().copied(), Some(m.bases[1]), "{:?}", f);
            }
        }
    }

    #[test]
    fn fields_have_distinct_character() {
        let count = |f: Field, t: Terrain| {
            let m = Map::generate(f, 4242);
            m.t.iter().filter(|&&x| x == t).count()
        };
        assert!(count(Field::Ardennes, Terrain::Forest) > 3 * count(Field::Kursk, Terrain::Forest).max(1));
        assert!(count(Field::Alps, Terrain::Mountain) > 100);
        assert!(count(Field::Alamein, Terrain::Sand) > 1000);
        assert!(count(Field::Normandy, Terrain::Bocage) > 150);
        assert!(count(Field::Stalingrad, Terrain::Town) > 1500);
    }
}
