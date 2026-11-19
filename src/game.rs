//! 画面と操作：タイトル（戦場・予算・難易度の選択）、準備（購入と配置）、戦闘、結果
//!
//! 世界（地図・部隊）は WebGL / Direct3D の三角形で、文字と画面部品は Canvas2D / Direct2D で描く。

use crate::ai::{self, Commander};
use crate::data::{Budget, Nation, TroopSpec, Weight, BUDGETS, DECOY_COST, NATIONS, FENCE_COST};
use crate::general_radio::{self, Cue, Radio, EXPRESSION_SIZE, EXPRESSIONS};
use crate::generals::{self, Gender, GeneralCommand, GENERALS, PORTRAIT_SIZE};
use crate::gfx::{Bitmap, Font, Gfx, Texture};

use crate::lang;
use crate::map::{center, idx, in_zone, tile_of, Field, FIELDS, MH, MW, TS};
use crate::sim::{dist, EndReason, FxKind, Kind, Order, Phase, Target, Unit, World, CAPTURE_R, DECOY_REVEAL, HAWK_SIGHT, DT, TIME_LIMIT};
use crate::Ev;

struct DelegateStroke {
    last: Option<(f32, f32)>,
    targets: Vec<usize>,
}

// Segment distance catches units even when pointer events are sparse.
fn segment_hits(p: (f32, f32), a: (f32, f32), b: (f32, f32), radius: f32) -> bool {
    let delta = (b.0 - a.0, b.1 - a.1);
    let length_sq = delta.0 * delta.0 + delta.1 * delta.1;
    let t = if length_sq > 0.0 {
        (((p.0 - a.0) * delta.0 + (p.1 - a.1) * delta.1) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    dist(p, (a.0 + t * delta.0, a.1 + t * delta.1)) < radius
}

type C = [f32; 4];

const TEXT: C = [0.94, 0.91, 0.82, 1.0];
const DIM: C = [0.70, 0.68, 0.60, 1.0];
const GOLD: C = [0.98, 0.82, 0.42, 1.0];
const PANEL: C = [0.07, 0.08, 0.06, 0.90];
const EDGE: C = [0.55, 0.50, 0.34, 1.0];
const BLUE: C = [0.35, 0.62, 1.0, 1.0];
const RED: C = [1.0, 0.36, 0.30, 1.0];
const GOOD: C = [0.55, 0.92, 0.55, 1.0];
const TITLE_SIZE: (u32, u32) = (1280, 720);
const TITLE_RGB: &[u8; (TITLE_SIZE.0 * TITLE_SIZE.1 * 3) as usize] =
    include_bytes!("../assets/branding/title.rgb");
const TITLE_MARK: &[u8; 128 * 128 * 3] = include_bytes!("../assets/branding/mark.rgb");

fn side_color(side: usize) -> C {
    if side == 0 {
        BLUE
    } else {
        RED
    }
}

fn with_a(c: C, a: f32) -> C {
    [c[0], c[1], c[2], c[3] * a]
}

fn shade(c: C, k: f32) -> C {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    Title,
    Battle,
}

struct TitleLayout {
    compact: bool,
    left: f32,
    top: f32,
    menu_width: f32,
    preview: [f32; 4],
}

impl TitleLayout {
    fn new(gfx: &Gfx) -> Self {
        let (w, h) = (gfx.width as f32, gfx.height as f32);
        let compact = gfx.compact || w < 1080.0 || h < 610.0;
        let left = (w * 0.054).clamp(16.0, 76.0);
        let top = if compact { (h * 0.045).clamp(18.0, 30.0) } else { (h * 0.065).clamp(34.0, 64.0) };
        let menu_width = if compact { (w * 0.57).min(610.0) } else { (w * 0.46).min(640.0) };
        let preview_width = if compact { (w * 0.26).min(250.0) } else { (menu_width * 0.42).clamp(200.0,250.0) };
        let right = w - left;
        let bottom = h - if compact { 22.0 } else { 38.0 };
        let preview_height = (preview_width - 12.0) * MH as f32 / MW as f32 + 70.0;
        let preview_left = if h >= 820.0 { left } else { right - preview_width };
        let preview = [preview_left, bottom - preview_height, preview_left + preview_width, bottom];
        Self { compact, left, top, menu_width, preview }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Troop(Nation, usize),
    Decoy,
    Fence,
    Remove,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Act {
    Field(usize),
    Budget(usize),
    Diff(usize),
    Reroll,
    #[cfg(target_arch = "wasm32")]
    ClearCache,
    Lang,
    Fullscreen,
    Start,
    Tab(usize),
    Tool(Tool),
    Auto,
    Clear,
    Begin,
    Generals,
    AssignGeneral(Option<usize>),
    ReturnGeneral(usize),
    ActivateSkill(usize),
    SkipCutin,
    DelegateSelected(usize),
    GeneralFocus((i32, i32)),
    Pause,
    Speed(u32),
    Help,
    Menu,
    AttackMode,
    Stop,
    Hawk,
    HawkPatrol,
    AllTroops,
    Again,
    ToTitle,
}

struct Btn {
    r: [f32; 4],
    act: Act,
}

struct LogLine {
    text: String,
    age: f32,
    good: bool,
}

pub struct Game {
    gfx: Gfx,
    screen: Screen,
    field_i: usize,
    budget_i: usize,
    diff_i: usize,
    seed: u32,
    world: World,
    commander: Commander,
    general_command: GeneralCommand,
    enemy_general_command: GeneralCommand,
    radio: Radio,
    cutins: crate::skill_cutin::Cutins,
    cutin_art: Option<(usize, Bitmap)>,
    show_generals: bool,
    assign_to: Option<usize>,
    portraits: [Option<Bitmap>; GENERALS.len()],
    expressions: [[Option<Bitmap>; EXPRESSIONS]; GENERALS.len()],
    portrait_gen: Option<u32>,
    terrain: Option<Texture>,
    terrain_key: (Field, u32),
    fog: Option<Texture>,
    fog_px: Vec<u32>,
    atlas: Option<Texture>,
    title_art: Option<Texture>,
    title_mark: Option<Texture>,
    cam: (f32, f32),
    zoom: f32,
    mouse: (f32, f32),
    ldown: Option<(f32, f32)>,
    delegate_stroke: Option<DelegateStroke>,
    mdown: Option<((f32, f32), (f32, f32))>,
    painting: bool,
    sel: Vec<usize>,
    buttons: Vec<Btn>,
    panels: Vec<[f32; 4]>,
    minimap: [f32; 4],
    tool: Tool,
    tab: usize,
    mimic: (Nation, usize),
    paused: bool,
    speed: u32,
    acc: f32,
    attack_mode: bool,
    log: Vec<LogLine>,
    show_help: bool,
    touch: bool,
    clock: f32,
    keys: [bool; 4],
    patrol_key_down: bool,
    marker: Option<((f32, f32), f32, bool)>,
    menu_confirm: f32,
    notice: Option<(String, f32)>,
    /// ミニマップに出す出来事の場所
    pings: Vec<((f32, f32), f32)>,
}

fn diff_name(i: usize) -> &'static str {
    match i {
        0 => t!("易しい", "Easy", "简单"),
        1 => t!("普通", "Normal", "普通"),
        _ => t!("難しい", "Hard", "困难"),
    }
}

fn weight_name(w: Weight) -> &'static str {
    match w {
        Weight::Light => t!("軽", "Light", "轻"),
        Weight::Medium => t!("中", "Medium", "中"),
        Weight::Heavy => t!("重", "Heavy", "重"),
    }
}

impl Game {
    pub fn new(gfx: Gfx) -> Game {
        let seed = crate::platform::random_u32() | 1;
        let world = World::new(Field::Kursk, seed, Budget::Medium.points());
        let commander = Commander::new(1, seed, 1.0);
        let title_pixels: Vec<_> = TITLE_RGB.chunks_exact(3)
            .map(|p| u32::from_le_bytes([p[0], p[1], p[2], 255]))
            .collect();
        let title_art = gfx.create_texture(TITLE_SIZE.0, TITLE_SIZE.1, &title_pixels).ok();
        let mark_pixels: Vec<_> = TITLE_MARK.chunks_exact(3)
            .map(|p| u32::from_le_bytes([p[0],p[1],p[2],255])).collect();
        let title_mark = gfx.create_texture(128,128,&mark_pixels).ok();
        let mut g = Game {
            gfx,
            screen: Screen::Title,
            field_i: 0,
            budget_i: 1,
            diff_i: 1,
            seed,
            world,
            commander,
            general_command: GeneralCommand::default(),
            enemy_general_command: GeneralCommand::new(1),
            radio: Radio::default(),
            cutins: crate::skill_cutin::Cutins::default(),
            cutin_art: None,
            show_generals: false,
            assign_to: Some(0),
            portraits: std::array::from_fn(|_| None),
            expressions: std::array::from_fn(|_| std::array::from_fn(|_| None)),
            portrait_gen: None,
            terrain: None,
            terrain_key: (Field::Kursk, 0),
            fog: None,
            fog_px: vec![0; (MW * MH) as usize],
            atlas: None,
            title_art,
            title_mark,
            cam: (MW as f32 * TS / 2.0, MH as f32 * TS / 2.0),
            zoom: 0.5,
            mouse: (0.0, 0.0),
            ldown: None,
            delegate_stroke: None,
            mdown: None,
            painting: false,
            sel: Vec::new(),
            buttons: Vec::new(),
            panels: Vec::new(),
            minimap: [0.0; 4],
            tool: Tool::Troop(Nation::Roster, 1),
            tab: 0,
            mimic: (Nation::Roster, 3),
            paused: false,
            speed: 1,
            acc: 0.0,
            attack_mode: false,
            log: Vec::new(),
            show_help: false,
            touch: false,
            clock: 0.0,
            keys: [false; 4],
            patrol_key_down: false,
            marker: None,
            menu_confirm: 0.0,
            notice: None,
            pings: Vec::new(),
        };
        g.ensure_terrain();
        g
    }

    #[cfg_attr(windows, allow(dead_code))]
    pub fn apply_loaded(&mut self, _data: &[u8]) {}

    pub fn notify(&mut self, s: &str) {
        self.notice = Some((s.to_string(), 4.0));
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn state_text(&self) -> String {
        format!("screen={:?} field={:?} phase={:?} t={:.0} units={}", self.screen, self.world.map.field, self.world.phase, self.world.time, self.world.units.len())
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn crash_bytes(&self) -> Option<Vec<u8>> {
        None
    }

    /// タッチした場所に何があるか：0=地図 1=画面部品 2=ミニマップ 5=委任をなぞる地図
    #[cfg_attr(windows, allow(dead_code))]
    pub fn hit_kind(&self, x: f32, y: f32) -> u32 {
        let (x, y) = (x / self.gfx.ui, y / self.gfx.ui);
        if self.screen == Screen::Title || self.show_help || self.world.phase == Phase::Over {
            return 1;
        }
        let inr = |r: &[f32; 4]| x >= r[0] && y >= r[1] && x <= r[2] && y <= r[3];
        if inr(&self.minimap) {
            return 2;
        }
        if self.buttons.iter().any(|b| inr(&b.r)) || self.panels.iter().any(inr) {
            return 1;
        }
        if self.show_generals { 5 } else { 0 }
    }

    fn field(&self) -> Field {
        FIELDS[self.field_i]
    }

    /// 地形の絵を作り直す（戦場か種が変わったとき）
    fn ensure_terrain(&mut self) {
        let key = (self.world.map.field, self.world.map.seed);
        if self.terrain.is_some() && self.terrain_key == key {
            return;
        }
        let (w, h, px) = self.world.map.render_pixels();
        if let Some(terrain) = &self.terrain {
            self.gfx.update_texture(terrain, 0, 0, w, h, &px);
        } else {
            self.terrain = self.gfx.create_texture(w, h, &px).ok();
        }
        self.terrain_key = key;
        if self.fog.is_none() {
            self.fog = self.gfx.create_texture(MW as u32, MH as u32, &self.fog_px).ok();
        }
        if self.atlas.is_none() {
            if let Some((aw, ah, apx)) = decode_atlas_rle(OBLIQUE_ATLAS_RLE) {
                self.atlas = self.gfx.create_texture(aw, ah, &apx).ok();
            }
        }
    }

    /// タイトル画面の地図を選んだ戦場で作り直す
    fn regen_preview(&mut self) {
        self.world = World::new(self.field(), self.seed, BUDGETS[self.budget_i].points());
        self.ensure_terrain();
    }

    fn start_prep(&mut self) {
        let budget = BUDGETS[self.budget_i].points();
        self.world = World::new(self.field(), self.seed, budget);
        let (skill, aggr) = match self.diff_i {
            0 => (0.8, 0.7),
            1 => (1.0, 1.0),
            _ => (1.12, 1.3),
        };
        self.world.skill = [1.0, skill];
        ai::prepare(&mut self.world, 1, self.seed ^ 0x51ed);
        self.commander = Commander::new(1, self.seed ^ 0xc0, aggr);
        self.general_command = GeneralCommand::default();
        self.enemy_general_command = GeneralCommand::new(1);
        self.radio = Radio::default();
        self.cutins = crate::skill_cutin::Cutins::default();
        self.show_generals = false;
        self.assign_to = Some(0);
        self.ldown = None;
        self.delegate_stroke = None;
        self.screen = Screen::Battle;
        self.sel.clear();
        self.log.clear();
        self.paused = false;
        self.speed = 1;
        self.acc = 0.0;
        self.show_help = false;
        self.tool = Tool::Troop(NATIONS[self.tab], 1);
        self.ensure_terrain();
        // 自陣の配置範囲を映す
        let b = self.world.base_pos(0);
        self.zoom = (self.gfx.width as f32 / (34.0 * TS)).clamp(0.3, 2.0);
        self.cam = (b.0 + 10.0 * TS, b.1);
        self.clamp_cam();
        self.push_log(t!("準備：予算の範囲で部隊を買って自陣に配置し、デコイと柵を置く", "Preparation: buy troops within budget and deploy them, then place decoys and palisades", "准备：在预算内购买部队部署于己方阵地，并布置诱饵和栅栏").to_string(), true);
    }

    fn push_log(&mut self, text: String, good: bool) {
        self.log.push(LogLine { text, age: 0.0, good });
        if self.log.len() > 40 {
            self.log.remove(0);
        }
    }

    // ------------------------------------------------------------ 座標

    fn to_world(&self, p: (f32, f32)) -> (f32, f32) {
        let (w, h) = (self.gfx.width as f32, self.gfx.height as f32);
        (self.cam.0 + (p.0 - w / 2.0) / self.zoom, self.cam.1 + (p.1 - h / 2.0) / self.zoom)
    }

    fn to_screen(&self, p: (f32, f32)) -> (f32, f32) {
        let (w, h) = (self.gfx.width as f32, self.gfx.height as f32);
        ((p.0 - self.cam.0) * self.zoom + w / 2.0, (p.1 - self.cam.1) * self.zoom + h / 2.0)
    }

    fn min_zoom(&self) -> f32 {
        (self.gfx.width as f32 / (MW as f32 * TS)).min(self.gfx.height as f32 / (MH as f32 * TS)) * 0.9
    }

    fn clamp_cam(&mut self) {
        self.zoom = self.zoom.clamp(self.min_zoom(), 3.0);
        let (mw, mh) = (MW as f32 * TS, MH as f32 * TS);
        self.cam.0 = self.cam.0.clamp(0.0, mw);
        self.cam.1 = self.cam.1.clamp(0.0, mh);
    }

    // ------------------------------------------------------------ 1フレーム

    pub fn frame(&mut self, evs: &[Ev], dt: f32, focused: bool) -> bool {
        for n in crate::platform::take_notes() {
            self.notify(&n);
        }
        self.clock += dt;
        let ui = self.gfx.ui;
        for e in evs {
            let e = match *e {
                Ev::MouseMove(x, y) => Ev::MouseMove(x / ui, y / ui),
                Ev::Down(b, x, y) => Ev::Down(b, x / ui, y / ui),
                Ev::Up(b, x, y) => Ev::Up(b, x / ui, y / ui),
                o => o,
            };
            self.handle(e);
        }
        if self.screen == Screen::Battle {
            self.scroll(dt, focused);
            let running = !self.paused && !self.show_help;
            if running && self.world.phase != Phase::Over {
                self.acc += dt * self.speed as f32;
                let mut steps = 0;
                while self.acc >= DT && steps < 12 {
                    self.commander.update(&mut self.world, DT);
                    self.general_command.update(&mut self.world);
                    self.enemy_general_command.update(&mut self.world);
                    self.world.step();
                    self.acc -= DT;
                    steps += 1;
                }
                if steps >= 12 {
                    self.acc = 0.0;
                }
            } else if self.world.phase == Phase::Over {
                // 結果画面でも煙などは動かす
                self.world.step();
            }
            self.radio.update(&self.world, dt, !running);
            self.cutins.update(if self.show_help { 0.0 } else { dt });
            for cue in std::mem::take(&mut self.world.skill_cues) {
                self.cutins.push(cue);
            }
            for e in std::mem::take(&mut self.world.events) {
                if let Some(p) = e.pos {
                    self.pings.push((p, 0.0));
                }
                self.push_log(e.text, e.side == 1);
            }
            for p in self.pings.iter_mut() {
                p.1 += dt;
            }
            self.pings.retain(|p| p.1 < 2.5);
            let w = &self.world;
            self.sel.retain(|&i| w.units.get(i).is_some_and(Unit::manually_controlled));
            for l in self.log.iter_mut() {
                l.age += dt;
            }
        }
        if let Some((_, t)) = &mut self.notice {
            *t -= dt;
            if *t <= 0.0 {
                self.notice = None;
            }
        }
        if let Some(m) = &mut self.marker {
            m.1 -= dt;
            if m.1 <= 0.0 {
                self.marker = None;
            }
        }
        self.menu_confirm = (self.menu_confirm - dt).max(0.0);
        self.render();
        false
    }

    fn scroll(&mut self, dt: f32, focused: bool) {
        let sp = 900.0 / self.zoom * dt;
        let (w, h) = (self.gfx.width as f32, self.gfx.height as f32);
        let mut d = (0.0, 0.0);
        if self.keys[0] {
            d.0 -= sp;
        }
        if self.keys[1] {
            d.1 -= sp;
        }
        if self.keys[2] {
            d.0 += sp;
        }
        if self.keys[3] {
            d.1 += sp;
        }
        if focused && !self.touch && self.mdown.is_none() && self.ldown.is_none() {
            let m = self.mouse;
            let e = 4.0;
            if m.0 <= e {
                d.0 -= sp;
            } else if m.0 >= w - e {
                d.0 += sp;
            }
            if m.1 <= e {
                d.1 -= sp;
            } else if m.1 >= h - e {
                d.1 += sp;
            }
        }
        if d != (0.0, 0.0) {
            self.cam.0 += d.0;
            self.cam.1 += d.1;
            self.clamp_cam();
        }
    }

    // ------------------------------------------------------------ 入力

    fn button_at(&self, p: (f32, f32)) -> Option<Act> {
        self.buttons.iter().rev().find(|b| p.0 >= b.r[0] && p.0 <= b.r[2] && p.1 >= b.r[1] && p.1 <= b.r[3]).map(|b| b.act)
    }

    fn over_ui(&self, p: (f32, f32)) -> bool {
        let inr = |r: &[f32; 4]| p.0 >= r[0] && p.1 >= r[1] && p.0 <= r[2] && p.1 <= r[3];
        self.buttons.iter().any(|b| inr(&b.r)) || self.panels.iter().any(inr)
    }

    fn in_minimap(&self, p: (f32, f32)) -> bool {
        let r = self.minimap;
        self.screen == Screen::Battle && p.0 >= r[0] && p.0 <= r[2] && p.1 >= r[1] && p.1 <= r[3]
    }

    fn minimap_jump(&mut self, p: (f32, f32)) {
        let r = self.minimap;
        let fx = ((p.0 - r[0]) / (r[2] - r[0])).clamp(0.0, 1.0);
        let fy = ((p.1 - r[1]) / (r[3] - r[1])).clamp(0.0, 1.0);
        self.cam = (fx * MW as f32 * TS, fy * MH as f32 * TS);
        self.clamp_cam();
    }

    fn handle(&mut self, e: Ev) {
        match e {
            Ev::Resize(w, h) => {
                self.gfx.resize(w, h);
                self.clamp_cam();
            }
            Ev::Compact(c) => self.gfx.set_compact(c),
            Ev::Touch(t) => self.touch = t,
            Ev::Scroll(_) | Ev::Char(_) => {}
            Ev::MouseMove(x, y) => {
                self.mouse = (x, y);
                if let Some((m0, c0)) = self.mdown {
                    self.cam = (c0.0 - (x - m0.0) / self.zoom, c0.1 - (y - m0.1) / self.zoom);
                    self.clamp_cam();
                }
                if self.ldown.is_some() && self.in_minimap((x, y)) && self.ldown.is_some_and(|d| self.in_minimap(d)) {
                    self.minimap_jump((x, y));
                }
                self.trace_delegation((x, y));
                if self.painting && self.world.phase == Phase::Prep {
                    let p = self.to_world((x, y));
                    self.world.place_fences(0, p);
                }
            }
            Ev::Wheel(d) => {
                if self.screen != Screen::Battle {
                    return;
                }
                let before = self.to_world(self.mouse);
                self.zoom = (self.zoom * 1.15f32.powf(d)).clamp(self.min_zoom(), 3.0);
                let after = self.to_world(self.mouse);
                self.cam.0 += before.0 - after.0;
                self.cam.1 += before.1 - after.1;
                self.clamp_cam();
            }
            Ev::Down(b, x, y) => self.down(b, (x, y)),
            Ev::Up(b, x, y) => self.up(b, (x, y)),
            Ev::Key(vk, down) => self.key(vk, down),
        }
    }

    fn down(&mut self, b: u8, p: (f32, f32)) {
        self.mouse = p;
        if b == 2 {
            self.mdown = Some((p, self.cam));
            return;
        }
        if b == 0 {
            if let Some(a) = self.button_at(p) {
                self.act(a);
                return;
            }
            if self.screen != Screen::Battle || self.show_help || self.world.phase == Phase::Over {
                return;
            }
            if self.in_minimap(p) {
                self.ldown = Some(p);
                self.minimap_jump(p);
                return;
            }
            if self.over_ui(p) {
                return;
            }
            let wp = self.to_world(p);
            match self.world.phase {
                Phase::Prep if self.show_generals => self.start_delegation(p),
                Phase::Prep => self.apply_tool(wp),
                Phase::Battle if self.show_generals => self.start_delegation(p),
                Phase::Battle => {
                    if self.attack_mode && !self.sel.is_empty() {
                        self.command(wp, true);
                        self.attack_mode = false;
                    } else {
                        self.ldown = Some(p);
                    }
                }
                Phase::Over => {}
            }
        } else if b == 1 {
            if self.screen != Screen::Battle || self.show_help || (self.over_ui(p) && !self.in_minimap(p)) {
                return;
            }
            let wp = if self.in_minimap(p) {
                let r = self.minimap;
                (((p.0 - r[0]) / (r[2] - r[0])) * MW as f32 * TS, ((p.1 - r[1]) / (r[3] - r[1])) * MH as f32 * TS)
            } else {
                self.to_world(p)
            };
            match self.world.phase {
                Phase::Prep => {
                    if !self.show_generals {
                        self.world.remove_at(0, wp);
                    }
                }
                Phase::Battle => {
                    let am = self.attack_mode;
                    self.command(wp, am);
                    self.attack_mode = false;
                }
                Phase::Over => {}
            }
        }
    }

    fn up(&mut self, b: u8, p: (f32, f32)) {
        self.mouse = p;
        if b == 2 {
            self.mdown = None;
            return;
        }
        if b != 0 {
            return;
        }
        self.painting = false;
        self.trace_delegation(p);
        let stroke = self.delegate_stroke.take();
        let Some(d) = self.ldown.take() else { return };
        if self.in_minimap(d) {
            return;
        }
        let drag = (p.0 - d.0).abs() > 8.0 || (p.1 - d.1).abs() > 8.0;
        if matches!(self.world.phase, Phase::Prep | Phase::Battle)
            && self.show_generals
            && !self.show_help
        {
            if let Some(stroke) = stroke {
                self.delegate_targets(stroke.targets);
            } else if !self.over_ui(p) {
                self.delegate_at(self.to_world(d), self.to_world(p), drag);
            }
            return;
        }
        if self.world.phase != Phase::Battle {
            return;
        }
        if drag {
            // 範囲選択
            let (a, c) = (self.to_world(d), self.to_world(p));
            let (x0, x1) = (a.0.min(c.0), a.0.max(c.0));
            let (y0, y1) = (a.1.min(c.1), a.1.max(c.1));
            let picked: Vec<usize> = (0..self.world.units.len())
                .filter(|&i| {
                    let u = &self.world.units[i];
                    u.manually_controlled() && u.kind == Kind::Troop && u.pos.0 >= x0 && u.pos.0 <= x1 && u.pos.1 >= y0 && u.pos.1 <= y1
                })
                .collect();
            self.sel = picked;
            return;
        }
        let wp = self.to_world(p);
        if let Some(i) = self.own_at(wp) {
            self.sel = vec![i];
            return;
        }
        if self.touch && !self.sel.is_empty() {
            // タッチ：選んでいる部隊への命令
            let am = self.attack_mode;
            self.command(wp, am);
            self.attack_mode = false;
            return;
        }
        self.sel.clear();
    }

    fn key(&mut self, vk: u32, down: bool) {
        if vk == 0x50 {
            if down && !self.patrol_key_down && self.screen == Screen::Battle {
                self.act(Act::HawkPatrol);
            }
            self.patrol_key_down = down;
            return;
        }
        match vk {
            0x25 => self.keys[0] = down,
            0x26 => self.keys[1] = down,
            0x27 => self.keys[2] = down,
            0x28 => self.keys[3] = down,
            _ => {}
        }
        if !down {
            return;
        }
        match vk {
            0x1b => {
                if self.show_help {
                    self.show_help = false;
                } else if self.cutins.current().is_some() {
                    self.cutins.skip();
                } else if self.show_generals {
                    self.act(Act::Generals);
                } else if self.attack_mode {
                    self.attack_mode = false;
                } else {
                    self.sel.clear();
                }
            }
            0x70 | 0x48 => self.act(Act::Help),
            0x7a => self.act(Act::Fullscreen),
            _ => {}
        }
        if self.screen != Screen::Battle {
            if vk == 0x0d {
                self.act(Act::Start);
            }
            return;
        }
        match vk {
            0x20 => self.act(Act::Pause),
            0x41 => self.act(Act::AttackMode),
            0x53 => self.act(Act::Stop),
            0x44 => self.act(Act::Hawk),
            0x47 => self.act(Act::Generals),
            0x51 => self.act(Act::AllTroops),
            0xbb | 0x6b => self.speed = (self.speed + 1).min(4),
            0xbd | 0x6d => self.speed = self.speed.saturating_sub(1).max(1),
            0x2e => {
                if self.world.phase == Phase::Prep {
                    self.tool = Tool::Remove;
                }
            }
            _ => {}
        }
    }

    fn own_at(&self, wp: (f32, f32)) -> Option<usize> {
        let r = TS * 0.65;
        (0..self.world.units.len())
            .filter(|&i| {
                let u = &self.world.units[i];
                u.manually_controlled() && dist(u.pos, wp) < r
            })
            .min_by(|&a, &b| dist(self.world.units[a].pos, wp).partial_cmp(&dist(self.world.units[b].pos, wp)).unwrap_or(std::cmp::Ordering::Equal))
    }

    fn selected_direct_troops(&self) -> Vec<usize> {
        self.sel
            .iter()
            .copied()
            .filter(|&i| {
                self.world
                    .units
                    .get(i)
                    .is_some_and(|u| u.kind == Kind::Troop && u.manually_controlled())
            })
            .collect()
    }

    // Starting on a unit paints a path; starting on empty ground keeps mouse box selection.
    fn start_delegation(&mut self, p: (f32, f32)) {
        self.ldown = Some(p);
        let wp = self.to_world(p);
        let on_unit = self.world.units.iter().any(|u| {
            u.alive && u.side == 0 && u.kind == Kind::Troop && dist(u.pos, wp) < TS * 0.70
        });
        self.delegate_stroke = (self.touch || on_unit).then_some(DelegateStroke {
            last: Some(wp),
            targets: Vec::new(),
        });
        self.trace_delegation(p);
    }

    fn trace_delegation(&mut self, p: (f32, f32)) {
        if self.screen != Screen::Battle || !self.show_generals || self.show_help
            || !matches!(self.world.phase, Phase::Prep | Phase::Battle)
        {
            return;
        }
        // Break the path at UI so returning to the map cannot select through a panel.
        if self.over_ui(p) || self.in_minimap(p) {
            if let Some(stroke) = &mut self.delegate_stroke {
                stroke.last = None;
            }
            return;
        }
        let wp = self.to_world(p);
        if let Some(stroke) = &mut self.delegate_stroke {
            for (i, u) in self.world.units.iter().enumerate() {
                if u.alive && u.side == 0 && u.kind == Kind::Troop
                    && segment_hits(u.pos, stroke.last.unwrap_or(wp), wp, TS * 0.70)
                    && !stroke.targets.contains(&i)
                {
                    stroke.targets.push(i);
                }
            }
            stroke.last = Some(wp);
        }
    }

    fn delegate_at(&mut self, from: (f32, f32), to: (f32, f32), drag: bool) {
        let eligible = self.world.units.iter().enumerate().filter(|(_, u)| {
            u.alive && u.side == 0 && u.kind == Kind::Troop
        });
        let targets: Vec<usize> = if drag {
            let (x0, x1) = (from.0.min(to.0), from.0.max(to.0));
            let (y0, y1) = (from.1.min(to.1), from.1.max(to.1));
            eligible
                .filter(|(_, u)| (x0..=x1).contains(&u.pos.0) && (y0..=y1).contains(&u.pos.1))
                .map(|(i, _)| i)
                .collect()
        } else {
            eligible
                .map(|(i, u)| (i, dist(u.pos, to)))
                .filter(|(_, d)| *d < TS * 0.70)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| vec![i])
                .unwrap_or_default()
        };
        self.delegate_targets(targets);
    }

    fn delegate_targets(&mut self, targets: Vec<usize>) {
        for i in targets {
            match self.world.phase {
                Phase::Prep => {
                    self.world.assign_general(i, self.assign_to);
                }
                Phase::Battle => {
                    if self.world.delegate_in_battle(i, self.assign_to) && self.assign_to.is_none() && !self.sel.contains(&i) {
                        self.sel.push(i);
                    }
                }
                Phase::Over => {}
            }
        }
        if self.world.phase == Phase::Battle {
            let w = &self.world;
            self.sel.retain(|&u| w.units.get(u).is_some_and(Unit::manually_controlled));
        }
    }

    fn apply_tool(&mut self, wp: (f32, f32)) {
        let left = self.world.left(0);
        let ok = match self.tool {
            Tool::Troop(n, i) => {
                let ok = self.world.place_troop(0, n, i, wp);
                if !ok && n.troops()[i].cost > left {
                    self.notify(t!("予算が足りない", "Not enough budget", "预算不足"));
                }
                ok
            }
            Tool::Decoy => {
                let (n, i) = self.mimic;
                self.world.place_decoy(0, n, i, wp)
            }
            Tool::Fence => {
                self.painting = true;
                self.world.place_fences(0, wp)
            }
            Tool::Remove => self.world.remove_at(0, wp),
        };
        if !ok && matches!(self.tool, Tool::Troop(..)) && self.world.left(0) >= self.tool_cost() {
            let t = tile_of(wp);
            if !in_zone(0, t.0, false) {
                self.notify(t!("部隊は自陣の配置範囲（明るい帯）にだけ置ける", "Troops can only be deployed in your deployment zone (the bright strip)", "部队只能部署在己方部署区（亮色区域）"));
            }
        }
        if !ok && matches!(self.tool, Tool::Decoy | Tool::Fence) && self.world.left(0) < self.tool_cost() {
            self.notify(t!("予算が足りない", "Not enough budget", "预算不足"));
        }
    }

    fn tool_cost(&self) -> u32 {
        match self.tool {
            Tool::Troop(n, i) => n.troops()[i].cost,
            Tool::Decoy => DECOY_COST,
            Tool::Fence => FENCE_COST,
            Tool::Remove => 0,
        }
    }

    /// 選んだ部隊への命令（右クリック・タップ）
    fn command(&mut self, wp: (f32, f32), attack: bool) {
        if self.sel.is_empty() {
            return;
        }
        // 敵の部隊（デコイかもしれない）
        let enemy = (0..self.world.units.len()).filter(|&j| self.world.targetable_by(0, j) && dist(self.world.units[j].pos, wp) < TS * 0.75).min_by(|&a, &b| dist(self.world.units[a].pos, wp).partial_cmp(&dist(self.world.units[b].pos, wp)).unwrap_or(std::cmp::Ordering::Equal));
        let t = tile_of(wp);
        let fences = {
            let k = self.world.fences_at[idx(t.0, t.1)];
            (k != usize::MAX && self.world.fences[k].alive && self.world.fences[k].side == 1 && self.world.fences[k].seen[0]).then_some(k)
        };
        let target = enemy.map(Target::Unit).or(fences.map(Target::Fence));
        let sel: Vec<_> = self.sel.iter().copied()
            .filter(|&i| self.world.units.get(i).is_some_and(Unit::manually_controlled))
            .collect();
        if sel.is_empty() {
            return;
        }
        let troops: Vec<usize> = sel.iter().copied().filter(|&i| self.world.units[i].kind == Kind::Troop).collect();
        for &i in &sel {
            if self.world.units[i].kind == Kind::Hawk {
                self.world.order(i, Order::Move { goal: wp, attack: false });
            }
        }
        if let Some(t) = target {
            for &i in &troops {
                self.world.order(i, Order::Attack(t));
            }
            self.marker = Some((wp, 0.7, true));
            return;
        }
        // 隊形：目標の周りに並べる
        let n = troops.len();
        let cols = (n as f32).sqrt().ceil() as usize;
        let mut sorted = troops.clone();
        sorted.sort_by(|&a, &b| {
            let (pa, pb) = (self.world.units[a].pos, self.world.units[b].pos);
            (pa.1 + pa.0 * 0.01).partial_cmp(&(pb.1 + pb.0 * 0.01)).unwrap_or(std::cmp::Ordering::Equal)
        });
        for (k, &i) in sorted.iter().enumerate() {
            let (r, c) = (k / cols.max(1), k % cols.max(1));
            let off = ((c as f32 - (cols as f32 - 1.0) / 2.0) * 1.1 * TS, (r as f32 - ((n / cols.max(1)) as f32) / 2.0) * 1.1 * TS);
            let mut g = (wp.0 + off.1, wp.1 + off.0);
            let gt = tile_of(g);
            if !self.world.map.at(gt.0, gt.1).passable() {
                g = wp;
            }
            self.world.order(i, Order::Move { goal: g, attack });
        }
        self.marker = Some((wp, 0.7, attack));
    }

    fn act(&mut self, a: Act) {
        match a {
            Act::Field(i) => {
                self.field_i = i;
                self.regen_preview();
            }
            Act::Budget(i) => self.budget_i = i,
            Act::Diff(i) => self.diff_i = i,
            Act::Reroll => {
                self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223) | 1;
                self.regen_preview();
            }
            #[cfg(target_arch = "wasm32")]
            Act::ClearCache => {
                if self.screen == Screen::Title {
                    crate::platform::refresh_cache();
                }
            }
            Act::Lang => {
                let l = lang::lang().next();
                lang::set_lang(l);
                crate::platform::save_lang(l);
                self.gfx.reload_fonts();
            }
            Act::Fullscreen => self.gfx.toggle_fullscreen(),
            Act::Start => {
                if self.screen == Screen::Title {
                    self.start_prep();
                }
            }
            Act::Tab(i) => {
                self.tab = i;
                if let Tool::Troop(..) = self.tool {
                    self.tool = Tool::Troop(NATIONS[i], 0);
                }
            }
            Act::Tool(t) => {
                if let Tool::Troop(n, i) = t {
                    self.mimic = (n, i);
                }
                self.tool = t;
            }
            Act::Auto => {
                self.world.clear_side(0);
                ai::prepare(&mut self.world, 0, self.seed ^ (self.clock * 1000.0) as u32);
            }
            Act::Clear => self.world.clear_side(0),
            Act::Generals if matches!(self.world.phase, Phase::Prep | Phase::Battle) => {
                self.show_generals = !self.show_generals;
                self.ldown = None;
                self.delegate_stroke = None;
                self.painting = false;
                self.attack_mode = false;
                if self.show_generals && self.world.phase == Phase::Battle {
                    let focused_active = self.assign_to.is_some_and(|id| {
                        self.world.adjutants[id]
                            || self.world.units.iter().any(|u| u.alive && u.side == 0 && u.general == Some(id))
                    });
                    if !focused_active {
                        if let Some(id) = self
                            .world
                            .units
                            .iter()
                            .find_map(|u| (u.alive && u.side == 0).then_some(u.general).flatten())
                            .or_else(|| self.world.adjutants.iter().position(|&a| a))
                        {
                            self.assign_to = Some(id);
                        }
                    }
                }
            }
            Act::AssignGeneral(id)
                if matches!(self.world.phase, Phase::Prep | Phase::Battle) && self.show_generals =>
            {
                self.assign_to = id;
            }
            Act::SkipCutin => self.cutins.skip(),
            Act::ActivateSkill(id) => {
                self.world.activate_skill(0, id);
            }
            Act::ReturnGeneral(id) if self.world.phase == Phase::Battle => {
                let freed = self.world.return_general_command(id);
                if !freed.is_empty() {
                    let n = freed.len();
                    self.sel = freed;
                    self.assign_to = Some(id);
                    self.push_log(
                        tf!(
                            "{} が指揮権を返還し、副官に着任（{}隊を直轄へ）",
                            "{} returned command as adjutant ({} troops to direct control)",
                            "{} 交还指挥权并转为副官（{} 队归入直属）",
                            GENERALS[id].name(),
                            n
                        ),
                        true,
                    );
                }
            }
            Act::DelegateSelected(id) if self.world.phase == Phase::Battle => {
                let troops = self.selected_direct_troops();
                let mut count = 0;
                for i in troops {
                    if self.world.delegate_in_battle(i, Some(id)) {
                        count += 1;
                    }
                }
                if count > 0 {
                    let w = &self.world;
                    self.sel.retain(|&u| w.units.get(u).is_some_and(Unit::manually_controlled));
                    self.assign_to = Some(id);
                    self.push_log(
                        tf!(
                            "{} に {}隊の指揮を委任",
                            "Delegated {} troops to {}",
                            "向 {} 委任了 {} 队部队",
                            GENERALS[id].name(),
                            count
                        ),
                        true,
                    );
                }
            }
            Act::Generals
            | Act::AssignGeneral(_)
            | Act::ReturnGeneral(_)
            | Act::DelegateSelected(_) => {}
            Act::GeneralFocus(tile) => {
                let point = center(tile);
                self.cam = point;
                self.pings.push((point, 0.0));
                self.clamp_cam();
            }
            Act::Begin => {
                if self.world.phase != Phase::Prep {
                    return;
                }
                if self.world.troop_count(0) == 0 {
                    self.notify(t!("部隊を1隊以上配置してください", "Deploy at least one troop", "请至少部署一队部队"));
                    return;
                }
                self.show_generals = false;
                self.ldown = None;
                self.delegate_stroke = None;
                generals::delegate_enemy(&mut self.world, self.seed);
                self.world.start_battle();
                self.act(Act::AllTroops);
            }
            Act::Pause => self.paused = !self.paused,
            Act::Speed(s) => self.speed = s,
            Act::Help => self.show_help = !self.show_help,
            Act::Menu => {
                if self.menu_confirm > 0.0 || self.world.phase == Phase::Over {
                    self.screen = Screen::Title;
                    self.cutins = crate::skill_cutin::Cutins::default();
                    self.regen_preview();
                } else {
                    self.menu_confirm = 3.0;
                }
            }
            Act::AttackMode => {
                if !self.sel.is_empty() {
                    self.attack_mode = !self.attack_mode;
                }
            }
            Act::Stop => {
                for &i in &self.sel {
                    if self.world.units.get(i).is_some_and(Unit::manually_controlled) {
                        self.world.order(i, Order::Idle);
                    }
                }
            }
            Act::Hawk => {
                if let Some(d) = self.world.hawk(0) {
                    self.sel = vec![d];
                    self.cam = self.world.units[d].pos;
                    self.clamp_cam();
                }
            }
            Act::HawkPatrol => {
                let enabled = !self.world.hawk_patrolling(0);
                self.world.set_hawk_patrol(0, enabled);
            }
            Act::AllTroops => {
                self.sel = self.world.units.iter().enumerate()
                    .filter(|(_, u)| u.kind == Kind::Troop && u.manually_controlled())
                    .map(|(i, _)| i).collect();
            }
            Act::Again => {
                self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223) | 1;
                self.start_prep();
            }
            Act::ToTitle => {
                self.screen = Screen::Title;
                self.cutins = crate::skill_cutin::Cutins::default();
                self.regen_preview();
            }
        }
    }

    // ------------------------------------------------------------ 描画

    fn render(&mut self) {
        self.gfx.begin([0.08, 0.09, 0.07, 1.0]);
        self.ensure_terrain();
        let title = (self.screen == Screen::Title).then(|| TitleLayout::new(&self.gfx));
        if let Some(layout) = &title {
            self.render_title_background(layout);
        } else {
            self.render_world();
            self.render_minimap();
        }
        if self.gfx.d2d_begin() {
            self.buttons.clear();
            self.panels.clear();
            if let Some(layout) = &title {
                if self.show_help {
                    self.render_help();
                } else {
                    self.render_title(layout);
                }
            } else {
                self.render_labels();
                self.render_hud();
                if self.world.phase == Phase::Prep {
                    if self.show_generals { self.render_generals(); }
                    else { self.render_shop(); }
                } else {
                    self.render_selection();
                    if self.world.phase == Phase::Battle && self.show_generals {
                        self.render_generals();
                    } else if !self.show_help {
                        self.render_general_report();
                    }
                }
                if self.show_help {
                    self.render_help();
                }
                if self.world.phase == Phase::Over {
                    self.render_over();
                }
            }
            if self.screen == Screen::Battle && !self.show_help {
                self.render_skill_cutin();
            }
            if let Some((s, t)) = &self.notice {
                let (s, a) = (s.clone(), t.min(1.0));
                let w = self.gfx.width as f32;
                let tw = self.gfx.measure(&s, Font::Big) + 40.0;
                let y = self.gfx.height as f32 * 0.24;
                self.gfx.fill_round(w / 2.0 - tw / 2.0, y, w / 2.0 + tw / 2.0, y + 40.0, 8.0, with_a(PANEL, a));
                self.gfx.text(&s, w / 2.0 - tw / 2.0, y, tw, 40.0, Font::BigCenter, with_a(GOLD, a));
            }
            self.gfx.d2d_end();
        }
        self.gfx.present();
    }

    fn set_world_xform(&mut self) {
        let (w, h) = (self.gfx.width as f32, self.gfx.height as f32);
        let z = self.zoom;
        let sx = 2.0 * z / w;
        let sy = -2.0 * z / h;
        self.gfx.set_xform(sx, sy, -self.cam.0 * sx, -self.cam.1 * sy);
    }

    fn render_title_background(&mut self, layout: &TitleLayout) {
        let g = &mut self.gfx;
        let (w, h) = (g.width as f32, g.height as f32);
        g.set_xform(2.0 / w, -2.0 / h, -1.0, 1.0);
        if let Some(art) = &self.title_art {
            let scale = (w / art.w as f32).max(h / art.h as f32);
            let (aw, ah) = (art.w as f32 * scale, art.h as f32 * scale);
            let (x, y) = ((w - aw) * 0.70, (h - ah) * 0.50);
            g.draw_textured(art, x, y, x + aw, y + ah, [1.0; 4], true);
        }
        g.rect(0.0, 0.0, w * 0.49, h, [0.01,0.025,0.045,0.20]);
        // Small drifting embers give the otherwise still illustration a little life.
        for i in 0..24 {
            let seed=i as f32;
            let x=w*(0.46+(seed*0.173+self.clock*0.006).fract()*0.53);
            let y=h*(1.0-(seed*0.271+self.clock*(0.017+seed*0.0003)).fract());
            let glow=0.12+0.18*(self.clock*1.3+seed).sin().abs();
            g.circle(x,y,if i%3==0 {1.8}else{1.0},6,[1.0,0.64,0.22,glow]);
        }
        if let Some(mark)=&self.title_mark {
            let size=if layout.compact {50.0}else{66.0};
            g.draw_textured(mark,layout.left,layout.top+15.0,layout.left+size,layout.top+15.0+size,[1.0;4],true);
        }
        let [x0, y0, x1, y1] = layout.preview;
        g.rect(x0, y0, x1, y1, [0.02, 0.045, 0.065, 0.95]);
        if let Some(map) = &self.terrain {
            g.draw_textured(map, x0 + 6.0, y0 + 28.0, x1 - 6.0, y1 - 42.0, [1.0; 4], true);
        }
        g.flush();
    }

    fn update_fog(&mut self) {
        let w = &self.world;
        for y in 0..MH {
            for x in 0..MW {
                let i = idx(x, y);
                let a = match w.phase {
                    Phase::Prep if x >= MW / 2 => 0.45,
                    Phase::Battle if !w.vis[0][i] => 0.5,
                    _ => 0.0,
                };
                self.fog_px[i] = (((a * 255.0) as u32) << 24) | 0x0a0c0a;
            }
        }
        if let Some(f) = &self.fog {
            self.gfx.update_texture(f, 0, 0, MW as u32, MH as u32, &self.fog_px);
        }
    }

    fn render_world(&mut self) {
        self.update_fog();
        self.set_world_xform();
        let (mw, mh) = (MW as f32 * TS, MH as f32 * TS);
        if let Some(t) = &self.terrain {
            self.gfx.draw_textured(t, 0.0, 0.0, mw, mh, [1.0; 4], true);
        }
        let mouse_w = self.to_world(self.mouse);
        let over = self.over_ui(self.mouse);
        let tool_cost = self.tool_cost();
        let left = self.world.left(0);
        let (tool, mimic, touch, screen, marker) = (self.tool, self.mimic, self.touch, self.screen, self.marker);
        let g = &mut self.gfx;
        let atlas = self.atlas.as_ref();
        let w = &self.world;
        let px = 1.0 / self.zoom;
        let clock = self.clock;
        // 準備：配置範囲
        if w.phase == Phase::Prep {
            let x1 = crate::map::DEPLOY as f32 * TS;
            g.rect(0.0, 0.0, x1, mh, [0.6, 0.8, 1.0, 0.10]);
            g.rect(x1 - 2.0 * px, 0.0, x1, mh, [0.6, 0.85, 1.0, 0.7]);
            let x2 = (MW / 2 - crate::map::HALF_MARGIN) as f32 * TS;
            g.rect(x1, 0.0, x2, mh, [1.0, 0.9, 0.5, 0.05]);
            g.rect(x2 - 2.0 * px, 0.0, x2, mh, [1.0, 0.85, 0.4, 0.55]);
        }
        // 基地
        for side in 0..2 {
            let b = w.base_pos(side);
            let col = side_color(side);
            g.circle(b.0, b.1, CAPTURE_R * TS, 48, with_a(col, 0.10));
            g.ring(b.0, b.1, CAPTURE_R * TS, 2.5 * px, 48, with_a(col, 0.8));
            let c = w.capture[side];
            if c > 0.0 {
                let seg = (48.0 * c).ceil() as usize;
                let oc = side_color(1 - side);
                let r = CAPTURE_R * TS;
                for k in 0..seg {
                    let a0 = -std::f32::consts::FRAC_PI_2 + k as f32 / 48.0 * std::f32::consts::TAU;
                    let a1 = a0 + std::f32::consts::TAU / 48.0;
                    let p0 = (b.0 + a0.cos() * r, b.1 + a0.sin() * r);
                    let p1 = (b.0 + a1.cos() * r, b.1 + a1.sin() * r);
                    g.line(p0, p1, 7.0 * px, oc);
                }
            }
            // 司令部の建物（フォールバック用）
            if atlas.is_none() {
                let s = TS * 0.75;
                g.rect(b.0 - s + 4.0, b.1 - s + 5.0, b.0 + s + 4.0, b.1 + s + 5.0, [0.0, 0.0, 0.0, 0.35]);
                g.rect(b.0 - s, b.1 - s, b.0 + s, b.1 + s, [0.42, 0.40, 0.36, 1.0]);
                g.rect(b.0 - s * 0.7, b.1 - s * 0.7, b.0 + s * 0.7, b.1 + s * 0.7, [0.34, 0.33, 0.30, 1.0]);
                g.rect_outline(b.0 - s, b.1 - s, b.0 + s, b.1 + s, 2.0, shade(col, 0.9));
                g.line((b.0, b.1), (b.0, b.1 - TS * 1.3), 2.0, [0.2, 0.2, 0.2, 1.0]);
                let wave = (clock * 4.0).sin() * 2.0;
                g.tri((b.0, b.1 - TS * 1.3), (b.0 + TS * 0.8, b.1 - TS * 1.12 + wave), (b.0, b.1 - TS * 0.95), col);
            }
        }
        // 地面上の印（柵リング・生存ユニットの陣営円）
        for tt in w.fences.iter() {
            if !tt.alive || !(tt.side == 0 || tt.seen[0]) {
                continue;
            }
            let c = center(tt.tile);
            draw_fence(g, c, tt.hp / crate::data::FENCE_HP);
            g.ring(c.0, c.1, TS * 0.48, 1.5 * px, 16, with_a(side_color(tt.side), 0.6));
        }
        for (j, u) in w.units.iter().enumerate() {
            if !u.alive || u.kind == Kind::Hawk {
                continue;
            }
            let visible = u.side == 0 || w.phase == Phase::Over || w.spotted[0].get(j).copied().unwrap_or(false);
            if !visible {
                continue;
            }
            let alpha = if u.kind == Kind::Decoy && (u.side == 0 || u.revealed) { 0.72 } else { 1.0 };
            g.circle(u.pos.0, u.pos.1, u.spec().len * TS * 0.58, 20, with_a(side_color(u.side), 0.20 * alpha));
        }
        // YYMythology 風 3/4 クォータービュー：Yソートして手前から奧へ重なり描画
        if let Some(at) = atlas {
            let mut items: Vec<(f32, usize, f32, f32, f32, bool, [f32; 4])> = Vec::new();
            for side in 0..2 {
                let b = w.base_pos(side);
                let r = TS * 1.45;
                items.push((b.1 + TS * 0.25, CELL_HQ, b.0, b.1 - r * 0.22, r, side == 1, [1.0, 1.0, 1.0, 1.0]));
            }
            for (j, u) in w.units.iter().enumerate() {
                if u.kind == Kind::Hawk {
                    continue;
                }
                let visible = u.side == 0 || !u.alive || w.phase == Phase::Over || w.spotted[0].get(j).copied().unwrap_or(false);
                if !visible {
                    continue;
                }
                let flip_x = unit_flip_x(u);
                if !u.alive && u.kind == Kind::Decoy {
                    let r = TS * 0.72;
                    items.push((u.pos.1 - 1000.0, CELL_DECOY_WRECK, u.pos.0, u.pos.1, r, flip_x, [1.0, 1.0, 1.0, 0.85]));
                    continue;
                }
                let cell = unit_oblique_cell(u);
                let r = unit_oblique_radius(u.spec());
                let alpha = if u.kind == Kind::Decoy && (u.side == 0 || !u.revealed) && u.side == 0 {
                    0.78
                } else {
                    1.0
                };
                let tint = if !u.alive { [0.30, 0.30, 0.30, 1.0] } else { [1.0, 1.0, 1.0, alpha] };
                let sort_y = if u.alive { u.pos.1 } else { u.pos.1 - 500.0 };
                items.push((sort_y, cell, u.pos.0, u.pos.1 - r * 0.20, r, flip_x, tint));
            }
            items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            g.begin_sprites();
            for (_, cell, sx, sy, r, flip_x, tint) in items {
                let uv = atlas_uv(cell, flip_x);
                g.push_sprite(sx, sy, r, r, 0.0, uv, tint);
            }
            g.end_sprites(at, true);
        }
        // 基地の旗・デコイ見破り印・撃破煙/炎（およびフォールバック図形）
        for side in 0..2 {
            if atlas.is_some() {
                let b = w.base_pos(side);
                let col = side_color(side);
                let flag_base = (b.0, b.1 - TS * 0.9);
                g.line(flag_base, (flag_base.0, flag_base.1 - TS * 1.15), 2.0, [0.2, 0.2, 0.2, 1.0]);
                let wave = (clock * 4.0).sin() * 2.0;
                g.tri((flag_base.0, flag_base.1 - TS * 1.15), (flag_base.0 + TS * 0.72, flag_base.1 - TS * 0.98 + wave), (flag_base.0, flag_base.1 - TS * 0.82), col);
            }
        }
        for pass in 0..2 {
            for (j, u) in w.units.iter().enumerate() {
                if u.kind == Kind::Hawk || (pass == 0) == u.alive {
                    continue;
                }
                let visible = u.side == 0 || !u.alive || w.phase == Phase::Over || w.spotted[0].get(j).copied().unwrap_or(false);
                if !visible {
                    continue;
                }
                if !u.alive && u.kind == Kind::Decoy {
                    if atlas.is_none() {
                        draw_decoy_wreck(g, u);
                    }
                    continue;
                }
                let alpha = if u.kind == Kind::Decoy && (u.side == 0 || u.revealed) { 0.55 } else { 1.0 };
                if atlas.is_none() {
                    draw_troop_fallback(g, u, alpha);
                }
                if !u.alive {
                    draw_wreck_fx(g, u, clock);
                }
                if u.kind == Kind::Decoy && u.side == 1 && u.revealed && u.alive {
                    let r = TS * 0.35;
                    g.line((u.pos.0 - r, u.pos.1 - r), (u.pos.0 + r, u.pos.1 + r), 3.0 * px, [1.0, 0.9, 0.3, 0.9]);
                    g.line((u.pos.0 - r, u.pos.1 + r), (u.pos.0 + r, u.pos.1 - r), 3.0 * px, [1.0, 0.9, 0.3, 0.9]);
                }
            }
        }
        // 砲弾
        for s in &w.shells {
            if w.units[s.shooter].spec().range < 2.0 { continue; }
            let f = (s.t / s.dur).min(1.0);
            let p = (s.from.0 + (s.to.0 - s.from.0) * f, s.from.1 + (s.to.1 - s.from.1) * f);
            let tile = tile_of(p);
            if !w.vis[0][idx(tile.0, tile.1)] && s.side == 1 {
                continue;
            }
            let b = (f - 0.25).max(0.0);
            let q = (s.from.0 + (s.to.0 - s.from.0) * b, s.from.1 + (s.to.1 - s.from.1) * b);
            g.line(q, p, if s.big { 3.0 } else { 2.0 }, [1.0, 0.9, 0.5, 0.85]);
        }
        // 爆発・発砲炎
        for f in &w.fx {
            let k = f.t / f.life;
            let p = f.pos;
            match f.kind {
                FxKind::Flash => {
                    g.circle(p.0, p.1, TS * 0.35 * (1.0 - k), 10, [1.0, 0.85, 0.4, 0.9 * (1.0 - k)]);
                }
                FxKind::Spark => {
                    g.circle(p.0, p.1, TS * 0.3 * (0.5 + k), 10, [1.0, 0.6, 0.2, 0.9 * (1.0 - k)]);
                }
                FxKind::Dust => {
                    g.circle(p.0, p.1, TS * 0.45 * (0.4 + k), 12, [0.55, 0.48, 0.36, 0.55 * (1.0 - k)]);
                }
                FxKind::Boom => {
                    g.circle(p.0, p.1, TS * 1.1 * (0.3 + k), 18, [1.0, 0.55, 0.15, 0.85 * (1.0 - k)]);
                    g.circle(p.0, p.1, TS * 0.6 * (0.3 + k), 14, [1.0, 0.9, 0.5, 0.9 * (1.0 - k)]);
                }
            }
        }
        // 霧
        if let Some(f) = &self.fog {
            g.draw_textured(f, 0.0, 0.0, mw, mh, [1.0; 4], true);
        }
        // 鷹（空中なので霧の上）
        for (j, u) in w.units.iter().enumerate() {
            if u.kind != Kind::Hawk || !u.alive {
                continue;
            }
            if u.side == 1 && !w.spotted[0].get(j).copied().unwrap_or(false) && w.phase != Phase::Over {
                continue;
            }
            draw_hawk(g, u, clock);
            if u.side == 0 {
                g.ring(u.pos.0, u.pos.1, HAWK_SIGHT * TS, 1.5 * px, 64, [0.6, 0.85, 1.0, 0.25]);
            }
        }
        // 選択と体力
        for (j, u) in w.units.iter().enumerate() {
            if !u.alive || u.kind == Kind::Hawk {
                continue;
            }
            let shown = u.side == 0 || w.spotted[0].get(j).copied().unwrap_or(false) || w.phase == Phase::Over;
            if !shown {
                continue;
            }
            let s = u.spec();
            let selected = self.sel.contains(&j);
            if selected {
                g.ring(u.pos.0, u.pos.1, s.len * TS * 0.62, 2.0 * px, 24, [0.5, 1.0, 0.5, 0.95]);
                if let Order::Move { goal, attack } = u.order {
                    g.line(u.pos, goal, 1.2 * px, if attack { [1.0, 0.5, 0.3, 0.45] } else { [0.5, 1.0, 0.5, 0.45] });
                }
                if let Order::Attack(t) = u.order {
                    g.line(u.pos, w.target_pos(t), 1.2 * px, [1.0, 0.3, 0.2, 0.6]);
                }
            }
            if w.phase == Phase::Battle && u.kind == Kind::Troop || (u.kind == Kind::Decoy && u.side == 1 && w.phase == Phase::Battle) {
                // デコイも本物らしく見せる
                let frac = if u.kind == Kind::Decoy { 1.0 } else { u.hp / s.hp };
                let (bw, by) = (s.len * TS * 0.9, u.pos.1 - s.len * TS * 0.7);
                g.rect(u.pos.0 - bw / 2.0, by, u.pos.0 + bw / 2.0, by + 4.0 * px.max(0.6), [0.0, 0.0, 0.0, 0.6]);
                g.rect(u.pos.0 - bw / 2.0, by, u.pos.0 - bw / 2.0 + bw * frac, by + 4.0 * px.max(0.6), side_color(u.side));
            }
        }
        // 準備：置く物の予告
        if w.phase == Phase::Prep && screen == Screen::Battle && !self.show_generals && !over && !touch {
            let wp = mouse_w;
            let t = tile_of(wp);
            let c = center(t);
            let (ok, r) = match tool {
                Tool::Troop(..) => (w.can_place(0, wp, false, false) && left >= tool_cost, TS * 0.45),
                Tool::Decoy => (w.can_place(0, wp, true, false) && left >= DECOY_COST, TS * 0.45),
                Tool::Fence => (w.can_place(0, wp, true, true) && left >= FENCE_COST, TS * 0.5),
                Tool::Remove => (true, TS * 0.5),
            };
            let col = if tool == Tool::Remove { [1.0, 0.6, 0.3, 0.8] } else if ok { [0.5, 1.0, 0.5, 0.8] } else { [1.0, 0.3, 0.3, 0.8] };
            g.rect_outline(c.0 - r, c.1 - r, c.0 + r, c.1 + r, 2.0 * px, col);
            if let (Tool::Troop(n, i), true) = (tool, ok) {
                let ghost = ghost_unit(n, i, c);
                draw_troop_with_atlas(g, atlas, &ghost, 0.5);
                g.ring(c.0, c.1, n.troops()[i].range * TS, 1.5 * px, 64, [1.0, 1.0, 1.0, 0.2]);
            }
            if let (Tool::Decoy, true) = (tool, ok) {
                let mut ghost = ghost_unit(mimic.0, mimic.1, c);
                ghost.kind = Kind::Decoy;
                draw_troop_with_atlas(g, atlas, &ghost, 0.65);
                g.ring(c.0, c.1, DECOY_REVEAL * TS, 1.5 * px, 48, [1.0, 0.9, 0.4, 0.3]);
            }
        }
        // 命令の印
        if let Some((p, t, attack)) = marker {
            let k = t / 0.7;
            let col = if attack { [1.0, 0.45, 0.3, k] } else { [0.5, 1.0, 0.5, k] };
            g.ring(p.0, p.1, TS * (0.7 - 0.4 * k), 2.5 * px, 20, col);
        }
        g.flush();
    }

    fn render_minimap(&mut self) {
        let (w, h) = (self.gfx.width as f32, self.gfx.height as f32);
        let mw = if self.gfx.compact { 150.0 } else { 220.0 };
        let mh = mw * MH as f32 / MW as f32;
        let (x0, y1) = (8.0, h - 8.0);
        let y0 = y1 - mh;
        let x1 = x0 + mw;
        self.minimap = [x0, y0, x1, y1];
        self.gfx.set_screen_xform();
        let view_a = self.to_world((0.0, 0.0));
        let view_b = self.to_world((w, h));
        let g = &mut self.gfx;
        g.rect(x0 - 3.0, y0 - 3.0, x1 + 3.0, y1 + 3.0, [0.05, 0.05, 0.04, 0.95]);
        g.rect_outline(x0 - 3.0, y0 - 3.0, x1 + 3.0, y1 + 3.0, 1.5, EDGE);
        if let Some(t) = &self.terrain {
            g.draw_textured(t, x0, y0, x1, y1, [1.0; 4], true);
        }
        if let Some(f) = &self.fog {
            g.draw_textured(f, x0, y0, x1, y1, [1.0; 4], true);
        }
        let sx = mw / (MW as f32 * TS);
        let sy = mh / (MH as f32 * TS);
        let m = |p: (f32, f32)| (x0 + p.0 * sx, y0 + p.1 * sy);
        let wd = &self.world;
        for side in 0..2 {
            let b = m(wd.base_pos(side));
            g.rect(b.0 - 4.0, b.1 - 4.0, b.0 + 4.0, b.1 + 4.0, side_color(side));
            g.rect_outline(b.0 - 4.0, b.1 - 4.0, b.0 + 4.0, b.1 + 4.0, 1.0, [0.0, 0.0, 0.0, 1.0]);
        }
        for tt in &wd.fences {
            if tt.alive && (tt.side == 0 || tt.seen[0]) {
                let p = m(center(tt.tile));
                g.rect(p.0 - 1.0, p.1 - 1.0, p.0 + 1.0, p.1 + 1.0, [0.85, 0.85, 0.8, 1.0]);
            }
        }
        for (j, u) in wd.units.iter().enumerate() {
            if !u.alive {
                continue;
            }
            let shown = u.side == 0 || wd.spotted[0].get(j).copied().unwrap_or(false);
            if !shown || (u.kind == Kind::Decoy && u.side == 1 && u.revealed) {
                continue;
            }
            let p = m(u.pos);
            let r = if u.kind == Kind::Hawk { 2.0 } else { 2.5 };
            let mut c = side_color(u.side);
            if u.kind == Kind::Decoy && u.side == 0 {
                c = with_a(c, 0.45);
            }
            if self.sel.contains(&j) {
                c = [0.6, 1.0, 0.6, 1.0];
            }
            if u.kind == Kind::Hawk {
                g.tri((p.0, p.1 - r - 1.0), (p.0 + r + 1.0, p.1 + r), (p.0 - r - 1.0, p.1 + r), c);
            } else {
                g.rect(p.0 - r, p.1 - r, p.0 + r, p.1 + r, c);
            }
        }
        for &(p, age) in &self.pings {
            let q = m(p);
            let k = (age / 2.5).min(1.0);
            g.ring(q.0, q.1, 4.0 + 14.0 * k, 1.5, 16, [1.0, 0.85, 0.3, 1.0 - k]);
        }
        // 見えている範囲
        let (a, b) = (m(view_a), m(view_b));
        g.rect_outline(a.0.max(x0), a.1.max(y0), b.0.min(x1), b.1.min(y1), 1.2, [1.0, 1.0, 1.0, 0.8]);
        g.flush();
    }

    // ------------------------------------------------------------ 画面部品

    fn hovered(&self, r: [f32; 4]) -> bool {
        !self.touch && self.mouse.0 >= r[0] && self.mouse.0 <= r[2] && self.mouse.1 >= r[1] && self.mouse.1 <= r[3]
    }

    fn button(&mut self, r: [f32; 4], label: &str, act: Act, active: bool) {
        let hov = self.hovered(r);
        let primary = act == Act::Start;
        let title_screen = self.screen == Screen::Title;
        let bg = if title_screen && primary {
            if hov { [0.76,0.22,0.16,1.0] } else { [0.58,0.12,0.10,1.0] }
        } else if title_screen && active {
            [0.30,0.25,0.15,0.98]
        } else if title_screen {
            if hov {[0.11,0.19,0.24,0.98]} else {[0.045,0.095,0.13,0.94]}
        } else if primary {
            if hov { GOLD } else { [0.84, 0.64, 0.31, 1.0] }
        } else if active {
            [0.55, 0.42, 0.16, 0.95]
        } else if hov {
            [0.26, 0.26, 0.20, 0.95]
        } else {
            [0.15, 0.16, 0.12, 0.92]
        };
        self.gfx.fill_round(r[0], r[1], r[2], r[3], 5.0, bg);
        self.gfx.stroke_round(r[0], r[1], r[2], r[3], 5.0, 1.0, if active { GOLD } else { EDGE });
        let font = if primary { Font::BigCenter } else { Font::NormalCenter };
        let text = if title_screen && primary { [1.0,0.92,0.72,1.0] } else if primary { [0.11, 0.08, 0.045, 1.0] } else if active { [1.0, 0.97, 0.88, 1.0] } else { TEXT };
        self.gfx.text(label, r[0] + 3.0, r[1], r[2] - r[0] - 6.0, r[3] - r[1], font, text);
        self.buttons.push(Btn { r, act });
    }

    fn panel(&mut self, r: [f32; 4]) {
        let background=if self.screen==Screen::Title {[0.025,0.055,0.08,0.94]}else{PANEL};
        self.gfx.fill_round(r[0], r[1], r[2], r[3], 8.0, background);
        self.gfx.stroke_round(r[0], r[1], r[2], r[3], 8.0, 1.0, EDGE);
        self.panels.push(r);
    }

    fn render_title(&mut self, layout: &TitleLayout) {
        let TitleLayout { compact, left, top, menu_width, preview } = *layout;
        let title = t!("英雄の戦場", "Ikusa", "英雄战场");
        let font = if compact { Font::Title } else { Font::CoverTitle };
        let th = if compact { 47.0 } else { 72.0 };
        let mark_space=if compact {62.0}else{80.0};
        let tw = (self.gfx.measure(title, font).ceil() + 6.0).min(menu_width-mark_space);
        self.gfx.text("Y Y  I K U S A",left+mark_space,top,menu_width-mark_space,18.0,Font::Small,GOLD);
        self.gfx.text_shadow(title, left+mark_space, top + 17.0, tw, th, font, [1.0, 0.91, 0.73, 1.0]);
        self.gfx.text(t!("時代を超えた英雄たちの合戦", "Battles of historical heroes", "英雄之战"),
            left + 1.0, top + th + 23.0, menu_width, 23.0, Font::Big, [0.78,0.83,0.84,1.0]);

        let pad = if compact { 14.0 } else { 20.0 };
        let y0 = top + if compact { 99.0 } else { 130.0 };
        let x = left + pad;
        let y = y0 + pad;
        let width = menu_width - 2.0 * pad;
        let gap = 10.0;
        let column = (width - gap) / 2.0;
        let row = if compact { 33.0 } else { 38.0 };
        let map_y = y + 28.0;
        let desc_y = map_y + 3.0 * row + 4.0;
        let desc = self.field().desc();
        let desc_h = self.gfx.measure_h(desc, Font::Normal, width).max(30.0);
        let options_y = desc_y + desc_h + 12.0;
        let options_h = if compact { 30.0 } else { 34.0 };
        let footer = options_y + 26.0 + options_h + 16.0;
        let footer_h = if compact { 40.0 } else { 46.0 };

        self.panel([left, y0, left + menu_width, footer + footer_h + pad]);
        self.gfx.text(t!("戦場", "Battlefield", "战场"), x, y, width, 24.0, Font::Big, GOLD);
        for (i, field) in FIELDS.iter().enumerate() {
            let bx = x + (i % 2) as f32 * (column + gap);
            let by = map_y + (i / 2) as f32 * row;
            self.button([bx, by, bx + column, by + row - 6.0], field.name(), Act::Field(i), i == self.field_i);
        }
        self.gfx.text(desc, x, desc_y, width, desc_h, Font::Normal, TEXT);

        let right = x + column + gap;
        self.gfx.text(t!("予算規模（双方同じ）", "Budget (same for both)", "预算规模（双方相同）"),
            x, options_y, column, 22.0, Font::Big, GOLD);
        self.gfx.text(t!("敵の強さ", "Enemy strength", "敌方强度"),
            right, options_y, column, 22.0, Font::Big, GOLD);
        let oy = options_y + 26.0;
        let ow = (column - 12.0) / 3.0;
        for (i, b) in BUDGETS.iter().enumerate() {
            let dx = i as f32 * (ow + 6.0);
            self.button([x + dx, oy, x + dx + ow, oy + options_h],
                &format!("{}  {}", b.name(), b.points()), Act::Budget(i), i == self.budget_i);
            self.button([right + dx, oy, right + dx + ow, oy + options_h],
                diff_name(i), Act::Diff(i), i == self.diff_i);
        }
        let end = footer + footer_h;
        let start_width = width * 0.57;
        self.button([x, footer, x + start_width, end],
            t!("出陣する  ▶", "Muster for battle  ▶", "出征  ▶"), Act::Start, true);
        let shortcut = (width - start_width - 2.0 * gap) / 2.0;
        let bx = x + start_width + gap;
        self.button([bx, footer, bx + shortcut, end], lang::lang().next().native(), Act::Lang, false);
        self.button([bx + shortcut + gap, footer, x + width, end],
            t!("全画面", "Fullscreen", "全屏"), Act::Fullscreen, false);

        let [px0, py0, px1, py1] = preview;
        self.gfx.stroke_round(px0, py0, px1, py1, 4.0, 1.0, EDGE);
        self.gfx.text(t!("戦場プレビュー", "Battlefield preview", "战场预览"),
            px0 + 10.0, py0 + 2.0, px1 - px0 - 20.0, 24.0, Font::Normal, GOLD);
        #[cfg(not(target_arch = "wasm32"))]
        let reroll_end = px1 - 6.0;
        #[cfg(target_arch = "wasm32")]
        let reroll_end = (px0 + px1) / 2.0 - 3.0;
        self.button([px0 + 6.0, py1 - 36.0, reroll_end, py1 - 6.0],
            t!("地形を作り直す", "Regenerate terrain", "重新生成地形"), Act::Reroll, false);
        #[cfg(target_arch = "wasm32")]
        self.button([reroll_end + 6.0, py1 - 36.0, px1 - 6.0, py1 - 6.0],
            t!("キャッシュ削除", "Clear cache", "清除缓存"), Act::ClearCache, false);
    }

    fn render_hud(&mut self) {
        let w = self.gfx.width as f32;
        let bar = 36.0;
        self.panel([0.0, 0.0, w, bar]);
        let wd = &self.world;
        let field = wd.map.field.name().to_string();
        let status = match wd.phase {
            Phase::Prep => tf!("準備　予算 {} / {}", "Deployment — budget {} / {}", "准备　预算 {} / {}", wd.left(0), wd.budget),
            Phase::Battle | Phase::Over => {
                let t = wd.time as u32;
                let lim = TIME_LIMIT as u32;
                tf!("戦闘 {:02}:{:02} / {:02}:00", "Battle {:02}:{:02} / {:02}:00", "战斗 {:02}:{:02} / {:02}:00", t / 60, t % 60, lim / 60)
            }
        };
        let forces = {
            let mine = wd.troop_count(0);
            let lost = wd.units.iter().filter(|u| u.side == 0 && u.kind == Kind::Troop && !u.alive).count();
            let kills = wd.units.iter().filter(|u| u.side == 1 && u.kind == Kind::Troop && !u.alive).count();
            tf!("自軍 {}隊（損失 {}）　撃破 {}", "Ours: {} troops (lost {})   Kills: {}", "我方 {}队（损失 {}）　击毁 {}", mine, lost, kills)
        };
        let compact = self.gfx.compact;
        let fw = self.gfx.measure(&field, Font::Big).min(300.0);
        self.gfx.text(&field, 10.0, 0.0, fw, bar, Font::Big, GOLD);
        let sx = 10.0 + fw + 24.0;
        let sw = self.gfx.measure(&status, Font::Normal);
        self.gfx.text(&status, sx, 0.0, sw + 2.0, bar, Font::Normal, TEXT);
        if !compact && self.world.phase != Phase::Prep {
            let fx = sx + sw + 24.0;
            let room = (w - 480.0 - fx).max(0.0);
            if room > 120.0 {
                self.gfx.text(&forces, fx, 0.0, room, bar, Font::Normal, TEXT);
            }
        }
        // 右側のボタン
        let mut x = w - 8.0;
        let menu_label = if self.menu_confirm > 0.0 { t!("もう一度押すと終了", "Click again to quit", "再按一次退出") } else { t!("タイトルへ", "Title", "返回标题") };
        let mw = if self.menu_confirm > 0.0 { 150.0 } else { 86.0 };
        self.button([x - mw, 5.0, x, bar - 5.0], menu_label, Act::Menu, self.menu_confirm > 0.0);
        x -= mw + 6.0;
        self.button([x - 34.0, 5.0, x, bar - 5.0], "?", Act::Help, self.show_help);
        x -= 40.0;
        if self.world.phase == Phase::Prep {
            self.button([x - 100.0, 5.0, x, bar - 5.0],
                t!("将軍", "Generals", "将军"), Act::Generals, self.show_generals);
        }
        if self.world.phase == Phase::Battle {
            for s in (1..=3).rev() {
                self.button([x - 38.0, 5.0, x, bar - 5.0], &format!("x{}", s), Act::Speed(s), self.speed == s);
                x -= 42.0;
            }
            let pl = if self.paused { t!("再開", "Resume", "继续") } else { t!("一時停止", "Pause", "暂停") };
            self.button([x - 80.0, 5.0, x, bar - 5.0], pl, Act::Pause, self.paused);
            x -= 86.0;
            self.button([x - 76.0, 5.0, x, bar - 5.0],
                t!("将軍 G", "Generals G", "将军 G"), Act::Generals, self.show_generals);
            // 占領の度合い
            let wd = &self.world;
            let caps = [wd.capture[0], wd.capture[1]];
            let y = bar + 6.0;
            let bx = w / 2.0 - 160.0;
            if caps[0] > 0.0 || caps[1] > 0.0 {
                self.gfx.fill_round(bx - 6.0, y - 2.0, bx + 326.0, y + 48.0, 6.0, PANEL);
                for (k, side) in [0usize, 1].into_iter().enumerate() {
                    let yy = y + k as f32 * 24.0;
                    let label = if side == 0 { t!("自軍基地", "Our base", "我方基地") } else { t!("敵基地", "Enemy base", "敌方基地") };
                    self.gfx.text(label, bx, yy, 90.0, 20.0, Font::Small, TEXT);
                    self.gfx.fill_rect(bx + 90.0, yy + 5.0, bx + 320.0, yy + 15.0, [0.0, 0.0, 0.0, 0.6]);
                    self.gfx.fill_rect(bx + 90.0, yy + 5.0, bx + 90.0 + 230.0 * caps[side], yy + 15.0, side_color(1 - side));
                }
            }
        }
        // 出来事の記録
        let lw = if compact { 300.0 } else { 380.0 };
        let mut y = bar + 8.0;
        let lx = if self.world.phase == Phase::Prep { 10.0 } else { w - lw - 10.0 };
        let lines: Vec<(String, f32, bool)> = self.log.iter().rev().take(6).filter(|l| l.age < 10.0).map(|l| (l.text.clone(), l.age, l.good)).collect();
        for (text, age, good) in lines.into_iter() {
            let a = (10.0 - age).min(1.0);
            let th = self.gfx.measure_h(&text, Font::Small, lw - 12.0).max(16.0) + 6.0;
            self.gfx.fill_round(lx, y, lx + lw, y + th, 4.0, [0.0, 0.0, 0.0, 0.5 * a]);
            self.gfx.text(&text, lx + 6.0, y + 3.0, lw - 12.0, th - 6.0, Font::Small, with_a(if good { GOOD } else { [1.0, 0.7, 0.6, 1.0] }, a));
            y += th + 3.0;
        }
        if self.attack_mode {
            let (mx, my) = self.mouse;
            self.gfx.text_shadow(t!("攻撃移動：目標をクリック", "Attack-move: click a target", "攻击移动：点击目标"), mx + 16.0, my + 10.0, 240.0, 20.0, Font::Small, [1.0, 0.6, 0.4, 1.0]);
        }
        // 範囲選択の枠
        if let Some(d) = self.ldown {
            if !self.in_minimap(d) && self.delegate_stroke.is_none() {
                let m = self.mouse;
                if (m.0 - d.0).abs() > 8.0 || (m.1 - d.1).abs() > 8.0 {
                    self.gfx.fill_rect(d.0.min(m.0), d.1.min(m.1), d.0.max(m.0), d.1.max(m.1), [0.5, 1.0, 0.5, 0.12]);
                    self.gfx.stroke_rect(d.0.min(m.0), d.1.min(m.1), d.0.max(m.0), d.1.max(m.1), 1.0, [0.5, 1.0, 0.5, 0.9]);
                }
            }
        }
        if let Some(stroke) = &self.delegate_stroke {
            for &i in &stroke.targets {
                let p = self.to_screen(self.world.units[i].pos);
                self.gfx.ring(p.0, p.1, TS * 0.70 * self.zoom, 2.0, 24, GOOD);
            }
        }
        // ミニマップの見出し
        let r = self.minimap;
        self.panels.push([r[0] - 3.0, r[1] - 3.0, r[2] + 3.0, r[3] + 3.0]);
    }

    /// 部隊の名前など、地図の上の文字（重ならないよう、指している部隊と1隊だけ選んだ部隊に）
    fn render_labels(&mut self) {
        let mw = self.to_world(self.mouse);
        let mut items: Vec<(String, (f32, f32), C)> = Vec::new();
        for (j, u) in self.world.units.iter().enumerate() {
            if !u.alive || u.kind == Kind::Hawk {
                continue;
            }
            let shown = u.side == 0 || self.world.spotted[0].get(j).copied().unwrap_or(false);
            if !shown {
                continue;
            }
            let hover = !self.touch && dist(u.pos, mw) < TS * 0.6;
            let decoy_tag = u.kind == Kind::Decoy && (u.side == 0 || u.revealed);
            if decoy_tag && self.zoom > 0.7 {
                items.push((t!("デコイ", "Decoy", "诱饵").to_string(), u.pos, [1.0, 0.9, 0.5, 0.9]));
            } else if let Some(id) = u.general {
                if hover || self.zoom > 0.57 || self.show_generals {
                    let general = &GENERALS[id];
                    let marker = if u.side == 0 { format!("G{:02}", id + 1) }
                    else { tf!("敵 G{:02}", "Enemy G{:02}", "敌 G{:02}", id + 1) };
                    let name = if hover {
                        format!("{}  {}", marker, general.name())
                    } else {
                        marker
                    };
                    items.push((name, u.pos, if u.side == 0 { general.strategy.color() } else { RED }));
                }
            } else if hover || (self.sel.len() == 1 && self.sel[0] == j) {
                let name = if u.side == 1 { tf!("敵 {}", "Enemy {}", "敌 {}", u.spec().name()) } else { u.spec().name().to_string() };
                items.push((name, u.pos, with_a(side_color(u.side), 0.95)));
            }
        }
        for (s, p, c) in items {
            let sp = self.to_screen((p.0, p.1 + TS * 0.55));
            self.gfx.text_shadow(&s, sp.0 - 90.0, sp.1, 180.0, 18.0, Font::NormalCenter, c);
        }
    }

    fn refresh_portrait_cache(&mut self) {
        if self.portrait_gen != Some(self.gfx.d2d_gen) {
            self.portraits.fill_with(|| None);
            self.cutin_art = None;
            for faces in &mut self.expressions {
                faces.fill_with(|| None);
            }
            self.portrait_gen = Some(self.gfx.d2d_gen);
        }
    }

    fn render_skill_cutin(&mut self) {
        let Some((cue, elapsed)) = self.cutins.current() else { return };
        self.refresh_portrait_cache();
        if self.cutin_art.as_ref().is_none_or(|(id, _)| *id != cue.general) {
            if let Some((w, h, pixels)) = crate::skill_cutin::art(cue.general) {
                if let Some((id, image)) = &mut self.cutin_art {
                    // Reuse one bitmap: the browser retains canvas handles, so allocating per activation would leak.
                    self.gfx.update_bitmap(image, w, &pixels);
                    *id = cue.general;
                } else {
                    self.cutin_art = self.gfx.create_bitmap(w, h, &pixels).map(|image| (cue.general, image));
                }
            }
        }
        let (w, h) = (self.gfx.width as f32, self.gfx.height as f32);
        let duration = cue.duration();
        let fade = (elapsed / 0.16).min((duration - elapsed) / 0.22).clamp(0.0, 1.0);
        let slide = (1.0 - (elapsed / 0.24).clamp(0.0, 1.0)).powi(3) * 64.0;
        let width = (w - 28.0).min(1120.0);
        let height = (h * 0.64).min(490.0).min(width / 1.5);
        let left = (w - width) / 2.0 + if cue.side == 0 { slide } else { -slide };
        let top = (h - height) / 2.0;
        let right = left + width;
        let bottom = top + height;
        let accent = if cue.side == 1 { RED } else { GOLD };
        self.gfx.fill_rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.22 * fade]);
        self.gfx.fill_rect(left, top, right, bottom, [0.025, 0.045, 0.08, 0.98 * fade]);
        if let Some((_, image)) = &self.cutin_art {
            let image_width = height * 1.5;
            self.gfx.draw_bitmap_alpha(image, right - image_width, top, right, bottom, fade);
        }
        // A dark feathered text area keeps the generated image and title legible at every viewport size.
        let tw = (width * 0.46).min(440.0);
        for k in 0..64 {
            let x0 = left + width * 0.16 + k as f32 * tw / 64.0;
            self.gfx.fill_rect(x0, top, x0 + tw / 64.0, bottom,
                [0.02, 0.035, 0.06, (0.72 * (1.0 - k as f32 / 64.0)) * fade]);
        }
        self.gfx.fill_rect(left, top, right, top + 3.0, with_a(accent, fade));
        self.gfx.fill_rect(left, bottom - 3.0, right, bottom, with_a(accent, fade));
        let x = left + 22.0;
        let text_width = (width * 0.43 - 28.0).max(140.0);
        let heading = if cue.side == 0 { t!("固有スキル発動", "SIGNATURE SKILL", "专属技能发动") }
            else { t!("敵将軍・固有スキル", "ENEMY SIGNATURE SKILL", "敌将专属技能") };
        self.gfx.text(heading, x, top + height * 0.20, text_width, 24.0, Font::Big, with_a(accent, fade));
        let general = &GENERALS[cue.general];
        self.gfx.text(general.name(), x, top + height * 0.33, text_width, 44.0, Font::Big, with_a(TEXT, fade));
        self.gfx.text(general.skill.name(), x, top + height * 0.49, text_width, 72.0,
            if width >= 700.0 { Font::TitleMid } else { Font::Big }, with_a(accent, fade));
        self.gfx.text(t!("クリック／Escで閉じる", "Click / Esc to dismiss", "点击／Esc关闭"),
            x, bottom - 33.0, text_width, 22.0, Font::Small, with_a(DIM, fade));
        let r = [left, top, right, bottom];
        self.panels.push(r);
        self.buttons.push(Btn { r, act: Act::SkipCutin });
    }

    fn render_general_report(&mut self) {
        let Some(report) = self.radio.current() else { return };
        self.refresh_portrait_cache();
        let general = &GENERALS[report.general];
        let enemy = report.side == 1;
        let urgent = report.cue == Cue::Support && !enemy;
        let accent = if enemy { RED } else if urgent { [1.0, 0.63, 0.43, 1.0] } else { general.strategy.color() };
        let compact = self.gfx.compact;
        let width = if compact { 392.0 } else { 476.0 };
        let right = self.gfx.width as f32 - 10.0;
        let bottom = self.gfx.height as f32 - 10.0;
        let left = right - width;
        let height = if urgent { 124.0 } else { 100.0 };
        let top = bottom - height;
        self.panel([left, top, right, bottom]);
        self.gfx.fill_rect(left + 2.0, top + 14.0, left + 5.0, bottom - 14.0, accent);
        let size = if compact { 72.0 } else { 80.0 };
        let art_x = left + 11.0;
        let art_y = top + 10.0;
        let texture = &mut self.expressions[report.general][report.cue as usize];
        if texture.is_none() {
            let pixels = general_radio::expression_pixels(report.general, report.cue);
            *texture = self.gfx.create_bitmap(EXPRESSION_SIZE, EXPRESSION_SIZE, &pixels);
        }
        let image = texture.as_ref().or(self.portraits[report.general].as_ref());
        if let Some(image) = image {
            self.gfx.draw_bitmap(image, art_x, art_y, art_x + size, art_y + size);
        }
        let x = art_x + size + 12.0;
        let text_width = right - x - 12.0;
        let name_width = if urgent { text_width - 104.0 } else { text_width };
        let heading = if enemy {
            tf!("敵軍通信 · {}", "Enemy radio · {}", "敌军通信 · {}", general.name())
        } else {
            general.name().to_owned()
        };
        self.gfx.text(&heading, x, top + 9.0, name_width, 22.0, Font::Big, accent);
        self.gfx.text(report.line(), x, top + 34.0, text_width, 57.0, Font::Normal, TEXT);
        if urgent {
            self.gfx.text(t!("救援要請", "Support requested", "请求支援"), right - 110.0, top + 9.0, 100.0, 20.0, Font::Small, accent);
            self.button([right - 114.0, bottom - 30.0, right - 9.0, bottom - 7.0],
                t!("現場へ", "Go to location", "查看位置"), Act::GeneralFocus(report.tile), false);
        }
    }

    fn render_generals(&mut self) {
        self.refresh_portrait_cache();
        for (i, portrait) in self.portraits.iter_mut().enumerate() {
            if portrait.is_none() {
                let pixels = generals::portrait_pixels(i);
                *portrait = self.gfx.create_bitmap(PORTRAIT_SIZE, PORTRAIT_SIZE, &pixels);
            }
        }

        let battle = self.world.phase == Phase::Battle;
        let compact = self.gfx.compact;
        let w = self.gfx.width as f32;
        let bottom = (self.gfx.height as f32 - 8.0).min(610.0);
        let width = if compact { 410.0 } else { 486.0 };
        let left = w - width - 8.0;
        let top = 44.0;
        self.panel([left, top, left + width, bottom]);
        let x = left + 10.0;
        let inner = width - 20.0;
        let title = if battle {
            t!("将軍・副官の指揮権", "Command & Adjutants", "将军与副官指挥权")
        } else {
            t!("将軍に指揮を委任", "Delegate command", "委任指挥")
        };
        let women=GENERALS.iter().filter(|g|g.gender==Gender::Female).count();
        let title=format!("{}  ♀{} / ♂{}",title,women,GENERALS.len()-women);
        self.gfx.text(&title, x, top + 8.0, inner, 23.0, Font::Big, GOLD);
        let hint = if self.touch {
            t!("将軍を選び、部隊をなぞってまとめて委任。", "Choose a general, then trace units to assign them.", "选择将军后，滑过部队即可批量委任。")
        } else {
            t!("部隊からドラッグでなぞる／空白から範囲指定。", "Drag from a unit to trace; from empty ground to box-select.", "从部队拖动可涂选，从空白处拖动可框选。")
        };
        self.gfx.text(hint, x, top + 33.0, inner, 24.0, Font::Small, TEXT);

        let mut counts = [0; GENERALS.len()];
        for unit in &self.world.units {
            if unit.alive && unit.side == 0 {
                if let Some(i) = unit.general {
                    counts[i] += 1;
                }
            }
        }
        let grid_top = top + 61.0;
        let footer = bottom - 45.0;
        let details = footer - 138.0;
        let gap = 5.0;
        let columns = 6;
        let rows = GENERALS.len().div_ceil(columns);
        let cw = (inner - gap * (columns - 1) as f32) / columns as f32;
        let ch = ((details - grid_top - 7.0) / rows as f32).min(116.0);
        for (i, general) in GENERALS.iter().enumerate() {
            let bx = x + (i % columns) as f32 * (cw + gap);
            let by = grid_top + (i / columns) as f32 * ch;
            let r = [bx, by, bx + cw, by + ch - gap];
            let selected = self.assign_to == Some(i);
            let col = general.strategy.color();
            let bg = if selected { [0.29, 0.25, 0.16, 1.0] }
                else if self.hovered(r) { [0.22, 0.23, 0.18, 1.0] }
                else { [0.13, 0.15, 0.12, 1.0] };
            self.gfx.fill_round(r[0], r[1], r[2], r[3], 4.0, bg);
            self.gfx.stroke_round(r[0], r[1], r[2], r[3], 4.0, if selected { 2.0 } else { 1.0 }, if selected { GOLD } else { EDGE });
            let size = (ch - 29.0).min(cw - 9.0).max(10.0);
            let px = bx + (cw - size) / 2.0;
            if let Some(img) = &self.portraits[i] {
                self.gfx.draw_bitmap(img, px, by + 4.0, px + size, by + 4.0 + size);
            }
            self.gfx.fill_rect(bx + 4.0, by + 4.0, bx + 31.0, by + 20.0, PANEL);
            self.gfx.text(&format!("G{:02}", i + 1), bx + 6.0, by + 4.0, 25.0, 16.0, Font::Small, col);
            if counts[i] != 0 {
                self.gfx.fill_rect(r[2] - 22.0, by + 4.0, r[2] - 4.0, by + 22.0, PANEL);
                self.gfx.text(&counts[i].to_string(), r[2] - 22.0, by + 4.0, 18.0, 18.0, Font::NormalCenter, GOLD);
            } else if battle && self.world.adjutants[i] {
                self.gfx.fill_rect(r[2] - 34.0, by + 4.0, r[2] - 4.0, by + 20.0, PANEL);
                self.gfx.text(t!("副官", "Adj", "副官"), r[2] - 33.0, by + 4.0, 28.0, 16.0, Font::Small, [0.62, 0.85, 0.98, 1.0]);
            }
            let short = if i == 2 { t!("徴姉妹","Trung Sisters","征氏姐妹") } else { general.name().split([' ', '・', '·']).next().unwrap_or(general.name()) };
            self.gfx.text(short, bx + 5.0, r[3] - 19.0, cw - 10.0, 16.0, Font::NormalCenter, TEXT);
            self.gfx.fill_rect(bx + 5.0, r[3] - 3.0, r[2] - 5.0, r[3] - 2.0, col);
            self.buttons.push(Btn { r, act: Act::AssignGeneral(Some(i)) });
        }
        let header = self.assign_to.map_or_else(
            || t!("直轄部隊", "Direct command", "直属部队").to_string(),
            |i| {
                if battle && counts[i] == 0 && self.world.adjutants[i] {
                    tf!("{}　副官（再委任可能）", "{}  ·  Adjutant (standby)", "{}  ·  副官（可重新委任）", GENERALS[i].name())
                } else if battle && counts[i] > 0 {
                    tf!("{}　将軍（委任 {} 隊）", "{}  ·  General ({} assigned)", "{}  ·  将军（委任 {} 队）", GENERALS[i].name(), counts[i])
                } else {
                    tf!("{}　委任 {} 隊", "{}  ·  {} assigned", "{}  ·  委任 {} 队", GENERALS[i].name(), counts[i])
                }
            },
        );
        self.gfx.text(&header, x, details, inner, 22.0, Font::Big, GOLD);
        if let Some(i) = self.assign_to {
            let strategy = GENERALS[i].strategy;
            let skill = &GENERALS[i].skill;
            let label = format!("{} · {}", strategy.name(), skill.name());
            self.gfx.text(&label, x, details + 23.0, inner, 19.0, Font::Normal, strategy.color());
            self.gfx.text(&skill.description(), x, details + 44.0, inner, 49.0, Font::Small, TEXT);
            if skill.trigger == crate::general_skills::Trigger::Once {
                if self.world.can_activate_skill(0, i) {
                    self.button([x, details + 98.0, x + inner, details + 128.0],
                        t!("固有スキル発動（1戦1回）", "Activate skill (once per battle)", "发动专属技能（每战一次）"), Act::ActivateSkill(i), true);
                } else {
                    let state = match self.world.skill_remaining(0, i) {
                        Some(t) if t > 0.0 => tf!("発動中：残り {}秒", "Active: {}s remaining", "生效中：剩余 {}秒", t.ceil() as u32),
                        Some(_) => t!("使用済み", "Already used", "已使用").to_string(),
                        None => t!("戦闘中、部隊を委任すると発動できます", "Assign units during battle to activate", "战斗中委任部队后可发动").to_string(),
                    };
                    self.gfx.text(&state, x, details + 98.0, inner, 28.0, Font::Small, GOLD);
                }
            } else {
                self.gfx.text(strategy.description(), x, details + 98.0, inner, 28.0, Font::Small, DIM);
            }
        } else {
            self.gfx.text(t!("指定した部隊は開戦後も自分で操作します。", "Marked troops remain under your manual control in battle.", "标记的部队在开战后仍由你直接控制。"),
                x, details + 23.0, inner, 34.0, Font::Small, TEXT);
        }
        let third = (inner - 2.0 * gap) / 3.0;
        if battle {
            self.button([x, footer, x + third, bottom - 7.0], t!("閉じる", "Close", "关闭"), Act::Generals, false);
            if let Some(id) = self.assign_to.filter(|&id| counts[id] > 0) {
                self.button([x + third + gap, footer, x + 2.0 * third + gap, bottom - 7.0],
                    t!("指揮権を返還", "Return command", "交还指挥权"), Act::ReturnGeneral(id), true);
            } else {
                self.button([x + third + gap, footer, x + 2.0 * third + gap, bottom - 7.0],
                    t!("直轄へ戻す", "Direct control", "恢复直属"), Act::AssignGeneral(None), self.assign_to.is_none());
            }
            let sel_n = self.selected_direct_troops().len();
            if let Some(id) = self.assign_to.filter(|_| sel_n > 0) {
                self.button([x + 2.0 * (third + gap), footer, x + inner, bottom - 7.0],
                    &tf!("選択 {} 隊を委任", "Assign {} troops", "委任 {} 队", sel_n), Act::DelegateSelected(id), true);
            } else {
                self.button([x + 2.0 * (third + gap), footer, x + inner, bottom - 7.0],
                    t!("直轄部隊を選ぶ Q", "Select direct Q", "选择直属 Q"), Act::AllTroops, false);
            }
        } else {
            self.button([x, footer, x + third, bottom - 7.0], t!("配置に戻る", "Deployment", "返回部署"), Act::Generals, false);
            self.button([x + third + gap, footer, x + 2.0 * third + gap, bottom - 7.0],
                t!("直轄へ戻す", "Direct control", "恢复直属"), Act::AssignGeneral(None), self.assign_to.is_none());
            self.button([x + 2.0 * (third + gap), footer, x + inner, bottom - 7.0],
                t!("戦闘開始 ▶", "Begin battle ▶", "开始战斗 ▶"), Act::Begin, true);
        }
    }

    fn render_shop(&mut self) {
        let (w, h) = (self.gfx.width as f32, self.gfx.height as f32);
        let compact = self.gfx.compact;
        let pw = if compact { 312.0 } else { 378.0 };
        let x0 = w - pw - 8.0;
        let y0 = 44.0;
        let y1 = h - 8.0;
        self.panel([x0, y0, x0 + pw, y1]);
        let ix = x0 + 10.0;
        let iw = pw - 20.0;
        let mut y = y0 + 8.0;
        // 国の見出し
        let tw = (iw - 10.0) / 3.0;
        for (k, n) in NATIONS.iter().enumerate() {
            let (r, c) = (k / 3, k % 3);
            let rr = [ix + c as f32 * (tw + 5.0), y + r as f32 * 30.0, ix + c as f32 * (tw + 5.0) + tw, y + r as f32 * 30.0 + 26.0];
            self.button(rr, n.name(), Act::Tab(k), self.tab == k);
            // 旗の色
            let f = n.flag();
            for (s, col) in f.iter().enumerate() {
                self.gfx.fill_rect(rr[0] + 4.0 + s as f32 * 4.0, rr[3] - 5.0, rr[0] + 8.0 + s as f32 * 4.0, rr[3] - 2.0, *col);
            }
        }
        y += 62.0;
        let nation = NATIONS[self.tab];
        if !compact {
            self.gfx.text(nation.blurb(), ix, y, iw, 32.0, Font::Small, DIM);
            y += 32.0;
        }
        // 2列グリッド（8兵種が1画面に収まる）
        let col_w = (iw - 6.0) / 2.0;
        let ch = if compact { 44.0 } else { 54.0 };
        let troops_list = nation.troops();
        let rows = (troops_list.len() + 1) / 2;
        let mut hovered_spec: Option<&TroopSpec> = None;
        for (i, s) in troops_list.iter().enumerate() {
            let (row, col) = (i / 2, i % 2);
            let cx0 = ix + col as f32 * (col_w + 6.0);
            let cy0 = y + row as f32 * ch;
            let r = [cx0, cy0, cx0 + col_w, cy0 + ch - 4.0];
            let active = self.tool == Tool::Troop(nation, i);
            let hov = self.hovered(r);
            if hov || active {
                hovered_spec = Some(s);
            }
            let afford = s.cost <= self.world.left(0);
            self.gfx.fill_round(r[0], r[1], r[2], r[3], 5.0, if active { [0.45, 0.35, 0.14, 0.95] } else if hov { [0.24, 0.24, 0.19, 0.95] } else { [0.14, 0.15, 0.11, 0.92] });
            self.gfx.stroke_round(r[0], r[1], r[2], r[3], 5.0, 1.0, if active { GOLD } else { EDGE });
            let nc = if afford { TEXT } else { DIM };
            self.gfx.text(s.name(), r[0] + 6.0, r[1] + 2.0, col_w - 44.0, 18.0, Font::Small, nc);
            self.gfx.text(&format!("{}", s.cost), r[2] - 40.0, r[1] + 2.0, 34.0, 18.0, Font::Normal, if afford { GOLD } else { RED });
            let tag = s.role.tag();
            let line2 = if compact {
                if tag.is_empty() {
                    tf!("甲{}/{} 貫{}", "A{}/{} P{}", "甲{}/{} 穿{}", s.front as u32, s.side as u32, s.pen as u32)
                } else {
                    format!("{} {}", tag, tf!("甲{} 貫{}", "A{} P{}", "甲{} 穿{}", s.front as u32, s.pen as u32))
                }
            } else if tag.is_empty() {
                tf!("{} 甲{}/{} 貫{}", "{} A{}/{} P{}", "{} 甲{}/{} 穿{}", weight_name(s.weight), s.front as u32, s.side as u32, s.pen as u32)
            } else {
                format!("{} {}", tag, tf!("甲{}/{} 貫{}", "A{}/{} P{}", "甲{}/{} 穿{}", s.front as u32, s.side as u32, s.pen as u32))
            };
            self.gfx.text(&line2, r[0] + 6.0, r[1] + if compact { 20.0 } else { 22.0 }, col_w - 12.0, 16.0, Font::Small, DIM);
            if !compact {
                self.gfx.text(&format!("{}  HP {}", s.gun, s.hp as u32), r[0] + 6.0, r[1] + 35.0, col_w - 12.0, 15.0, Font::Small, DIM);
            }
            self.buttons.push(Btn { r, act: Act::Tool(Tool::Troop(nation, i)) });
        }
        y += rows as f32 * ch + 2.0;
        if !compact {
            if let Some(s) = hovered_spec {
                let info = tf!(
                    "{}：{} HP{} 速{:.1} 射程{} 攻撃間隔{:.1}s",
                    "{}: {} HP{} Spd{:.1} Rng{} Rld{:.1}s",
                    "{}：{} HP{} 速{:.1} 射程{} 攻撃間隔{:.1}s",
                    s.name(),
                    s.gun,
                    s.hp as u32,
                    s.speed,
                    s.range as u32,
                    s.reload
                );
                self.gfx.text(&info, ix, y, iw, 18.0, Font::Small, GOLD);
            }
            y += 18.0;
        }
        let (mn, mi) = self.mimic;
        let decoy_label = tf!("デコイ {}（{}に偽装）", "Decoy {} (looks like {})", "诱饵 {}（伪装成{}）", DECOY_COST, mn.troops()[mi].name());
        self.button([ix, y, ix + iw, y + 26.0], &decoy_label, Act::Tool(Tool::Decoy), self.tool == Tool::Decoy);
        y += 30.0;
        let hw = (iw - 6.0) / 2.0;
        self.button([ix, y, ix + hw, y + 26.0], &tf!("柵 {}", "Palisades {}", "栅栏 {}", FENCE_COST), Act::Tool(Tool::Fence), self.tool == Tool::Fence);
        self.button([ix + hw + 6.0, y, ix + iw, y + 26.0], t!("撤去", "Remove", "撤除"), Act::Tool(Tool::Remove), self.tool == Tool::Remove);
        y += 30.0;
        let wd = &self.world;
        let troops = wd.troop_count(0);
        let decoys = wd.units.iter().filter(|u| u.side == 0 && u.kind == Kind::Decoy).count();
        let fences = wd.fences.iter().filter(|t| t.side == 0).count();
        let summary = tf!("部隊 {}隊　デコイ {}　柵 {}", "Troops {}  Decoys {}  Fence {}", "部队 {}队　诱饵 {}　栅栏 {}", troops, decoys, fences);
        self.gfx.text(&summary, ix, y, iw, 20.0, Font::Normal, TEXT);
        y += 20.0;
        if !compact {
            let hint = t!("左クリックで置く・右クリックで取り除く。柵はドラッグで連続して置ける", "Left-click to place, right-click to remove. Drag to lay a line of palisades.", "左键放置，右键撤除。拖动可连续放置栅栏");
            self.gfx.text(hint, ix, y, iw, 34.0, Font::Small, DIM);
        }
        let by = y1 - 46.0;
        self.button([ix, by - 32.0, ix + hw, by - 4.0], t!("おまかせ配置", "Auto deploy", "自动部署"), Act::Auto, false);
        self.button([ix + hw + 6.0, by - 32.0, ix + iw, by - 4.0], t!("全て撤去", "Clear all", "全部撤除"), Act::Clear, false);
        self.button([ix, by, ix + iw, by + 38.0], t!("戦闘開始 ▶", "Begin battle ▶", "开始战斗 ▶"), Act::Begin, true);
    }

    fn render_selection(&mut self) {
        let (w, h) = (self.gfx.width as f32, self.gfx.height as f32);
        let compact = self.gfx.compact;
        // 命令ボタン
        let bx = self.minimap[2] + 14.0;
        let bw = if compact { 74.0 } else { 96.0 };
        let by = h - 40.0;
        let all_label = if self.world.units.iter().any(|u| u.alive && u.side == 0 && u.general.is_some()) {
            t!("直轄部隊 Q", "Direct Q", "直属部队 Q")
        } else {
            t!("全部隊 Q", "All troops Q", "全部部队 Q")
        };
        let acts = [
            (t!("攻撃移動 A", "Attack A", "攻击移动 A"), Act::AttackMode, self.attack_mode),
            (t!("停止 S", "Stop S", "停止 S"), Act::Stop, false),
            (all_label, Act::AllTroops, false),
            (t!("鷹 D", "Hawk D", "鹰 D"), Act::Hawk, false),
        ];
        for (k, (label, a, on)) in acts.into_iter().enumerate() {
            let x = bx + k as f32 * (bw + 6.0);
            self.button([x, by, x + bw, by + 32.0], label, a, on);
        }
        if self.sel.is_empty() {
            return;
        }
        let pw = if compact { 360.0 } else { 440.0 };
        let x0 = bx;
        let y1 = by - 6.0;
        let y0 = y1 - if compact { 66.0 } else { 82.0 };
        if x0 + pw > w {
            return;
        }
        self.panel([x0, y0, x0 + pw, y1]);
        let wd = &self.world;
        if self.sel.len() == 1 {
            let u = &wd.units[self.sel[0]];
            if u.kind == Kind::Hawk {
                self.gfx.text(t!("偵察鷹", "Recon hawk", "侦察鹰"), x0 + 10.0, y0 + 4.0, pw - 150.0, 22.0, Font::Big, GOLD);
                let d = tf!("視界 {} マス。手動の移動・停止で巡回を解除。", "Sight {} tiles. Manual move or stop cancels patrol.", "视野 {} 格。手动移动或停止会取消巡逻。", HAWK_SIGHT as u32);
                self.gfx.text(&d, x0 + 10.0, y0 + 30.0, pw - 20.0, 28.0, Font::Small, TEXT);
                let patrol = self.world.hawk_patrolling(0);
                let label = if patrol { t!("巡回解除 P", "End patrol P", "解除巡逻 P") }
                    else { t!("自動巡回 P", "Auto patrol P", "自动巡逻 P") };
                self.button([x0 + pw - 132.0, y0 + 3.0, x0 + pw - 7.0, y0 + 27.0],
                    label, Act::HawkPatrol, patrol);
                return;
            }
            let s = u.spec();
            let terr = wd.map.at(u.tile().0, u.tile().1);
            let title_w = if self.world.phase == Phase::Battle { pw - 166.0 } else { pw - 20.0 };
            self.gfx.text(&format!("{}（{}）", s.name(), u.nation.name()), x0 + 10.0, y0 + 4.0, title_w, 22.0, Font::Big, GOLD);
            let l1 = tf!("耐久 {}/{}　防御 正面{} 側面{} 背面{}　突破 {}", "HP {}/{}  Armor F{} S{} R{}  Pen {}", "耐久 {}/{}　装甲 正{} 侧{} 后{}　穿深 {}", u.hp.ceil() as u32, s.hp as u32, s.front as u32, s.side as u32, s.rear() as u32, s.pen as u32);
            let l2 = tf!("地形：{}（速度 x{:.2}）　撃破 {}", "Terrain: {} (speed x{:.2})  Kills {}", "地形：{}（速度 x{:.2}）　击毁 {}", terr.name(), terr.speed(s.weight), u.kills);
            self.gfx.text(&l1, x0 + 10.0, y0 + 28.0, pw - 20.0, 20.0, Font::Small, TEXT);
            self.gfx.text(&l2, x0 + 10.0, y0 + 48.0, pw - 20.0, 20.0, Font::Small, TEXT);
        } else {
            let n = self.sel.len();
            let hp: f32 = self.sel.iter().map(|&i| wd.units[i].hp / wd.units[i].spec().hp.max(1.0)).sum::<f32>() / n as f32;
            let title_w = if self.world.phase == Phase::Battle { pw - 166.0 } else { pw - 20.0 };
            self.gfx.text(&tf!("{}隊を選択中", "{} selected", "已选择 {}队", n), x0 + 10.0, y0 + 4.0, title_w, 22.0, Font::Big, GOLD);
            let mut names: Vec<&str> = self.sel.iter().map(|&i| wd.units[i].spec().name()).collect();
            names.sort();
            names.dedup();
            let l = tf!("平均耐久 {}%　{}", "Avg HP {}%  {}", "平均耐久 {}%　{}", (hp * 100.0) as u32, names.join(", "));
            self.gfx.text(&l, x0 + 10.0, y0 + 28.0, pw - 20.0, 44.0, Font::Small, TEXT);
        }
        if self.world.phase == Phase::Battle && !self.selected_direct_troops().is_empty() {
            let adj: Vec<usize> = self.world.adjutants.iter().enumerate().filter(|(_, &a)| a).map(|(i, _)| i).collect();
            let (label, act, active) = if adj.len() == 1 {
                let id = adj[0];
                let short = GENERALS[id].name().split([' ', '・', '·']).next().unwrap_or(GENERALS[id].name());
                (tf!("副官 {} へ委任", "Assign {}", "委任副官 {}", short), Act::DelegateSelected(id), true)
            } else if !adj.is_empty() {
                (t!("副官へ委任 G", "Assign adjutant G", "委任副官 G").to_string(), Act::Generals, true)
            } else {
                (t!("将軍に委任 G", "Delegate G", "委任将军 G").to_string(), Act::Generals, self.show_generals)
            };
            self.button([x0 + pw - 150.0, y0 + 3.0, x0 + pw - 7.0, y0 + 27.0], &label, act, active);
        }
    }

    fn render_help(&mut self) {
        let (w, h) = (self.gfx.width as f32, self.gfx.height as f32);
        let pw = (w - 40.0).min(760.0);
        let ph = (h - 80.0).min(520.0);
        let (x0, y0) = ((w - pw) / 2.0, (h - ph) / 2.0);
        self.panel([x0, y0, x0 + pw, y0 + ph]);
        self.gfx.text(t!("操作と規則", "Controls & rules", "操作与规则"), x0, y0 + 8.0, pw, 30.0, Font::BigCenter, GOLD);
        let body = t!(
            "【準備】右の一覧から部隊を選び、自陣の明るい帯に左クリックで配置。デコイと柵は中央の手前まで置ける。右クリックで取り除き、予算が戻る。\n【戦闘】左クリック／ドラッグで自軍を選択、右クリックで移動。敵を右クリックすると攻撃。A キーのあと地点を指定すると、敵を見つけ次第止まって撃つ攻撃移動。S 停止、Q 全部隊、D 鷹、G 将軍・副官（指揮権返還／再委任）、Space 一時停止、＋／－ 速度。\n【地図】矢印キー・画面端・中ボタンのドラッグで移動、ホイールで拡大縮小、ミニマップで移動。\n【兵種】象兵・歩兵・騎馬槍隊は近接戦。弓隊・鉄砲隊・大砲は遠距離戦。側面と背面は防御が薄く、移動中は命中率が下がる。\n【視界】森・村落・生垣の中の部隊は近づかないと見つからないが、撃つと位置がばれる。丘の上は遠くまで見える。\n【デコイ】敵には本物の部隊に見え、攻撃を引きつける。敵の部隊が近づくと見破られる。\n【柵】部隊は通れない。敵は攻撃して壊さなければならない。\n【勝利】敵基地の円の中に自軍の部隊だけがいると占領が進む。占領するか全滅させれば勝ち。15分で決着しなければ残存戦力で判定。",
            "[Deploy] Pick a troop on the right and left-click inside your bright zone. Decoys and fences may go up to just short of the centre. Right-click removes an item and refunds it.\n[Battle] Left-click/drag to select, right-click to move. Right-click an enemy to attack. Press A then pick a point to attack-move (halt and fire at enemies). S stop, Q all troops, D hawk, G generals/adjutants (return or re-delegate command), Space pause, +/- speed.\n[Map] Arrow keys, screen edges or middle-drag to scroll; wheel to zoom; click the minimap.\n[Armor] Elephants, infantry and lancers fight in melee. Bows, muskets and cannon attack at range. Flanks and rear have weaker defenses. Firing on the move is less accurate.\n[Sight] Troops in forest, villages or hedgerows are only spotted up close — until they fire. Hills see further.\n[Decoys] Look like real troops to the enemy and draw fire, until an enemy troop comes close.\n[Palisades] Block troops. The enemy must shoot them apart.\n[Victory] Your troops alone inside the enemy base circle capture it. Capture the base or destroy every enemy troop. After 15 minutes the remaining strength decides.",
            "【准备】从右侧列表选择部队，在己方亮色区域左键部署。诱饵和栅栏可放置到中线前。右键撤除并退还预算。\n【战斗】左键/拖动选择部队，右键移动，右键点击敌人攻击。按A后指定地点为攻击移动（发现敌人即停下射击）。S停止，Q全部部队，D鹰，G将军/副官（交还或重新委任指挥权），空格暂停，＋/－速度。\n【地图】方向键、屏幕边缘或中键拖动移动，滚轮缩放，点击小地图移动。\n【装甲】正面、侧面、后部装甲不同。打击侧后方，轻装部队也能突破重装部队的防御。行进间射击命中率下降。\n【视野】森林、村落、树篱中的部队只有接近才能发现，但开火会暴露。丘陵上视野更远。\n【诱饵】在敌人看来是真部队，吸引火力，敌方部队接近时会被识破。\n【栅栏】部队无法通过，敌人必须攻击摧毁。\n【胜利】敌方基地圈内只有我方部队时开始占领。占领或全歼即获胜。15分钟未分胜负则按剩余战力判定。"
        );
        self.gfx.text(body, x0 + 20.0, y0 + 46.0, pw - 40.0, ph - 100.0, Font::Small, TEXT);
        self.button([x0 + pw / 2.0 - 70.0, y0 + ph - 46.0, x0 + pw / 2.0 + 70.0, y0 + ph - 12.0], t!("閉じる", "Close", "关闭"), Act::Help, false);
    }

    fn render_over(&mut self) {
        let (w, h) = (self.gfx.width as f32, self.gfx.height as f32);
        let pw = (w - 40.0).min(520.0);
        let ph = 250.0;
        let (x0, y0) = ((w - pw) / 2.0, (h - ph) / 2.0);
        self.panel([x0, y0, x0 + pw, y0 + ph]);
        let wd = &self.world;
        let (title, col) = match wd.winner {
            Some(0) => (t!("勝利", "Victory", "胜利"), GOLD),
            Some(_) => (t!("敗北", "Defeat", "失败"), RED),
            None => (t!("引き分け", "Draw", "平局"), TEXT),
        };
        let reason = match (wd.reason, wd.winner) {
            (Some(EndReason::Capture), Some(0)) => t!("敵の基地を占領した", "You captured the enemy base", "占领了敌方基地"),
            (Some(EndReason::Capture), _) => t!("基地を占領された", "Your base was captured", "基地被占领"),
            (Some(EndReason::Annihilation), Some(0)) => t!("敵の部隊をすべて撃破した", "All enemy troops destroyed", "击毁了敌方全部部队"),
            (Some(EndReason::Annihilation), Some(_)) => t!("自軍の部隊が全滅した", "All of your troops were destroyed", "我方部队全军覆没"),
            (Some(EndReason::Annihilation), None) => t!("双方が全滅した", "Both sides were wiped out", "双方全军覆没"),
            (Some(EndReason::TimeUp), _) => t!("時間切れ：残存戦力で判定", "Time up: decided by remaining strength", "时间到：按剩余战力判定"),
            (None, _) => "",
        };
        let lost = wd.units.iter().filter(|u| u.side == 0 && u.kind == Kind::Troop && !u.alive).count();
        let kills = wd.units.iter().filter(|u| u.side == 1 && u.kind == Kind::Troop && !u.alive).count();
        let decoys_hit = wd.units.iter().filter(|u| u.side == 0 && u.kind == Kind::Decoy && !u.alive).count();
        let t = wd.time as u32;
        let stats = tf!("撃破 {}隊　損失 {}隊　敵を欺いたデコイ {}　戦闘時間 {}:{:02}", "Kills {}  Losses {}  Decoys that fooled the enemy {}  Time {}:{:02}", "击毁 {}队　损失 {}队　骗过敌人的诱饵 {}　战斗时间 {}:{:02}", kills, lost, decoys_hit, t / 60, t % 60);
        self.gfx.text_shadow(title, x0, y0 + 14.0, pw, 56.0, Font::Title, col);
        self.gfx.text(reason, x0, y0 + 80.0, pw, 26.0, Font::BigCenter, TEXT);
        self.gfx.text(&stats, x0 + 20.0, y0 + 116.0, pw - 40.0, 50.0, Font::NormalCenter, DIM);
        let bw = (pw - 60.0) / 2.0;
        self.button([x0 + 20.0, y0 + ph - 56.0, x0 + 20.0 + bw, y0 + ph - 16.0], t!("新しい地形で再戦", "Rematch (new terrain)", "新地形再战"), Act::Again, true);
        self.button([x0 + 40.0 + bw, y0 + ph - 56.0, x0 + pw - 20.0, y0 + ph - 16.0], t!("タイトルへ", "Title", "返回标题"), Act::ToTitle, false);
    }
}

