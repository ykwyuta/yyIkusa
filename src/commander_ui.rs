//! Supreme command controls, shared by mouse, touch and C/Escape keyboard input.
use super::*;
use crate::commander_skills::{CommanderSkill, ALL, RECON_RADIUS, SUPPLY_RADIUS};

impl Game {
    pub(super) fn toggle_commander_panel(&mut self) {
        self.show_commander = !self.show_commander;
        self.commander_aim = false;
        self.show_generals = false;
        self.attack_mode = false;
        self.ldown = None;
        self.delegate_stroke = None;
        self.painting = false;
    }

    pub(super) fn begin_commander_use(&mut self) {
        if self.show_help || self.world.phase == Phase::Over {
            return;
        }
        if self.world.phase == Phase::Prep {
            self.toggle_commander_panel();
            return;
        }
        if !self.world.commander_ready() {
            let state = &self.world.commander_skill;
            let hint = if state.remaining() == 0 {
                t!(
                    "この戦闘では使用済みです",
                    "No uses left this battle",
                    "本场战斗次数已用完"
                )
            } else if state.selected == CommanderSkill::Reserves {
                t!(
                    "開戦前に予備部隊を購入してください",
                    "Purchase reserves before battle",
                    "请在开战前购买预备队"
                )
            } else {
                t!(
                    "直轄の戦闘部隊が必要です",
                    "You need directly controlled combat units",
                    "需要直属战斗部队"
                )
            };
            self.notify(hint);
            return;
        }
        if self.world.commander_skill.selected.targeted() {
            self.commander_aim = !self.commander_aim;
            self.show_commander = false;
            self.show_generals = false;
            self.attack_mode = false;
            self.ldown = None;
            self.delegate_stroke = None;
        } else {
            self.activate_commander(None);
        }
    }

    pub(super) fn activate_commander(&mut self, target: Option<(f32, f32)>) {
        let first = self.world.units.len();
        if self.world.use_commander_skill(target) {
            self.commander_aim = false;
            self.show_commander = false;
            if self.world.units.len() > first {
                self.sel = (first..self.world.units.len()).collect();
                self.cam = self.world.base_pos(0);
                self.clamp_cam();
            }
        } else {
            self.notify(
                if self.world.commander_skill.selected == CommanderSkill::Reserves {
                    t!(
                        "基地の周囲に予備隊を配置する空きがありません",
                        "Make room around your base for the reserves",
                        "基地周围没有足够空间部署预备队"
                    )
                } else if self.world.commander_skill.selected == CommanderSkill::EagleEye {
                    t!(
                        "地図の内側の地点を指定してください",
                        "Choose a location inside the battlefield",
                        "请选择战场内的地点"
                    )
                } else {
                    t!(
                        "回復が必要な味方のいる地点を指定してください",
                        "Choose a location with damaged friendly units",
                        "请选择有受损友军的地点"
                    )
                },
            );
        }
    }

