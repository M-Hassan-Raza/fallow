use super::common::{create_config, fixture_path};

// ── Enum/class members integration ─────────────────────────────

#[test]
fn enum_class_members_detects_unused_members() {
    let root = fixture_path("enum-class-members");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_enum_member_names: Vec<&str> = results
        .unused_enum_members
        .iter()
        .map(|m| m.member_name.as_str())
        .collect();

    // Only Status.Active is used; Inactive and Pending should be unused
    assert!(
        unused_enum_member_names.contains(&"Inactive"),
        "Inactive should be detected as unused enum member, found: {unused_enum_member_names:?}"
    );
    assert!(
        unused_enum_member_names.contains(&"Pending"),
        "Pending should be detected as unused enum member, found: {unused_enum_member_names:?}"
    );

    let unused_class_member_names: Vec<&str> = results
        .unused_class_members
        .iter()
        .map(|m| m.member_name.as_str())
        .collect();

    // unusedMethod is never called
    assert!(
        unused_class_member_names.contains(&"unusedMethod"),
        "unusedMethod should be detected as unused class member, found: {unused_class_member_names:?}"
    );

    // greet() is called via instance: `const svc = new MyService(); svc.greet()`
    assert!(
        !unused_class_member_names.contains(&"greet"),
        "greet should NOT be unused (called via instance), found: {unused_class_member_names:?}"
    );

    // name property is never accessed (not via svc.name or this.name)
    assert!(
        unused_class_member_names.contains(&"name"),
        "name should be detected as unused class property, found: {unused_class_member_names:?}"
    );
}

// ── Whole-object enum member heuristics ────────────────────────

#[test]
fn enum_whole_object_uses_no_false_positives() {
    let root = fixture_path("enum-whole-object");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_enum_member_names: Vec<&str> = results
        .unused_enum_members
        .iter()
        .map(|m| m.member_name.as_str())
        .collect();

    // Status used via Object.values — no members should be unused
    assert!(
        !unused_enum_member_names.contains(&"Active"),
        "Active should not be unused (Object.values), found: {unused_enum_member_names:?}"
    );
    assert!(
        !unused_enum_member_names.contains(&"Inactive"),
        "Inactive should not be unused (Object.values), found: {unused_enum_member_names:?}"
    );
    assert!(
        !unused_enum_member_names.contains(&"Pending"),
        "Pending should not be unused (Object.values), found: {unused_enum_member_names:?}"
    );

    // Direction used via Object.keys — no members should be unused
    assert!(
        !unused_enum_member_names.contains(&"Up"),
        "Up should not be unused (Object.keys), found: {unused_enum_member_names:?}"
    );
    assert!(
        !unused_enum_member_names.contains(&"Down"),
        "Down should not be unused (Object.keys), found: {unused_enum_member_names:?}"
    );

    // Color used via for..in — no members should be unused
    assert!(
        !unused_enum_member_names.contains(&"Red"),
        "Red should not be unused (for..in), found: {unused_enum_member_names:?}"
    );
    assert!(
        !unused_enum_member_names.contains(&"Green"),
        "Green should not be unused (for..in), found: {unused_enum_member_names:?}"
    );

    // Priority — only High accessed via computed literal, Low and Medium should be unused
    assert!(
        unused_enum_member_names.contains(&"Low"),
        "Low should be unused (only High accessed via computed), found: {unused_enum_member_names:?}"
    );
    assert!(
        unused_enum_member_names.contains(&"Medium"),
        "Medium should be unused (only High accessed via computed), found: {unused_enum_member_names:?}"
    );
}

// ── Type-level enum member usage ──────────────────────────────

#[test]
fn enum_type_level_usage_no_false_positives() {
    let root = fixture_path("enum-type-level");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_enum_member_names: Vec<&str> = results
        .unused_enum_members
        .iter()
        .map(|m| m.member_name.as_str())
        .collect();

    // BreakpointString used as mapped type constraint — all members should be used
    assert!(
        !unused_enum_member_names.contains(&"xs"),
        "xs should not be unused (mapped type constraint), found: {unused_enum_member_names:?}"
    );
    assert!(
        !unused_enum_member_names.contains(&"xxl"),
        "xxl should not be unused (mapped type constraint), found: {unused_enum_member_names:?}"
    );

    // Status.Active used via qualified type name, Status.Inactive via runtime access
    assert!(
        !unused_enum_member_names.contains(&"Active"),
        "Active should not be unused (type qualified name), found: {unused_enum_member_names:?}"
    );
    assert!(
        !unused_enum_member_names.contains(&"Inactive"),
        "Inactive should not be unused (runtime access), found: {unused_enum_member_names:?}"
    );

    // Status.Pending is not used in any way — should be unused
    assert!(
        unused_enum_member_names.contains(&"Pending"),
        "Pending should be unused (no type-level or runtime access), found: {unused_enum_member_names:?}"
    );

    // Color used via Record<Color, string> — all members should be used
    assert!(
        !unused_enum_member_names.contains(&"Red"),
        "Red should not be unused (Record<Color, T>), found: {unused_enum_member_names:?}"
    );
    assert!(
        !unused_enum_member_names.contains(&"Blue"),
        "Blue should not be unused (Record<Color, T>), found: {unused_enum_member_names:?}"
    );

    // Direction used via { [K in keyof typeof Direction]: ... } — all members should be used
    assert!(
        !unused_enum_member_names.contains(&"Up"),
        "Up should not be unused (keyof typeof in mapped type), found: {unused_enum_member_names:?}"
    );
    assert!(
        !unused_enum_member_names.contains(&"Right"),
        "Right should not be unused (keyof typeof in mapped type), found: {unused_enum_member_names:?}"
    );
}