// ------------------------------------------------------------ 部隊の絵

fn ghost_unit(n: Nation, i: usize, pos: (f32, f32)) -> Unit {
    World::new_unit(0, Kind::Troop, n, i, pos)
}

/// 向きのある長方形（中心 c、角度 a、前後のずれ off、半長 hl、半幅 hw）
fn quad(g: &mut Gfx, c: (f32, f32), a: f32, off: f32, hl: f32, hw: f32, col: C) {
    let (ca, sa) = (a.cos(), a.sin());
    let cx = c.0 + ca * off;
    let cy = c.1 + sa * off;
    let p = |l: f32, w: f32| (cx + ca * l - sa * w, cy + sa * l + ca * w);
    let (a1, b1, c1, d1) = (p(hl, hw), p(hl, -hw), p(-hl, -hw), p(-hl, hw));
    g.tri(a1, b1, c1, col);
    g.tri(a1, c1, d1, col);
}

const OBLIQUE_ATLAS_RLE: &[u8] = include_bytes!("../assets/sprites/oblique_atlas.rle");
const ATLAS_COLS: usize = crate::unit_animation::ATLAS_COLS;
const ATLAS_ROWS: usize = crate::unit_animation::ATLAS_ROWS;
const CELL_DECOY: usize = 8;
const CELL_DECOY_WRECK: usize = 9;
const CELL_HQ: usize = 12;

