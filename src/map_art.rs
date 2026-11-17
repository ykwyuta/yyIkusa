//! 地形テクスチャに描く装飾。地形の通行・視界判定には使わない。

use super::{Field, Map, MH, MW};
use crate::data::Terrain;
use crate::rng::hash2;

const CELL: usize = 96;
const COLS: usize = 4;
const ROWS: usize = 4;
const ATLAS: &[u8; COLS * ROWS * CELL * CELL * 4] =
    include_bytes!("../assets/sprites/map_atlas.rgba");

// tools/make_map_sprites.py の並びに対応。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Prop {
    Pines,
    Deciduous,
    Hedge,
    Reeds,
    Farmhouse,
    Barn,
    CityRuin,
    Rubble,
    AlpineCrag,
    DesertRocks,
    Haystacks,
    BridgeDeck,
    HedgeVertical,
    AlpineRidge,
}

#[derive(Clone, Copy)]
enum Facing {
    Normal,
    Mirrored,
    Vertical,
}

fn choose(field: Field, terrain: Terrain, density: f32, variant: f32) -> Option<(Prop, f32)> {
    use Prop::*;
    Some(match terrain {
        Terrain::Forest if density < 0.80 => {
            let pines = match field {
                Field::Ardennes | Field::Alps => 0.80,
                _ => 0.26,
            };
            if variant < pines {
                (Pines, 1.20)
            } else {
                (Deciduous, 1.12)
            }
        }
        Terrain::Town if field == Field::Stalingrad => {
            if density >= 0.77 {
                return None;
            }
            if variant < 0.67 {
                (CityRuin, 1.12)
            } else {
                (Rubble, 0.90)
            }
        }
        Terrain::Town if density < 0.75 => {
            if variant < 0.53 {
                (Farmhouse, 1.12)
            } else if variant < 0.83 {
                (Barn, 1.03)
            } else {
                (Rubble, 0.61)
            }
        }
        Terrain::Mountain if density < 0.32 => {
            if variant < 0.48 {
                (AlpineCrag, 1.78)
            } else {
                (AlpineRidge, 2.35)
            }
        }
        Terrain::Swamp if density < 0.49 => (Reeds, 0.70),
        Terrain::Sand | Terrain::Hill if field == Field::Alamein && density < 0.086 => {
            (DesertRocks, 0.88)
        }
        Terrain::Plain if matches!(field, Field::Kursk | Field::Normandy) && density < 0.038 => {
            (Haystacks, 0.64)
        }
        _ => return None,
    })
}

pub(super) fn bridge_along_x(map: &Map, x: i32, y: i32) -> bool {
    let routes = |a, b| {
        i32::from(matches!(a, Terrain::Road | Terrain::Bridge))
            + i32::from(matches!(b, Terrain::Road | Terrain::Bridge))
    };
    let wet = |a, b| {
        i32::from(matches!(a, Terrain::Water | Terrain::Ford))
            + i32::from(matches!(b, Terrain::Water | Terrain::Ford))
    };
    let (left, right) = (map.at(x - 1, y), map.at(x + 1, y));
    let (above, below) = (map.at(x, y - 1), map.at(x, y + 1));
    (routes(left, right), wet(above, below)) >= (routes(above, below), wet(left, right))
}

