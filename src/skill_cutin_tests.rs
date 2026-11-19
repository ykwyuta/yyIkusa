use super::*;

fn cue(trigger: Trigger, side: usize) -> SkillCue {
    SkillCue {
        side,
        general: GENERALS
            .iter()
            .position(|g| g.skill.trigger == trigger)
            .unwrap(),
    }
}

#[test]
fn deliberate_activation_preempts_passives_and_duplicate_losses_do_not_extend_animation() {
    let mut queue = Cutins::default();
    let passive = cue(Trigger::Passive, 0);
    let active = cue(Trigger::Once, 0);
    queue.push(passive);
    queue.update(0.2);
    queue.push(active);
    assert_eq!(queue.current(), Some((active, 0.0)));
    queue.update(0.1);
    queue.push(active);
    assert_eq!(queue.current(), Some((active, 0.1)));
    assert!(queue.pending.is_empty());
    queue.skip();
    assert!(queue.current().is_none());
}

#[test]
fn repeated_revenge_is_coalesced_then_can_show_again_after_cooldown() {
    let mut queue = Cutins::default();
    let revenge = cue(Trigger::Revenge, 0);
    queue.push(revenge);
    for _ in 0..100 {
        queue.push(revenge);
    }
    assert!(queue.pending.is_empty());
    for _ in 0..20 {
        queue.update(0.1);
    }
    assert!(queue.current().is_none());
    queue.push(revenge);
    assert!(queue.current().is_none());
    for _ in 0..50 {
        queue.update(0.1);
    }
    queue.push(revenge);
    assert_eq!(queue.current(), Some((revenge, 0.0)));
}

#[test]
fn animations_end_on_presentation_time_and_invalid_cues_are_ignored() {
    let mut queue = Cutins::default();
    queue.push(SkillCue {
        side: 2,
        general: 0,
    });
    queue.push(SkillCue {
        side: 0,
        general: GENERALS.len(),
    });
    assert!(queue.current().is_none());
    let active = cue(Trigger::Once, 1);
    queue.push(active);
    for _ in 0..23 {
        queue.update(0.1);
    }
    assert!(queue.current().is_some());
    queue.update(0.11);
    assert!(queue.current().is_none());
}

#[test]
fn queue_is_bounded_and_all_waiting_cues_are_unique() {
    let mut queue = Cutins::default();
    for side in 0..2 {
        for general in 0..GENERALS.len() {
            let cue = SkillCue { side, general };
            for _ in 0..3 {
                queue.push(cue);
            }
        }
    }
    assert!(queue.pending.len() <= 16);
    let mut seen = std::collections::HashSet::new();
    for c in &queue.pending {
        assert!(seen.insert((c.side, c.general)));
    }
}

#[test]
fn every_general_has_a_distinct_opaque_large_cutin_within_the_wasm_budget() {
    use std::hash::{Hash, Hasher};
    assert_eq!(art::DATA.len(), GENERALS.len());
    assert!(art::DATA.iter().map(|d| d.len()).sum::<usize>() < 8_000_000);
    let mut hashes = std::collections::HashSet::new();
    for id in 0..GENERALS.len() {
        let (w, h, pixels) = super::art(id).expect("valid generated cut-in");
        assert_eq!((w, h), (768, 512));
        assert_eq!(pixels.len(), (w * h) as usize);
        assert!(pixels.iter().all(|p| p >> 24 == 255));
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        pixels.hash(&mut hasher);
        assert!(
            hashes.insert(hasher.finish()),
            "Duplicate cut-in image at {id}"
        );
    }
    assert!(super::art(GENERALS.len()).is_none());
}

#[test]
fn indexed_decoder_rejects_truncated_oversized_and_trailing_data() {
    let data = art::DATA[0];
    for len in [0, 3, 7, 775, data.len() - 1] {
        assert!(decode(&data[..len]).is_none());
    }
    let mut oversized = data.to_vec();
    oversized[4..8].copy_from_slice(&[255, 255, 255, 255]);
    assert!(decode(&oversized).is_none());
    let mut trailing = data.to_vec();
    trailing.push(0);
    assert!(decode(&trailing).is_none());
}