fn atlas_uv(cell: usize, flip_x: bool) -> [f32; 4] {
    let col = (cell % ATLAS_COLS) as f32;
    let row = (cell / ATLAS_COLS) as f32;
    let u0 = (col + 0.005) / ATLAS_COLS as f32;
    let v0 = (row + 0.005) / ATLAS_ROWS as f32;
    let u1 = (col + 0.995) / ATLAS_COLS as f32;
    let v1 = (row + 0.995) / ATLAS_ROWS as f32;
    if flip_x { [u1, v0, u0, v1] } else { [u0, v0, u1, v1] }
}

fn decode_atlas_rle(data: &[u8]) -> Option<(u32, u32, Vec<u32>)> {
    if data.len() < 8 || &data[..4] != b"YYSP" {
        return None;
    }
    let w = u16::from_le_bytes([data[4], data[5]]) as u32;
    let h = u16::from_le_bytes([data[6], data[7]]) as u32;
    let total = (w as usize).checked_mul(h as usize)?;
    let mut px = Vec::with_capacity(total);
    let mut i = 8;
    while i < data.len() && px.len() < total {
        let tag = data[i];
        i += 1;
        if tag < 128 {
            let cnt = tag as usize + 1;
            if i + cnt * 4 > data.len() {
                return None;
            }
            for _ in 0..cnt {
                px.push(u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]));
                i += 4;
            }
        } else {
            let cnt = tag as usize - 126;
            if i + 4 > data.len() {
                return None;
            }
            let val = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
            i += 4;
            let rem = total - px.len();
            px.resize(px.len() + cnt.min(rem), val);
        }
    }
    if px.len() == total { Some((w, h, px)) } else { None }
}

