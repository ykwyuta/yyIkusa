//! Presentation-only skill announcements; simulation clocks never depend on animation.
use crate::general_skills::Trigger;
use crate::generals::GENERALS;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillCue {
    pub side: usize,
    pub general: usize,
}
impl SkillCue {
    fn priority(self) -> u8 {
        match GENERALS[self.general].skill.trigger {
            Trigger::Passive => 0,
            Trigger::Revenge => 1,
            Trigger::Once => 2,
        }
    }
    pub fn duration(self) -> f32 {
        match GENERALS[self.general].skill.trigger {
            Trigger::Passive => 1.4,
            Trigger::Revenge => 1.8,
            Trigger::Once => 2.4,
        }
    }
}
pub struct Cutins {
    current: Option<(SkillCue, f32)>,
    pending: VecDeque<SkillCue>,
    now: f32,
    last: [[f32; GENERALS.len()]; 2],
}
impl Default for Cutins {
    fn default() -> Self {
        Self {
            current: None,
            pending: VecDeque::new(),
            now: 0.0,
            last: [[-100.0; GENERALS.len()]; 2],
        }
    }
}
impl Cutins {
    pub fn current(&self) -> Option<(SkillCue, f32)> {
        self.current
    }
    pub fn push(&mut self, cue: SkillCue) {
        if cue.side >= 2
            || cue.general >= GENERALS.len()
            || self.current.is_some_and(|(c, _)| c == cue)
            || self.pending.contains(&cue)
            || self.now - self.last[cue.side][cue.general] < 6.0
        {
            return;
        }
        if self
            .current
            .is_none_or(|(c, _)| cue.priority() > c.priority())
        {
            self.start(cue);
        } else {
            if self.pending.len() >= 16 {
                if let Some(i) = self
                    .pending
                    .iter()
                    .position(|c| c.priority() < cue.priority())
                {
                    self.pending.remove(i);
                } else {
                    return;
                }
            }
            let at = self
                .pending
                .iter()
                .position(|c| c.priority() < cue.priority())
                .unwrap_or(self.pending.len());
            self.pending.insert(at, cue);
        }
    }
    fn start(&mut self, cue: SkillCue) {
        self.last[cue.side][cue.general] = self.now;
        self.current = Some((cue, 0.0));
    }
    pub fn skip(&mut self) {
        self.current = None;
        if let Some(cue) = self.pending.pop_front() {
            self.start(cue);
        }
    }
    pub fn update(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.25);
        self.now += dt;
        if let Some((cue, elapsed)) = &mut self.current {
            *elapsed += dt;
            if *elapsed >= cue.duration() {
                self.skip();
            }
        }
    }
}
pub fn decode(data: &[u8]) -> Option<(u32, u32, Vec<u32>)> {
    if data.get(..4)? != b"YYCI" {
        return None;
    }
    let w = u16::from_le_bytes(data.get(4..6)?.try_into().ok()?) as u32;
    let h = u16::from_le_bytes(data.get(6..8)?.try_into().ok()?) as u32;
    let total = w.checked_mul(h)? as usize;
    if w == 0 || h == 0 || total > 1024 * 768 {
        return None;
    }
    let mut palette = [0u32; 256];
    for (i, p) in data.get(8..776)?.chunks_exact(3).enumerate() {
        palette[i] = u32::from_le_bytes([p[2], p[1], p[0], 255]);
    }
    let mut pos = 776;
    let mut pixels = Vec::with_capacity(total);
    while pixels.len() < total {
        let tag = *data.get(pos)?;
        pos += 1;
        if tag < 128 {
            let count = tag as usize + 1;
            if count > total - pixels.len() {
                return None;
            }
            for &index in data.get(pos..pos + count)? {
                pixels.push(palette[index as usize]);
            }
            pos += count;
        } else {
            let count = tag as usize - 125;
            if count > total - pixels.len() {
                return None;
            }
            let color = palette[*data.get(pos)? as usize];
            pos += 1;
            pixels.resize(pixels.len() + count, color);
        }
    }
    (pos == data.len()).then_some((w, h, pixels))
}
#[path = "skill_cutin_art.rs"]
mod art;
pub fn art(id: usize) -> Option<(u32, u32, Vec<u32>)> {
    decode(art::DATA.get(id)?)
}
#[cfg(test)]
#[path = "skill_cutin_tests.rs"]
mod tests;
