use super::*;
use crate::models::*;
use crate::snapshot::*;

#[test]
fn category_upsert_list_and_assignment_round_trip() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let skill_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &now_rfc3339())
                .expect("upsert canonical skill")
                .expect("skill key");
        tx.commit().expect("commit canonical skill");

        let parent = upsert_category(MarketplaceCategoryUpsert {
            label: " AI Agents ".to_string(),
            slug: None,
            parent_id: None,
            position: 2,
        })
        .expect("upsert parent category");
        assert_eq!(parent.id, "ai-agents");
        assert_eq!(parent.label, "AI Agents");
        assert_eq!(parent.slug, "ai-agents");

        let child = upsert_category(MarketplaceCategoryUpsert {
            label: "Code Review".to_string(),
            slug: Some("Code_Review".to_string()),
            parent_id: Some(parent.id.clone()),
            position: 1,
        })
        .expect("upsert child category");
        assert_eq!(child.id, "code-review");
        assert_eq!(child.parent_id.as_deref(), Some("ai-agents"));

        let assigned =
            crate::snapshot::assign_categories_to_skill(MarketplaceSkillCategoryAssignmentInput {
                skill_key: skill_key.clone(),
                category_ids: vec!["AI Agents".to_string(), "code_review".to_string()],
            })
            .expect("assign categories");
        assert_eq!(assigned.len(), 2);
        assert_eq!(assigned[0].category_id, "code-review");
        assert_eq!(assigned[1].category_id, "ai-agents");

        let categories = crate::snapshot::list_categories().expect("list categories");
        assert_eq!(categories.len(), 2);
        assert_eq!(categories[0].id, "ai-agents");
    });
}

#[test]
fn tag_upsert_list_assignment_and_usage_count_round_trip() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let search_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &now_rfc3339())
                .expect("upsert search skill")
                .expect("search key");
        let screenshot_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "screenshot", 10, &now_rfc3339())
                .expect("upsert screenshot skill")
                .expect("screenshot key");
        tx.commit().expect("commit skills");

        let tag = upsert_tag(MarketplaceTagUpsert {
            label: " Rust Tools ".to_string(),
            slug: None,
        })
        .expect("upsert tag");
        assert_eq!(tag.slug, "rust-tools");
        assert_eq!(tag.label, "Rust Tools");
        assert_eq!(tag.usage_count, 0);

        let assigned = crate::snapshot::assign_tags_to_skill(MarketplaceSkillTagAssignmentInput {
            skill_key: search_key.clone(),
            tag_slugs: vec!["Rust_Tools".to_string(), "ai helper".to_string()],
            source_id: Some("Skills.Sh".to_string()),
        })
        .expect("assign tags");
        assert_eq!(assigned.len(), 2);
        assert!(
            assigned
                .iter()
                .all(|assignment| assignment.source_id.as_deref() == Some("skills_sh"))
        );

        crate::snapshot::assign_tags_to_skill(MarketplaceSkillTagAssignmentInput {
            skill_key: screenshot_key,
            tag_slugs: vec!["rust tools".to_string()],
            source_id: None,
        })
        .expect("assign second skill tag");

        let tags = crate::snapshot::list_tags().expect("list tags");
        let rust = tags
            .iter()
            .find(|tag| tag.slug == "rust-tools")
            .expect("rust tag exists");
        assert_eq!(rust.usage_count, 2);
        let ai = tags
            .iter()
            .find(|tag| tag.slug == "ai-helper")
            .expect("ai tag exists");
        assert_eq!(ai.usage_count, 1);

        let skill_tags =
            crate::snapshot::list_tags_for_skill(&search_key).expect("list skill tags");
        assert_eq!(skill_tags.len(), 2);
        assert_eq!(skill_tags[0].tag_slug, "ai-helper");
        assert_eq!(skill_tags[1].tag_slug, "rust-tools");
    });
}