fn unit_oblique_cell(u: &Unit) -> usize {
    if u.kind == Kind::Decoy && (u.side == 0 || u.revealed) {
        CELL_DECOY
    } else {
        crate::unit_animation::pose(u).0
    }
}

fn unit_oblique_radius(s: &TroopSpec) -> f32 {
    TS * (0.68 + s.len * 0.34)
}

fn unit_flip_x(u: &Unit) -> bool {
    crate::unit_animation::pose(u).1
}

fn push_troop_sprites(g: &mut Gfx, u: &Unit, alpha: f32) {
    let dead = !u.alive;
    let cell = unit_oblique_cell(u);
    let r = unit_oblique_radius(u.spec());
    let flip_x = unit_flip_x(u);
    let tint = if dead { [0.30, 0.30, 0.30, alpha] } else { [1.0, 1.0, 1.0, alpha] };
    g.push_sprite(u.pos.0, u.pos.1 - r * 0.20, r, r, 0.0, atlas_uv(cell, flip_x), tint);
}

fn draw_troop_with_atlas(g: &mut Gfx, atlas: Option<&Texture>, u: &Unit, alpha: f32) {
    if let Some(at) = atlas {
        g.begin_sprites();
        push_troop_sprites(g, u, alpha);
        g.end_sprites(at, true);
    } else {
        draw_troop_fallback(g, u, alpha);
    }
}

