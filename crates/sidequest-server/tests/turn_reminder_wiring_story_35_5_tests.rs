//! Wiring tests for Story 35-5: Wire turn_reminder into barrier creation.
//!
//! Verifies that:
//! 1. turn_reminder is imported in production server code (non-test consumer)
//! 2. lib.rs spawns a reminder task after barrier creation
//! 3. connect.rs spawns a reminder task after barrier creation
//! 4. OTEL "reminder_spawned" event is emitted
//! 5. OTEL "reminder_fired" event is emitted
//! 6. ReminderConfig is constructed (not just default) from genre pack or defaults
//! 7. tokio::spawn is used for async reminder task

// ===========================================================================
// 1. Non-test consumer — turn_reminder must be imported in sidequest-server
// ===========================================================================

#[test]
fn wiring_server_imports_turn_reminder() {
    // Check lib.rs for a use/import of turn_reminder
    let lib_source = include_str!("../src/lib.rs");
    let lib_prod = lib_source.split("#[cfg(test)]").next().unwrap_or(lib_source);

    let connect_source = include_str!("../src/dispatch/connect.rs");
    let connect_prod = connect_source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(connect_source);

    let has_import = lib_prod.contains("turn_reminder")
        || connect_prod.contains("turn_reminder");
    assert!(
        has_import,
        "sidequest-server must have a non-test reference to turn_reminder — story 35-5"
    );
}

// ===========================================================================
// 2. lib.rs — reminder spawned after barrier creation
// ===========================================================================

#[test]
fn wiring_lib_spawns_reminder_after_barrier() {
    let source = include_str!("../src/lib.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);

    // After TurnBarrier::new(), there must be a reminder spawn
    assert!(
        production_code.contains("run_reminder"),
        "lib.rs must call run_reminder after barrier creation — story 35-5"
    );
}

#[test]
fn wiring_lib_uses_tokio_spawn_for_reminder() {
    let source = include_str!("../src/lib.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);

    // The reminder is async — it must be spawned, not awaited inline
    // (blocking the barrier creation path would defeat the purpose)
    let has_spawn = production_code.contains("tokio::spawn")
        && production_code.contains("reminder");
    assert!(
        has_spawn,
        "lib.rs must use tokio::spawn for the reminder task — story 35-5"
    );
}

// ===========================================================================
// 3. connect.rs — reminder spawned after barrier creation
// ===========================================================================

#[test]
fn wiring_connect_spawns_reminder_after_barrier() {
    let source = include_str!("../src/dispatch/connect.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);

    assert!(
        production_code.contains("run_reminder"),
        "connect.rs must call run_reminder after barrier creation — story 35-5"
    );
}

#[test]
fn wiring_connect_uses_tokio_spawn_for_reminder() {
    let source = include_str!("../src/dispatch/connect.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);

    let has_spawn = production_code.contains("tokio::spawn")
        && production_code.contains("reminder");
    assert!(
        has_spawn,
        "connect.rs must use tokio::spawn for the reminder task — story 35-5"
    );
}

// ===========================================================================
// 4. OTEL — reminder_spawned event emitted
// ===========================================================================

#[test]
fn wiring_emits_reminder_spawned_otel() {
    let lib_source = include_str!("../src/lib.rs");
    let connect_source = include_str!("../src/dispatch/connect.rs");

    let has_event = lib_source.contains("reminder_spawned")
        || connect_source.contains("reminder_spawned");
    assert!(
        has_event,
        "Server must emit 'reminder_spawned' OTEL watcher event — story 35-5"
    );
}

// ===========================================================================
// 5. OTEL — reminder_fired event emitted (when idle players detected)
// ===========================================================================

#[test]
fn wiring_emits_reminder_fired_otel() {
    let lib_source = include_str!("../src/lib.rs");
    let connect_source = include_str!("../src/dispatch/connect.rs");

    // The reminder_fired event should be in the async reminder task,
    // which could be in either file or in a helper module
    let all_source = format!("{}{}", lib_source, connect_source);
    assert!(
        all_source.contains("reminder_fired"),
        "Server must emit 'reminder_fired' OTEL watcher event with idle_player_count — story 35-5"
    );
}

// ===========================================================================
// 6. ReminderConfig construction — not just Default, must be loaded
// ===========================================================================

#[test]
fn wiring_constructs_reminder_config() {
    let lib_source = include_str!("../src/lib.rs");
    let connect_source = include_str!("../src/dispatch/connect.rs");

    let all_source = format!("{}{}", lib_source, connect_source);
    assert!(
        all_source.contains("ReminderConfig"),
        "Server must construct ReminderConfig (from genre pack or default) — story 35-5"
    );
}

// ===========================================================================
// 7. Reminder receives barrier timeout and turn mode
// ===========================================================================

#[test]
fn wiring_reminder_receives_turn_mode() {
    let lib_source = include_str!("../src/lib.rs");
    let connect_source = include_str!("../src/dispatch/connect.rs");

    let all_source = format!("{}{}", lib_source, connect_source);
    // run_reminder takes turn mode — it must be passed through
    assert!(
        all_source.contains("turn_mode") && all_source.contains("run_reminder"),
        "Reminder task must receive turn_mode for mode-aware checks — story 35-5"
    );
}