    pub(super) fn render_commander_widget(&mut self) {
        if self.world.phase == Phase::Over {
            return;
        }
        let skill = self.world.commander_skill.selected;
        let panel_width = if self.gfx.compact { 410.0 } else { 486.0 };
        let preferred = if self.gfx.compact {
            282.0_f32
        } else {
            326.0_f32
        };
        let width = if self.show_commander || self.show_generals {
            preferred.min((self.gfx.width as f32 - panel_width - 30.0).max(210.0))
        } else {
            preferred
        };
        let x = 10.0;
        let y = 44.0;
        self.panel([x, y, x + width, y + 66.0]);
        let count = self.world.commander_skill.remaining();
        let title = tf!(
            "{} C  残り{}回",
            "{} C  ·  {} left",
            "{} C  ·  剩余{}次",
            skill.name(),
            count
        );
        self.button(
            [x + 5.0, y + 4.0, x + width - 60.0, y + 32.0],
            &title,
            Act::CommanderUse,
            self.commander_aim,
        );
        self.button(
            [x + width - 55.0, y + 4.0, x + width - 5.0, y + 32.0],
            t!("詳細", "Info", "详情"),
            Act::CommanderPanel,
            self.show_commander,
        );
        let remaining = self.world.commander_skill.active_remaining(self.world.time);
        let hint = if self.world.phase == Phase::Prep {
            t!(
                "総司令官スキル：開戦前に1つ選択",
                "Supreme command: choose one before battle",
                "总指挥技能：开战前选择一项"
            )
            .to_string()
        } else if self.commander_aim {
            t!(
                "地図上の地点を指定／Escで取消",
                "Pick a map location / Esc to cancel",
                "选择地图地点／Esc取消"
            )
            .to_string()
        } else if remaining > 0.0 {
            tf!(
                "発動中：残り{}秒",
                "Active: {}s remaining",
                "生效中：剩余{}秒",
                remaining.ceil() as u32
            )
        } else if skill == CommanderSkill::Reserves {
            tf!(
                "待機中の予備部隊：{}",
                "Reserve units waiting: {}",
                "待命预备队：{}",
                self.world.commander_skill.reserves.len()
            )
        } else {
            t!(
                "クリック／Cで発動・詳細で効果を確認",
                "Click / C to use; Info for effects",
                "点击／C发动，详情查看效果"
            )
            .to_string()
        };
        self.gfx.text(
            &hint,
            x + 8.0,
            y + 37.0,
            width - 16.0,
            23.0,
            Font::Small,
            skill.color(),
        );

        if self.commander_aim && !self.over_ui(self.mouse) {
            let radius = if skill == CommanderSkill::EagleEye {
                RECON_RADIUS
            } else {
                SUPPLY_RADIUS
            };
            let p = self.mouse;
            self.gfx
                .ring(p.0, p.1, radius * TS * self.zoom, 2.0, 64, skill.color());
            self.gfx.text_shadow(
                skill.name(),
                p.0 + 14.0,
                p.1 + 12.0,
                230.0,
                24.0,
                Font::Big,
                skill.color(),
            );
        }
        for &(at, until) in &self.world.commander_skill.recon {
            if self.world.time < until {
                let p = self.to_screen(at);
                self.gfx.ring(
                    p.0,
                    p.1,
                    RECON_RADIUS * TS * self.zoom,
                    1.0,
                    64,
                    [0.35, 0.85, 1.0, 0.45],
                );
            }
        }
    }