fn draw_troop_fallback(g: &mut Gfx, u: &Unit, alpha: f32) {
    let p=u.pos;
    let c=with_a(side_color(u.side),alpha * if u.alive {1.0} else {0.3});
    if u.ti==0 {
        quad(g,p,u.hull,0.0,TS*0.55,TS*0.35,[0.42,0.42,0.36,alpha]);
    } else if u.ti!=3 {
        if (4..=6).contains(&u.ti) { quad(g,p,u.hull,0.0,TS*0.4,TS*0.18,[0.4,0.24,0.12,alpha]); }
        g.circle(p.0,p.1-TS*0.2,TS*0.12,10,c);
        g.line((p.0,p.1-TS*0.1),(p.0,p.1+TS*0.15),5.0,c);
    }
    draw_equipment(g,u,alpha);
}

fn draw_equipment(g: &mut Gfx, u: &Unit, alpha: f32) {
    let p=u.pos;
    let a=if u.spec().turret {u.turret} else {u.hull};
    let point=|l:f32,w:f32| (p.0+a.cos()*l-a.sin()*w,p.1+a.sin()*l+a.cos()*w-TS*0.16);
    let alpha=alpha*if u.alive {1.0} else {0.3};
    let wood=[0.48,0.29,0.12,alpha]; let metal=[0.22,0.23,0.23,alpha];
    match u.ti {
        2|5 => {
            g.line(point(-TS*0.12,0.0),point(TS*0.12,0.0),4.0,wood);
            g.line(point(TS*0.04,0.0),point(TS*0.55,0.0),3.0,metal);
        },
        3 => {
            for side in [-1.0,1.0] {
                let c=point(-TS*0.12,side*TS*0.25);
                g.circle(c.0,c.1,TS*0.17,12,wood);
                g.ring(c.0,c.1,TS*0.17,2.0,12,metal);
                g.line((c.0-TS*0.12,c.1),(c.0+TS*0.12,c.1),1.5,metal);
                g.line((c.0,c.1-TS*0.12),(c.0,c.1+TS*0.12),1.5,metal);
            }
            g.line(point(-TS*0.48,0.0),point(TS*0.15,0.0),8.0,wood);
            g.line(point(-TS*0.15,0.0),point(TS*0.64,0.0),9.0,metal);
            let m=point(TS*0.64,0.0); g.circle(m.0,m.1,4.0,10,[0.08,0.08,0.08,alpha]);
        },
        _=>{}
    }
}

