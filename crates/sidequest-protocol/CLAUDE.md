# sidequest-protocol — Feature Inventory

WebSocket protocol types. **~6,300 LOC, fully complete.** Defines all message types
between server and client.

## COMPLETE — Do Not Rewrite

- **GameMessage** — `message.rs` — tagged enum covering the entire WebSocket
  protocol: PLAYER_ACTION, NARRATION, NARRATION_END, SESSION_EVENT,
  CHARACTER_CREATION, TURN_STATUS, PARTY_STATUS, IMAGE, AUDIO_CUE, VOICE_*,
  ERROR, and more. The NARRATION_CHUNK variant and the TTS_START/CHUNK/END
  variants were retired in Epic 27 / ADR-076 when the Kokoro TTS pipeline was
  removed; narration delivery is now a simplified two-message flow (NARRATION
  + NARRATION_END).
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