#[test]
fn category_and_tag_normalization_reject_empty_values() {
    with_temp_data_root(|_| {
        create_connection().expect("create marketplace connection");

        assert!(
            upsert_category(MarketplaceCategoryUpsert {
                label: "   ".to_string(),
                slug: None,
                parent_id: None,
                position: 0,
            })
            .is_err()
        );
        assert!(
            upsert_category(MarketplaceCategoryUpsert {
                label: "Valid".to_string(),
                slug: Some("!!!".to_string()),
                parent_id: None,
                position: 0,
            })
            .is_err()
        );
        assert!(
            upsert_tag(MarketplaceTagUpsert {
                label: "   ".to_string(),
                slug: None,
            })
            .is_err()
        );
        assert!(
            crate::snapshot::assign_tags_to_skill(MarketplaceSkillTagAssignmentInput {
                skill_key: "openai/skills/search".to_string(),
                tag_slugs: vec!["!!!".to_string()],
                source_id: None,
            })
            .is_err()
        );
    });
}

#[test]
fn rating_summary_upsert_and_list_round_trip() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &now_rfc3339())
            .expect("upsert canonical skill");
        tx.commit().expect("commit canonical skill");

        let global = upsert_rating_summary(MarketplaceRatingSummaryUpsert {
            skill_key: "openai/skills/search".to_string(),
            source_id: None,
            rating_avg: 4.25,
            rating_count: 8,
            review_count: 5,
            last_review_at: Some("2026-04-01T00:00:00Z".to_string()),
        })
        .expect("upsert global rating summary");
        assert_eq!(global.skill_key, "openai/skills/search");
        assert_eq!(global.source_id, None);
        assert_eq!(global.rating_count, 8);

        let source_specific = upsert_rating_summary(MarketplaceRatingSummaryUpsert {
            skill_key: "openai/skills/search".to_string(),
            source_id: Some("Skills.Sh".to_string()),
            rating_avg: 4.5,
            rating_count: 2,
            review_count: 1,
            last_review_at: None,
        })
        .expect("upsert source rating summary");
        assert_eq!(source_specific.source_id.as_deref(), Some("skills_sh"));

        let summaries =
            list_rating_summaries_for_skill("openai/skills/search").expect("list rating summaries");
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].source_id, None);
        assert_eq!(summaries[1].source_id.as_deref(), Some("skills_sh"));
    });
}

#[test]
fn review_upsert_and_list_round_trip() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &now_rfc3339())
            .expect("upsert canonical skill");
        tx.commit().expect("commit canonical skill");

        let first = upsert_review(MarketplaceReviewUpsert {
            review_id: "review-1".to_string(),
            skill_key: "openai/skills/search".to_string(),
            source_id: Some("Skills.Sh".to_string()),
            author_hash: Some("hash-a".to_string()),
            rating: 5,
            title: Some("Great".to_string()),
            body: Some("Very useful".to_string()),
            locale: Some("en-US".to_string()),
            status: Some("published".to_string()),
            reviewed_at: Some("2026-04-02T00:00:00Z".to_string()),
        })
        .expect("upsert first review");
        assert_eq!(first.rating, 5);
        assert_eq!(first.source_id.as_deref(), Some("skills_sh"));

        let updated = upsert_review(MarketplaceReviewUpsert {
            review_id: "review-1".to_string(),
            skill_key: "openai/skills/search".to_string(),
            source_id: Some("Skills.Sh".to_string()),
            author_hash: Some("hash-b".to_string()),
            rating: 4,
            title: Some("Updated".to_string()),
            body: Some("Still useful".to_string()),
            locale: Some("en".to_string()),
            status: Some("published".to_string()),
            reviewed_at: Some("2026-04-03T00:00:00Z".to_string()),
        })
        .expect("update review");
        assert_eq!(updated.rating, 4);
        assert_eq!(updated.author_hash.as_deref(), Some("hash-b"));

        let reviews = list_reviews_for_skill("openai/skills/search").expect("list reviews");
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].review_id, "review-1");
        assert_eq!(reviews[0].title.as_deref(), Some("Updated"));
    });
}

