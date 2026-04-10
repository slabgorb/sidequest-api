//! Delta-to-patch derivation for turn record capture.

/// Derive patch summaries from a state delta for turn record capture.
pub(super) fn derive_patches_from_delta(
    delta: &sidequest_game::StateDelta,
) -> Vec<sidequest_agents::turn_record::PatchSummary> {
    use sidequest_agents::turn_record::PatchSummary;
    let mut patches = Vec::new();
    if delta.characters_changed() {
        patches.push(PatchSummary { patch_type: "characters".into(), fields_changed: vec!["characters".into()] });
    }
    if delta.npcs_changed() {
        patches.push(PatchSummary { patch_type: "npcs".into(), fields_changed: vec!["npcs".into()] });
    }
    if delta.location_changed() {
        let mut fields = vec!["location".into()];
        if let Some(loc) = delta.new_location() {
            fields.push(format!("new_location:{loc}"));
        }
        patches.push(PatchSummary { patch_type: "location".into(), fields_changed: fields });
    }
    if delta.quest_log_changed() {
        patches.push(PatchSummary { patch_type: "quest_log".into(), fields_changed: vec!["quest_log".into()] });
    }
    if delta.atmosphere_changed() {
        patches.push(PatchSummary { patch_type: "atmosphere".into(), fields_changed: vec!["atmosphere".into()] });
    }
    if delta.regions_changed() {
        patches.push(PatchSummary { patch_type: "regions".into(), fields_changed: vec!["regions".into()] });
    }
    if delta.tropes_changed() {
        patches.push(PatchSummary { patch_type: "tropes".into(), fields_changed: vec!["tropes".into()] });
    }
    patches
}
