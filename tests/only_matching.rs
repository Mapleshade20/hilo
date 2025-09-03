use hilo::models::{Form, Gender, TagSystem};
use hilo::services::matching::MatchingService;
use std::collections::HashMap;
use std::sync::LazyLock;
use uuid::Uuid;

static TAG_SYSTEM: LazyLock<TagSystem> = LazyLock::new(|| {
    dotenvy::from_filename_override("tests/data/.test.env").unwrap();
    tracing_subscriber::fmt()
        .with_env_filter("trace")
        .with_test_writer()
        .init();

    let test_tags_json = std::fs::read_to_string("tests/data/tags.test.json")
        .expect("Failed to read test_tags.json");

    TagSystem::from_json(&test_tags_json).expect("Failed to load test tag system")
});

fn get_test_tag_system() -> &'static TagSystem {
    &TAG_SYSTEM
}

fn create_test_form(
    user_id: Uuid,
    gender: Gender,
    familiar_tags: Vec<String>,
    aspirational_tags: Vec<String>,
    self_traits: Vec<String>,
    ideal_traits: Vec<String>,
    physical_boundary: i16,
) -> Form {
    Form {
        user_id,
        gender,
        familiar_tags,
        aspirational_tags,
        recent_topics: "Test topic".to_string(),
        self_traits,
        ideal_traits,
        physical_boundary,
        self_intro: "Test intro".to_string(),
        profile_photo_path: None,
    }
}

#[test]
fn test_gender_compatibility_filter() {
    let tag_system = get_test_tag_system();
    let tag_frequencies = HashMap::new();

    // Same gender users should get -1 score
    let user1 = create_test_form(
        Uuid::new_v4(),
        Gender::Male,
        vec!["soccer".to_string()],
        vec!["volleyball".to_string()],
        vec!["humor".to_string()],
        vec!["bookworm".to_string()],
        3,
    );

    let user2 = create_test_form(
        Uuid::new_v4(),
        Gender::Male,
        vec!["soccer".to_string()],
        vec!["volleyball".to_string()],
        vec!["bookworm".to_string()],
        vec!["bookworm".to_string()],
        3,
    );

    let score =
        MatchingService::calculate_match_score(&user1, &user2, tag_system, &tag_frequencies, 20);

    assert_eq!(
        score, -1.0,
        "Same gender users should have -1 compatibility score"
    );
}

#[test]
fn test_physical_boundary() {
    let tag_system = get_test_tag_system();
    let tag_frequencies = HashMap::new();

    // Incompatible physical boundaries should get -1 score
    let user1 = create_test_form(
        Uuid::new_v4(),
        Gender::Male,
        vec![],
        vec!["volleyball".to_string()],
        vec!["humor".to_string()],
        vec!["bookworm".to_string()],
        1, // Low intimacy
    );

    let user2 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec![],
        vec!["volleyball".to_string()],
        vec!["humor".to_string()],
        vec!["bookworm".to_string()],
        3, // High intimacy (difference > 1)
    );

    let score =
        MatchingService::calculate_match_score(&user1, &user2, tag_system, &tag_frequencies, 20);

    assert_eq!(
        score, -1.0,
        "Incompatible physical boundaries should result in -1 score"
    );

    let user2 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec![],
        vec!["volleyball".to_string()],
        vec!["humor".to_string()],
        vec!["bookworm".to_string()],
        2, // Acceptable intimacy (difference = 1)
    );

    let score =
        MatchingService::calculate_match_score(&user1, &user2, tag_system, &tag_frequencies, 20);

    assert_eq!(
        score, 0.0,
        "Difference of 1 in physical boundaries should yield 0 score"
    );

    let user2 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec![],
        vec!["volleyball".to_string()],
        vec!["humor".to_string()],
        vec!["bookworm".to_string()],
        1, // Exact same intimacy
    );

    let score =
        MatchingService::calculate_match_score(&user1, &user2, tag_system, &tag_frequencies, 20);

    assert_eq!(
        score, 1.0,
        "Exact same physical boundaries should yield 1 score"
    );
}