    pub(super) fn render_commander_panel(&mut self) {
        let width = if self.gfx.compact { 410.0 } else { 486.0 };
        let right = self.gfx.width as f32 - 8.0;
        let left = right - width;
        let top = 44.0;
        let bottom = (self.gfx.height as f32 - 8.0).min(630.0);
        let x = left + 10.0;
        let inner = width - 20.0;
        let compact = self.gfx.compact || bottom - top < 490.0;
        let prep = self.world.phase == Phase::Prep;
        self.panel([left, top, right, bottom]);
        self.gfx.text(
            t!("総司令官スキル", "Supreme command", "总指挥技能"),
            x,
            top + 7.0,
            inner,
            25.0,
            Font::Big,
            GOLD,
        );
        let ch = if compact { 32.0 } else { 42.0 };
        let cw = (inner - 6.0) / 2.0;
        let mut y = top + 36.0;
        for (i, skill) in ALL.into_iter().enumerate() {
            let bx = x + (i % 2) as f32 * (cw + 6.0);
            let by = y + (i / 2) as f32 * (ch + 4.0);
            let label = tf!(
                "{}（{}回）",
                "{} ({})",
                "{}（{}次）",
                skill.name(),
                skill.max_uses()
            );
            if prep {
                self.button(
                    [bx, by, bx + cw, by + ch],
                    &label,
                    Act::SelectCommander(skill),
                    self.world.commander_skill.selected == skill,
                );
            } else {
                self.gfx.text(
                    &label,
                    bx + 4.0,
                    by,
                    cw - 8.0,
                    ch,
                    Font::Normal,
                    if self.world.commander_skill.selected == skill {
                        GOLD
                    } else {
                        DIM
                    },
                );
            }
        }
        y += 2.0 * (ch + 4.0) + 4.0;
        let skill = self.world.commander_skill.selected;
        let dh = if compact { 34.0 } else { 48.0 };
        self.gfx
            .text(skill.description(), x, y, inner, dh, Font::Small, TEXT);
        y += dh + 6.0;
        if skill == CommanderSkill::Reserves && prep {
            if NATIONS.len() > 1 {
                let nw = (inner - 3.0 * (NATIONS.len() - 1) as f32) / NATIONS.len() as f32;
                for (i, nation) in NATIONS.iter().enumerate() {
                    let bx = x + i as f32 * (nw + 3.0);
                    self.button(
                        [bx, y, bx + nw, y + 22.0],
                        nation.name(),
                        Act::Tab(i),
                        self.tab == i,
                    );
                }
                y += 26.0;
            }
            let nation = NATIONS[self.tab];
            let row_h = if compact { 25.0 } else { 31.0 };
            for (i, spec) in nation.troops().iter().enumerate() {
                let bx = x + (i % 2) as f32 * (cw + 6.0);
                let by = y + (i / 2) as f32 * row_h;
                let count = self
                    .world
                    .commander_skill
                    .reserves
                    .iter()
                    .filter(|u| u.nation == nation && u.ti == i)
                    .count();
                let label = format!("{}  {}  ×{}", spec.name(), spec.cost, count);
                self.button(
                    [bx, by, bx + cw, by + row_h - 3.0],
                    &label,
                    Act::BuyReserve(nation, i),
                    count > 0,
                );
            }
            y += nation.troops().len().div_ceil(2) as f32 * row_h + 4.0;
            let summary = tf!(
                "予備 {}隊 / 費用 {} / 残予算 {}",
                "Reserve {} / Cost {} / Budget {}",
                "预备 {}队 / 费用 {} / 预算 {}",
                self.world.commander_skill.reserves.len(),
                self.world.commander_skill.reserve_cost(),
                self.world.left(0)
            );
            self.gfx
                .text(&summary, x, y, inner, 22.0, Font::Small, GOLD);
            self.button(
                [x, bottom - 67.0, x + inner, bottom - 39.0],
                t!(
                    "最後の予備部隊を取り消す・返金",
                    "Undo last reserve purchase",
                    "取消最后购买并退款"
                ),
                Act::RefundReserve,
                false,
            );
        } else {
            let hint = if prep {
                t!(
                    "開戦後は選び直せません。Cキーまたは左上のボタンで使用します。",
                    "Selection locks at battle start. Use C or the upper-left button.",
                    "开战后不能更换。按C或左上按钮使用。"
                )
            } else {
                t!(
                    "選んだスキルはこの戦闘中、変更できません。",
                    "Your selected skill is locked for this battle.",
                    "本场战斗中不能更换技能。"
                )
            };
            self.gfx
                .text(hint, x, y + 4.0, inner, 48.0, Font::Small, DIM);
        }
        if prep {
            self.button(
                [x, bottom - 34.0, x + inner, bottom - 7.0],
                t!("配置に戻る", "Back to deployment", "返回部署"),
                Act::CommanderPanel,
                false,
            );
        } else {
            self.button(
                [x, bottom - 34.0, x + cw, bottom - 7.0],
                t!("閉じる", "Close", "关闭"),
                Act::CommanderPanel,
                false,
            );
            self.button(
                [x + cw + 6.0, bottom - 34.0, x + inner, bottom - 7.0],
                t!("発動／地点指定 C", "Use / Target C", "发动／指定地点 C"),
                Act::CommanderUse,
                self.world.commander_ready(),
            );
        }
    }
}