#[test]
fn update_notification_upsert_list_and_dismiss_round_trip() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let skill_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &now_rfc3339())
                .expect("upsert canonical skill")
                .expect("skill key");
        tx.commit().expect("commit canonical skill");

        let notification = upsert_update_notification(MarketplaceUpdateNotificationUpsert {
            skill_key: skill_key.clone(),
            source_id: "Skills.Sh".to_string(),
            installed_version: Some("1.0.0".to_string()),
            available_version: Some("1.1.0".to_string()),
            installed_hash: Some("old".to_string()),
            available_hash: Some("new".to_string()),
            detected_at: Some("2026-04-01T00:00:00Z".to_string()),
            message: Some("Update available".to_string()),
            metadata_json: Some("{\"source\":\"test\"}".to_string()),
        })
        .expect("upsert notification");
        assert_eq!(notification.skill_key, skill_key);
        assert_eq!(notification.source_id, "skills_sh");
        assert_eq!(notification.dismissed_at, None);
        assert_eq!(notification.available_version.as_deref(), Some("1.1.0"));

        let active = list_update_notifications(false).expect("list active notifications");
        assert_eq!(active.len(), 1);
        let by_skill = list_update_notifications_for_skill(&skill_key, false)
            .expect("list skill notifications");
        assert_eq!(by_skill.len(), 1);

        assert!(dismiss_update_notification(&skill_key, "skills.sh").expect("dismiss"));
        assert!(
            list_update_notifications(false)
                .expect("list active after dismiss")
                .is_empty()
        );
        let dismissed = list_update_notifications(true).expect("list dismissed notifications");
        assert_eq!(dismissed.len(), 1);
        assert!(dismissed[0].dismissed_at.is_some());
    });
}

#[test]
fn update_notification_replacement_clears_dismissal_and_updates_payload() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let skill_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &now_rfc3339())
                .expect("upsert canonical skill")
                .expect("skill key");
        tx.commit().expect("commit canonical skill");

        upsert_update_notification(MarketplaceUpdateNotificationUpsert {
            skill_key: skill_key.clone(),
            source_id: "team.registry".to_string(),
            installed_version: None,
            available_version: Some("1.1.0".to_string()),
            installed_hash: None,
            available_hash: Some("old-hash".to_string()),
            detected_at: Some("2026-04-01T00:00:00Z".to_string()),
            message: Some("Old message".to_string()),
            metadata_json: None,
        })
        .expect("upsert first notification");
        assert!(dismiss_update_notification(&skill_key, "team_registry").expect("dismiss"));

        let updated = upsert_update_notification(MarketplaceUpdateNotificationUpsert {
            skill_key: skill_key.clone(),
            source_id: "Team.Registry".to_string(),
            installed_version: Some("1.1.0".to_string()),
            available_version: Some("1.2.0".to_string()),
            installed_hash: Some("old-hash".to_string()),
            available_hash: Some("new-hash".to_string()),
            detected_at: Some("2026-04-02T00:00:00Z".to_string()),
            message: Some("New message".to_string()),
            metadata_json: Some("{\"priority\":1}".to_string()),
        })
        .expect("replace notification");

        assert_eq!(updated.source_id, "team_registry");
        assert_eq!(updated.dismissed_at, None);
        assert_eq!(updated.available_version.as_deref(), Some("1.2.0"));
        assert_eq!(updated.available_hash.as_deref(), Some("new-hash"));
        assert_eq!(updated.message.as_deref(), Some("New message"));
        assert_eq!(updated.detected_at, "2026-04-02T00:00:00Z");
        assert_eq!(
            list_update_notifications(false)
                .expect("list active notifications")
                .len(),
            1
        );
    });
}