pub(super) fn paint(map: &Map, pixels: &mut [u32], tile_pixels: i32) {
    for y in 0..MH {
        for x in 0..MW {
            let terrain = map.at(x, y);
            if terrain == Terrain::Bridge {
                let inset = tile_pixels / 10;
                let deck = tile_pixels - inset * 2;
                let (rect, facing) = if bridge_along_x(map, x, y) {
                    (
                        [x * tile_pixels, y * tile_pixels + inset, tile_pixels, deck],
                        Facing::Normal,
                    )
                } else {
                    (
                        [x * tile_pixels + inset, y * tile_pixels, deck, tile_pixels],
                        Facing::Vertical,
                    )
                };
                stamp(map, pixels, tile_pixels, Prop::BridgeDeck, rect, facing);
                continue;
            }
            // 左右で種類とサイズをそろえる。装飾用の乱数は地形の生成と独立させる。
            let mirrored = x >= MW / 2;
            let paired_x = x.min(MW - 1 - x);
            let noise = |salt| hash2(paired_x, y, map.seed ^ salt);
            let hedge = terrain == Terrain::Bocage;
            let placement = if hedge {
                let count =
                    |a, b| i32::from(a == Terrain::Bocage) + i32::from(b == Terrain::Bocage);
                let across = count(map.at(x - 1, y), map.at(x + 1, y));
                let along = count(map.at(x, y - 1), map.at(x, y + 1));
                Some((
                    if along > across {
                        Prop::HedgeVertical
                    } else {
                        Prop::Hedge
                    },
                    1.23,
                ))
            } else {
                choose(map.field, terrain, noise(0xa291), noise(0xa292))
            };
            let Some((prop, size)) = placement else {
                continue;
            };
            let size = size * (0.78 + noise(0xa293) * 0.44);
            let scatter = if hedge { 0.10 } else { 0.46 };
            let jitter = (noise(0xa294) - 0.5) * scatter;
            let xx = x as f32 + 0.5 + if mirrored { -jitter } else { jitter };
            let yy = y as f32 + 0.5 + (noise(0xa295) - 0.5) * scatter - size * 0.10;
            let p = tile_pixels as f32;
            let span = (size * p).round().max(1.0) as i32;
            let rect = [
                (xx * p - span as f32 / 2.0).round() as i32,
                (yy * p - span as f32 / 2.0).round() as i32,
                span,
                span,
            ];
            let facing = if mirrored ^ (noise(0xa296) < 0.5) {
                Facing::Mirrored
            } else {
                Facing::Normal
            };
            stamp(map, pixels, tile_pixels, prop, rect, facing);
        }
    }
}

fn stamp(
    map: &Map,
    pixels: &mut [u32],
    tile_pixels: i32,
    prop: Prop,
    rect: [i32; 4],
    facing: Facing,
) {
    let [left, top, width, height] = rect;
    let pitch = MW * tile_pixels;
    let index = prop as usize;
    let (origin_x, origin_y) = (index % COLS * CELL, index / COLS * CELL);
    for y in top.max(0)..(top + height).min(MH * tile_pixels) {
        let v = ((2 * (y - top) + 1) as usize * CELL) / (2 * height as usize);
        for x in left.max(0)..(left + width).min(pitch) {
            let u = ((2 * (x - left) + 1) as usize * CELL) / (2 * width as usize);
            let (u, v) = match facing {
                Facing::Normal => (u, v),
                Facing::Mirrored => (CELL - 1 - u, v),
                Facing::Vertical => (v, CELL - 1 - u),
            };
            let offset = ((origin_y + v) * COLS * CELL + origin_x + u) * 4;
            let alpha = ATLAS[offset + 3] as u32;
            if alpha == 0 {
                continue;
            }
            // 道路や水路の輪郭をまたいで木や瓦礫が描かれないようにする。
            if prop != Prop::BridgeDeck
                && matches!(
                    map.at(x / tile_pixels, y / tile_pixels),
                    Terrain::Road | Terrain::Bridge | Terrain::Water | Terrain::Ford
                )
            {
                continue;
            }
            let pixel = &mut pixels[(y * pitch + x) as usize];
            let blend = |channel: usize, shift: u32| -> u32 {
                let bg = (*pixel >> shift) & 255;
                (ATLAS[offset + channel] as u32 * alpha + bg * (255 - alpha) + 127) / 255
            };
            *pixel = 0xff00_0000 | (blend(2, 16) << 16) | (blend(1, 8) << 8) | blend(0, 0);
        }
    }
}
