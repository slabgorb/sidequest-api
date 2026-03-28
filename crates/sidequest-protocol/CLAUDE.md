# sidequest-protocol — Feature Inventory

WebSocket protocol types. **~1,000 LOC, fully complete.** Defines all message types
between server and client.

## COMPLETE — Do Not Rewrite

- **GameMessage** — `message.rs` (763 LOC) — tagged enum with 23 variants covering
  the entire WebSocket protocol: PLAYER_ACTION, NARRATION, NARRATION_CHUNK,
  NARRATION_END, SESSION_EVENT, CHARACTER_CREATION, TURN_STATUS, PARTY_STATUS,
  COMBAT_EVENT, IMAGE, AUDIO_CUE, TTS_START/CHUNK/END, VOICE_*, ERROR, etc.
- **NarrationPayload** — text + state_delta + footnotes. Uses `#[serde(deny_unknown_fields)]`.
  **Has 3 fields only: text, state_delta, footnotes. Do NOT add fields without
  updating both server and client.**
- **StateDelta** — state changes resulting from narration.
- **Footnote / FactCategory** — knowledge extraction from narrator (story 9-11).
- **NonBlankString** — `types.rs` (98 LOC) — validated newtype, rejects empty/whitespace.
- **sanitize_player_text()** — `sanitize.rs` (119 LOC) — strips XML tags, prompt
  injection vectors, unicode confusables, zero-width chars.
- **CharacterState, InitialState, PartyMember, InventoryItem, ExploredLocation,
  CombatEnemy** — all payload types for client state.

## Important Constraints

- `#[serde(deny_unknown_fields)]` on payloads — adding a field to a struct without
  updating the client will cause deserialization failures.
- Protocol version: `PROTOCOL_VERSION: "0.1.0"`
- Zero TODO/FIXME — this crate is done.