fn draw_fence(g: &mut Gfx, c:(f32,f32), health:f32) {
    let wood=[0.48,0.30,0.15,1.0];
    for n in 0..5 {
        if health<0.5 && n==2 {continue;}
        let x=c.0+(n as f32-2.0)*TS*0.2;
        g.line((x,c.1+TS*0.22),(x,c.1-TS*0.35),4.0,wood);
        g.tri((x-2.0,c.1-TS*0.35),(x+2.0,c.1-TS*0.35),(x,c.1-TS*0.46),wood);
    }
    for y in [-0.16,0.12] {g.line((c.0-TS*0.48,c.1+TS*y),(c.0+TS*0.48,c.1+TS*y),3.0,wood);}
}

fn draw_wreck_fx(g: &mut Gfx, u: &Unit, clock: f32) {
    let p = u.pos;
    let t = u.dead_t;
    let n = 3;
    for i in 0..n {
        let ph = (clock * 0.6 + i as f32 / n as f32).fract();
        let r = TS * (0.2 + ph * 0.5);
        let a = (0.35 * (1.0 - ph)) * (1.0 - (t / 60.0).min(0.8));
        g.circle(p.0 + ph * TS * 0.6, p.1 - ph * TS * 1.2, r, 12, [0.2, 0.2, 0.2, a]);
    }
    if t < 8.0 {
        let fl = 0.5 + 0.5 * (clock * 12.0).sin();
        g.circle(p.0, p.1, TS * 0.18 * (1.0 + fl * 0.3), 8, [1.0, 0.5, 0.1, 0.7 * (1.0 - t / 8.0)]);
    }
}