#[test]
fn test_hierarchical_tag_matching() {
    let tag_system = get_test_tag_system();
    let tag_frequencies = HashMap::from([
        // -- Sports tags
        ("sports".to_string(), 15),
        ("soccer".to_string(), 5),
        ("volleyball".to_string(), 3),
        ("basketball".to_string(), 7),
        // -- Desktop tags
        ("competitive".to_string(), 9),
        ("pc_fps".to_string(), 8),
        // -- Arts tags
        ("cooking".to_string(), 2),
        // -- Language tags
        ("language_exchange".to_string(), 12),
        ("spanish".to_string(), 10),
    ]);

    // User with specific tag should match with user having parent tag
    let primary_user = create_test_form(
        Uuid::new_v4(),
        Gender::Male,
        vec!["soccer".to_string()], // specific child tag
        vec!["pc_fps".to_string()],
        vec!["humor".to_string()],
        vec!["bookworm".to_string()],
        3,
    );

    let user1 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec!["basketball".to_string()], // another child tag under "sports"
        vec!["spanish".to_string()],
        vec!["discipline".to_string()],
        vec!["empathy".to_string()],
        2,
    );

    let score_diff = MatchingService::calculate_match_score(
        &primary_user,
        &user1,
        tag_system,
        &tag_frequencies,
        20,
    );

    assert!(
        score_diff > 0.0,
        "Child and parent tags should have positive compatibility"
    );

    // Test that exact matches score higher than hierarchical matches
    let user2 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec!["soccer".to_string()], // same child tag
        vec!["spanish".to_string()],
        vec!["discipline".to_string()],
        vec!["empathy".to_string()],
        2,
    );

    let score_exact = MatchingService::calculate_match_score(
        &primary_user,
        &user2,
        tag_system,
        &tag_frequencies,
        20,
    );

    assert!(
        score_exact > score_diff,
        "Exact tag matches should score higher than hierarchical matches"
    );

    let user3 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec!["cooking".to_string()], // completely different tag
        vec!["spanish".to_string()],
        vec!["discipline".to_string()],
        vec!["empathy".to_string()],
        2,
    );

    let zero_score = MatchingService::calculate_match_score(
        &primary_user,
        &user3,
        tag_system,
        &tag_frequencies,
        20,
    );

    assert_eq!(
        zero_score, 0.0,
        "Completely different tags should yield zero compatibility"
    );
}

#[test]
fn test_idf_scoring() {
    let tag_system = get_test_tag_system();
    let tag_frequencies = HashMap::from([
        // -- Sports tags
        ("soccer".to_string(), 10),    // common
        ("volleyball".to_string(), 2), // rare
        // -- Language tags
        ("language_exchange".to_string(), 12),
        ("spanish".to_string(), 10),
    ]);

    let user1 = create_test_form(
        Uuid::new_v4(),
        Gender::Male,
        vec!["soccer".to_string()],
        vec!["spanish".to_string()],
        vec!["humor".to_string()],
        vec!["bookworm".to_string()],
        3,
    );

    let user2 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec!["soccer".to_string()],
        vec!["spanish".to_string()],
        vec!["discipline".to_string()],
        vec!["empathy".to_string()],
        2,
    );

    let score_common =
        MatchingService::calculate_match_score(&user1, &user2, tag_system, &tag_frequencies, 20);

    let user1 = create_test_form(
        Uuid::new_v4(),
        Gender::Male,
        vec!["volleyball".to_string()],
        vec!["spanish".to_string()],
        vec!["humor".to_string()],
        vec!["bookworm".to_string()],
        3,
    );

    let user2 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec!["volleyball".to_string()],
        vec!["spanish".to_string()],
        vec!["discipline".to_string()],
        vec!["empathy".to_string()],
        2,
    );

    let score_rare =
        MatchingService::calculate_match_score(&user1, &user2, tag_system, &tag_frequencies, 20);

    assert!(
        score_rare > score_common,
        "Rare tag matches should score higher than common tag matches due to IDF"
    );
}

#[test]
fn test_trait_matching() {
    let tag_system = get_test_tag_system();
    let tag_frequencies = HashMap::new();

    // Perfect trait compatibility
    let user1 = create_test_form(
        Uuid::new_v4(),
        Gender::Male,
        vec![],
        vec![],
        vec!["outgoing".to_string(), "funny".to_string()],
        vec!["kind".to_string(), "intelligent".to_string()],
        3,
    );

    let user2 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec![],
        vec![],
        vec!["kind".to_string(), "intelligent".to_string()], // Matches user1's ideals
        vec!["outgoing".to_string(), "funny".to_string()],   // Matches user1's self traits
        3,
    );

    let score_perfect =
        MatchingService::calculate_match_score(&user1, &user2, tag_system, &tag_frequencies, 20);

    // No trait compatibility
    let user3 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec![],
        vec![],
        vec!["serious".to_string(), "quiet".to_string()],
        vec!["artistic".to_string(), "creative".to_string()],
        3,
    );

    let score_no_match =
        MatchingService::calculate_match_score(&user1, &user3, tag_system, &tag_frequencies, 20);

    assert!(
        score_perfect > score_no_match,
        "Perfect trait compatibility should score higher than no compatibility"
    );
}

#[test]
fn test_asymmetric_tag_matching() {
    let tag_system = get_test_tag_system();
    let tag_frequencies = HashMap::from([
        // -- Sports tags
        ("soccer".to_string(), 10),    // common
        ("volleyball".to_string(), 2), // rare
        // -- Language tags
        ("language_exchange".to_string(), 12),
        ("spanish".to_string(), 10),
    ]);

    let user1 = create_test_form(
        Uuid::new_v4(),
        Gender::Male,
        vec!["soccer".to_string()],
        vec!["spanish".to_string()],
        vec!["humor".to_string()],
        vec!["bookworm".to_string()],
        3,
    );

    let user2 = create_test_form(
        Uuid::new_v4(),
        Gender::Female,
        vec!["spanish".to_string()],
        vec!["soccer".to_string()],
        vec!["discipline".to_string()],
        vec!["empathy".to_string()],
        2,
    );

    let score_common =
        MatchingService::calculate_match_score(&user1, &user2, tag_system, &tag_frequencies, 20);

    assert!(
        score_common > 0.0,
        "Asymmetric tag matches should yield positive compatibility"
    );
}