#[test]
fn update_notification_rejects_empty_identity_fields() {
    with_temp_data_root(|_| {
        create_connection().expect("create marketplace connection");

        assert!(
            upsert_update_notification(MarketplaceUpdateNotificationUpsert {
                skill_key: "   ".to_string(),
                source_id: "skills_sh".to_string(),
                installed_version: None,
                available_version: None,
                installed_hash: None,
                available_hash: None,
                detected_at: None,
                message: None,
                metadata_json: None,
            })
            .is_err()
        );

        assert!(
            upsert_update_notification(MarketplaceUpdateNotificationUpsert {
                skill_key: "openai/skills/search".to_string(),
                source_id: "   ".to_string(),
                installed_version: None,
                available_version: None,
                installed_hash: None,
                available_hash: None,
                detected_at: None,
                message: None,
                metadata_json: None,
            })
            .is_err()
        );
    });
}

#[test]
fn fts_rowid_mirrors_skill_rowid_and_refresh_is_idempotent() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();

        let skill_key = upsert_skill_identity_in_tx(&tx, "acme/skills", "widget", 5, &synced_at)
            .expect("upsert skill")
            .expect("skill key");

        // Refreshing repeatedly must not accumulate duplicate FTS rows.
        crate::snapshot::refresh_fts_entry_in_tx(&tx, &skill_key).expect("refresh fts again");
        crate::snapshot::refresh_fts_entry_in_tx(&tx, &skill_key).expect("refresh fts thrice");

        let base_rowid: i64 = tx
            .query_row(
                "SELECT rowid FROM marketplace_skill WHERE skill_key = ?1",
                [skill_key.as_str()],
                |row| row.get(0),
            )
            .expect("base rowid");
        let (fts_count, fts_rowid): (i64, i64) = tx
            .query_row(
                "SELECT COUNT(*), COALESCE(MIN(rowid), -1) FROM marketplace_skill_fts WHERE skill_key = ?1",
                [skill_key.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("fts stats");

        assert_eq!(fts_count, 1, "exactly one FTS row per skill");
        assert_eq!(
            fts_rowid, base_rowid,
            "FTS rowid mirrors marketplace_skill.rowid"
        );

        tx.commit().expect("commit");
    });
}

#[test]
fn migrate_v8_to_v9_realigns_misaligned_fts_rows() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let synced_at = now_rfc3339();

        // Seed a skill, then simulate the pre-v9 on-disk state: an FTS row
        // inserted with an auto-assigned rowid that does NOT match the base
        // skill rowid (how the old skill_key-keyed code left every row).
        let skill_key = {
            let tx = conn.unchecked_transaction().expect("tx");
            let key = upsert_skill_identity_in_tx(&tx, "acme/skills", "gadget", 9, &synced_at)
                .expect("upsert")
                .expect("key");
            tx.execute("DELETE FROM marketplace_skill_fts", [])
                .expect("clear fts");
            tx.execute(
                "INSERT INTO marketplace_skill_fts (rowid, skill_key, name, description, summary, publisher_name, repo_name)
                 VALUES (999999, ?1, 'gadget', '', '', 'acme', 'skills')",
                [key.as_str()],
            )
            .expect("insert misaligned fts row");
            tx.commit().expect("commit seed");
            key
        };

        let base_rowid: i64 = conn
            .query_row(
                "SELECT rowid FROM marketplace_skill WHERE skill_key = ?1",
                [skill_key.as_str()],
                |row| row.get(0),
            )
            .expect("base rowid");
        assert_ne!(base_rowid, 999_999, "precondition: FTS row is misaligned");

        crate::snapshot::migrate_v8_to_v9(&conn).expect("realign fts rowids");

        let (count, rowid): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(MIN(rowid), -1) FROM marketplace_skill_fts WHERE skill_key = ?1",
                [skill_key.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("fts stats after migration");
        assert_eq!(count, 1, "no duplicate FTS rows after realign");
        assert_eq!(rowid, base_rowid, "FTS rowid realigned to base rowid");
    });
}