fn draw_decoy_wreck(g: &mut Gfx, u: &Unit) {
    let s = u.spec();
    let (l, wd) = (s.len * TS, s.wid * TS);
    quad(g, u.pos, u.hull + 0.3, 0.0, l * 0.4, wd * 0.2, [0.35, 0.30, 0.22, 0.7]);
    quad(g, u.pos, u.hull - 0.5, l * 0.1, l * 0.3, wd * 0.12, [0.30, 0.26, 0.20, 0.7]);
}

fn draw_hawk(g: &mut Gfx, u: &Unit, clock: f32) {
    let p=(u.pos.0,u.pos.1-TS*0.2);
    let a=u.hull;
    let point=|l:f32,w:f32| (p.0+a.cos()*l-a.sin()*w,p.1+a.sin()*l+a.cos()*w);
    let brown=[0.34,0.21,0.11,1.0]; let feather=[0.57,0.39,0.21,1.0];
    let wing=TS*(0.45+0.10*(clock*5.0).sin());
    g.circle(u.pos.0+8.0,u.pos.1+14.0,TS*0.14,12,[0.0,0.0,0.0,0.18]);
    g.tri(point(TS*0.15,0.0),point(-TS*0.1,wing),point(-TS*0.16,TS*0.08),feather);
    g.tri(point(TS*0.15,0.0),point(-TS*0.1,-wing),point(-TS*0.16,-TS*0.08),feather);
    g.line(point(-TS*0.23,0.0),point(TS*0.2,0.0),5.0,brown);
    g.tri(point(-TS*0.13,0.0),point(-TS*0.36,TS*0.12),point(-TS*0.36,-TS*0.12),brown);
    g.tri(point(TS*0.2,TS*0.04),point(TS*0.3,0.0),point(TS*0.2,-TS*0.04),[0.85,0.66,0.24,1.0]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders the production cut-in into a hidden swapchain, without driving any desktop UI.
    #[cfg(windows)]
    #[test]
    #[ignore = "requires a Windows graphics device; saves cut-in screenshots for visual QA"]
    fn skill_cutin_preview() {
        use windows::core::w;
        use windows::Win32::UI::WindowsAndMessaging::*;
        struct PreviewWindow(windows::Win32::Foundation::HWND);
        impl Drop for PreviewWindow {
            fn drop(&mut self) { unsafe { let _ = DestroyWindow(self.0); } }
        }
        crate::lang::set_lang(crate::lang::Lang::Ja);
        let destination = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/skill-cutin-preview");
        std::fs::create_dir_all(&destination).unwrap();
        for (width, height) in [(1280, 720), (900, 540)] {
            let hwnd = unsafe { CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Cut-in render test"), WS_POPUP,
                0, 0, width, height, None, None, None, None).unwrap() };
            let _window = PreviewWindow(hwnd);
            let gfx = Gfx::new(hwnd, width as u32, height as u32).unwrap();
            let mut game = Game::new(gfx);
            game.screen = Screen::Battle;
            for general in [1, 12, 0] {
                game.cutins = crate::skill_cutin::Cutins::default();
                game.cutins.push(crate::skill_cutin::SkillCue { side: if general == 0 { 1 } else { 0 }, general });
                for _ in 0..3 { game.cutins.update(0.1); }
                game.gfx.begin([0.08, 0.09, 0.07, 1.0]);
                assert!(game.gfx.d2d_begin());
                game.render_skill_cutin();
                game.gfx.d2d_end();
                let path = destination.join(format!("general-{}-{}x{}.bmp", general, width, height));
                game.gfx.save_backbuffer(path.to_str().unwrap()).unwrap();
                assert!(path.metadata().unwrap().len() > 100_000);
                assert!(game.cutin_art.as_ref().is_some_and(|(id, _)| *id == general));
                game.show_generals = true;
                let time = game.world.time;
                game.key(0x1b, true);
                assert!(game.cutins.current().is_none());
                assert!(game.show_generals); // Esc dismisses the cinematic, preserving the command panel.
                assert_eq!(game.world.time, time);
            }
        }
    }

    #[test]
    fn delegation_trace_catches_units_between_pointer_events() {
        assert!(segment_hits((50.0, 2.0), (0.0, 0.0), (100.0, 0.0), 5.0));
        assert!(segment_hits((50.0, 2.0), (100.0, 0.0), (0.0, 0.0), 5.0));
        assert!(!segment_hits((110.0, 0.0), (0.0, 0.0), (100.0, 0.0), 5.0));
        // A trace must not assign units elsewhere inside its bounding rectangle.
        assert!(!segment_hits((10.0, 90.0), (0.0, 0.0), (100.0, 100.0), 5.0));
    }

    #[test]
    fn delegation_trace_handles_taps_and_returning_to_start() {
        assert!(segment_hits((12.0, 10.0), (10.0, 10.0), (10.0, 10.0), 5.0));
        assert!(!segment_hits((30.0, 10.0), (10.0, 10.0), (10.0, 10.0), 5.0));
        let path = [(0.0, 0.0), (100.0, 0.0), (0.0, 0.0)];
        assert!(path.windows(2).any(|s| segment_hits((50.0, 0.0), s[0], s[1], 5.0)));
    }

    #[test]
    fn ghost_units_exist_for_every_troop() {
        for (n, i) in crate::data::all_troops() {
            let u = ghost_unit(n, i, (100.0, 100.0));
            assert_eq!(u.spec().name, n.troops()[i].name);
            let cell=unit_oblique_cell(&u);
            assert!((crate::unit_animation::MOTION_START..crate::unit_animation::MOTION_START+200).contains(&cell));
        }
    }

    #[test]
    fn oblique_atlas_decodes_cleanly() {
        let (w, h, px) = decode_atlas_rle(OBLIQUE_ATLAS_RLE).expect("valid RLE atlas");
        assert_eq!(w, (ATLAS_COLS * 128) as u32);
        assert_eq!(h, (ATLAS_ROWS * 128) as u32);
        assert_eq!(px.len(), (w * h) as usize);
        assert!(px.iter().any(|&p| p != 0));
    }

    #[test]
    fn every_directional_walk_cell_contains_unique_transparent_artwork() {
        use std::hash::{Hash,Hasher};
        let (width,_,pixels)=decode_atlas_rle(OBLIQUE_ATLAS_RLE).unwrap();
        let mut hashes=std::collections::HashSet::new();
        for cell in crate::unit_animation::MOTION_START..crate::unit_animation::MOTION_START+200 {
            let (x,y)=((cell%ATLAS_COLS)*128,(cell/ATLAS_COLS)*128);
            let mut sprite=Vec::with_capacity(128*128);
            for row in 0..128 {
                let start=(y+row)*width as usize+x;
                sprite.extend_from_slice(&pixels[start..start+128]);
            }
            assert!(sprite.iter().any(|p|p>>24==0), "Missing alpha at cell {cell}");
            assert!(sprite.iter().any(|p|p>>24>200), "Empty sprite at cell {cell}");
            let mut hash=std::collections::hash_map::DefaultHasher::new();
            sprite.hash(&mut hash);
            assert!(hashes.insert(hash.finish()), "Duplicate motion artwork at cell {cell}");
        }
    }
}
