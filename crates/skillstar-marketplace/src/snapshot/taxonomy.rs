use super::*;

pub fn upsert_category(input: MarketplaceCategoryUpsert) -> Result<MarketplaceCategory> {
    with_conn(|conn| {
        let label = normalize_required_label(&input.label, "category")?;
        let slug_source = input.slug.as_deref().unwrap_or(&label);
        let slug = normalize_marketplace_slug(slug_source, "category")?;
        let id = slug.clone();
        let parent_id = input
            .parent_id
            .as_deref()
            .map(|value| normalize_marketplace_slug(value, "category parent"))
            .transpose()?;
        if parent_id.as_deref() == Some(id.as_str()) {
            return Err(anyhow!("Marketplace category cannot be its own parent"));
        }

        let now = now_rfc3339();
        conn.execute(
            "INSERT INTO marketplace_category (
                id,
                label,
                slug,
                parent_id,
                position,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            ON CONFLICT(id) DO UPDATE SET
                label = excluded.label,
                slug = excluded.slug,
                parent_id = excluded.parent_id,
                position = excluded.position,
                updated_at = excluded.updated_at",
            params![id, label, slug, parent_id, input.position, now],
        )
        .context("Failed to upsert marketplace category")?;

        conn.query_row(
            "SELECT id, label, slug, parent_id, position, created_at, updated_at
             FROM marketplace_category
             WHERE id = ?1",
            [id],
            row_to_category,
        )
        .context("Failed to load upserted marketplace category")
    })
}

pub fn list_categories() -> Result<Vec<MarketplaceCategory>> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, label, slug, parent_id, position, created_at, updated_at
                 FROM marketplace_category
                 ORDER BY COALESCE(parent_id, ''), position ASC, label ASC",
            )
            .context("Failed to prepare marketplace category list query")?;
        let rows = stmt
            .query_map([], row_to_category)
            .context("Failed to read marketplace categories")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to decode marketplace categories")
    })
}

pub fn assign_categories_to_skill(
    input: MarketplaceSkillCategoryAssignmentInput,
) -> Result<Vec<MarketplaceSkillCategoryAssignment>> {
    with_conn(|conn| {
        let skill_key = normalize_skill_key_value(&input.skill_key)?;
        let mut category_ids = Vec::new();
        for category_id in input.category_ids {
            let normalized = normalize_marketplace_slug(&category_id, "category")?;
            if !category_ids.contains(&normalized) {
                category_ids.push(normalized);
            }
        }

        let tx = conn
            .unchecked_transaction()
            .context("Failed to start marketplace category assignment transaction")?;
        tx.execute(
            "DELETE FROM marketplace_skill_category WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .context("Failed to clear marketplace category assignments")?;

        let now = now_rfc3339();
        for category_id in &category_ids {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM marketplace_category WHERE id = ?1 LIMIT 1",
                    [category_id.as_str()],
                    |_| Ok(()),
                )
                .optional()
                .context("Failed to validate marketplace category assignment")?
                .is_some();
            if !exists {
                return Err(anyhow!(
                    "Marketplace category does not exist: {category_id}"
                ));
            }
            tx.execute(
                "INSERT INTO marketplace_skill_category (skill_key, category_id, assigned_at)
                 VALUES (?1, ?2, ?3)",
                params![skill_key, category_id, now],
            )
            .context("Failed to assign marketplace category to skill")?;
        }
        tx.commit()
            .context("Failed to commit marketplace category assignments")?;

        list_categories_for_skill(&skill_key)
    })
}

pub fn list_categories_for_skill(
    skill_key: &str,
) -> Result<Vec<MarketplaceSkillCategoryAssignment>> {
    let skill_key = match normalize_skill_key_value(skill_key) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT sc.skill_key, sc.category_id, sc.assigned_at
                 FROM marketplace_skill_category sc
                 JOIN marketplace_category c ON c.id = sc.category_id
                 WHERE sc.skill_key = ?1
                 ORDER BY c.position ASC, c.label ASC",
            )
            .context("Failed to prepare marketplace skill-category list query")?;
        let rows = stmt
            .query_map([skill_key], row_to_skill_category_assignment)
            .context("Failed to read marketplace skill-category rows")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to decode marketplace skill-category rows")
    })
}

pub fn upsert_tag(input: MarketplaceTagUpsert) -> Result<MarketplaceTag> {
    with_conn(|conn| {
        let label = normalize_required_label(&input.label, "tag")?;
        let slug_source = input.slug.as_deref().unwrap_or(&label);
        let slug = normalize_marketplace_slug(slug_source, "tag")?;
        let now = now_rfc3339();
        conn.execute(
            "INSERT INTO marketplace_tag (slug, label, usage_count, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?3)
             ON CONFLICT(slug) DO UPDATE SET
                label = excluded.label,
                updated_at = excluded.updated_at",
            params![slug, label, now],
        )
        .context("Failed to upsert marketplace tag")?;
        refresh_tag_usage_count(conn, &slug)?;

        conn.query_row(
            "SELECT slug, label, usage_count, created_at, updated_at
             FROM marketplace_tag
             WHERE slug = ?1",
            [slug],
            row_to_tag,
        )
        .context("Failed to load upserted marketplace tag")
    })
}

pub fn list_tags() -> Result<Vec<MarketplaceTag>> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT slug, label, usage_count, created_at, updated_at
                 FROM marketplace_tag
                 ORDER BY usage_count DESC, label ASC",
            )
            .context("Failed to prepare marketplace tag list query")?;
        let rows = stmt
            .query_map([], row_to_tag)
            .context("Failed to read marketplace tags")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to decode marketplace tags")
    })
}

pub fn assign_tags_to_skill(
    input: MarketplaceSkillTagAssignmentInput,
) -> Result<Vec<MarketplaceSkillTagAssignment>> {
    with_conn(|conn| {
        let skill_key = normalize_skill_key_value(&input.skill_key)?;
        let source_id = normalize_optional_source_id(input.source_id);
        let mut tag_slugs = Vec::new();
        for tag_slug in input.tag_slugs {
            let normalized = normalize_marketplace_slug(&tag_slug, "tag")?;
            if !tag_slugs.contains(&normalized) {
                tag_slugs.push(normalized);
            }
        }

        let tx = conn
            .unchecked_transaction()
            .context("Failed to start marketplace tag assignment transaction")?;
        tx.execute(
            "DELETE FROM marketplace_skill_tag WHERE skill_key = ?1 AND source_id = ?2",
            params![skill_key, source_id],
        )
        .context("Failed to clear marketplace tag assignments")?;

        let now = now_rfc3339();
        for tag_slug in &tag_slugs {
            tx.execute(
                "INSERT INTO marketplace_tag (slug, label, usage_count, created_at, updated_at)
                 VALUES (?1, ?2, 0, ?3, ?3)
                 ON CONFLICT(slug) DO NOTHING",
                params![tag_slug, tag_slug, now],
            )
            .context("Failed to ensure marketplace tag exists")?;
            tx.execute(
                "INSERT INTO marketplace_skill_tag (skill_key, tag_slug, source_id, assigned_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![skill_key, tag_slug, source_id, now],
            )
            .context("Failed to assign marketplace tag to skill")?;
        }
        tx.commit()
            .context("Failed to commit marketplace tag assignments")?;

        refresh_all_tag_usage_counts(conn)?;
        list_tags_for_skill(&skill_key)
    })
}

pub fn list_tags_for_skill(skill_key: &str) -> Result<Vec<MarketplaceSkillTagAssignment>> {
    let skill_key = match normalize_skill_key_value(skill_key) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT st.skill_key, st.tag_slug, st.source_id, st.assigned_at
                 FROM marketplace_skill_tag st
                 JOIN marketplace_tag t ON t.slug = st.tag_slug
                 WHERE st.skill_key = ?1
                 ORDER BY st.tag_slug ASC, st.source_id ASC",
            )
            .context("Failed to prepare marketplace skill-tag list query")?;
        let rows = stmt
            .query_map([skill_key], row_to_skill_tag_assignment)
            .context("Failed to read marketplace skill-tag rows")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to decode marketplace skill-tag rows")
    })
}

pub(crate) fn refresh_tag_usage_count(conn: &Connection, slug: &str) -> Result<()> {
    conn.execute(
        "UPDATE marketplace_tag
         SET usage_count = (
             SELECT COUNT(DISTINCT skill_key)
             FROM marketplace_skill_tag
             WHERE tag_slug = ?1
         )
         WHERE slug = ?1",
        [slug],
    )
    .context("Failed to refresh marketplace tag usage count")?;
    Ok(())
}

pub(crate) fn refresh_all_tag_usage_counts(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE marketplace_tag
         SET usage_count = (
             SELECT COUNT(DISTINCT skill_key)
             FROM marketplace_skill_tag
             WHERE tag_slug = marketplace_tag.slug
         )",
        [],
    )
    .context("Failed to refresh marketplace tag usage counts")?;
    Ok(())
}

pub fn upsert_rating_summary(
    summary: MarketplaceRatingSummaryUpsert,
) -> Result<MarketplaceRatingSummary> {
    with_conn(|conn| {
        let skill_key = normalize_skill_key_value(&summary.skill_key)?;
        let source_id = normalize_optional_source_id(summary.source_id);
        validate_rating_summary_values(
            summary.rating_avg,
            summary.rating_count,
            summary.review_count,
        )?;

        let now = now_rfc3339();
        conn.execute(
            "INSERT INTO marketplace_rating_summary (
                skill_key,
                source_id,
                rating_avg,
                rating_count,
                review_count,
                last_review_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(skill_key, source_id) DO UPDATE SET
                rating_avg = excluded.rating_avg,
                rating_count = excluded.rating_count,
                review_count = excluded.review_count,
                last_review_at = excluded.last_review_at,
                updated_at = excluded.updated_at",
            params![
                skill_key,
                source_id,
                summary.rating_avg,
                summary.rating_count,
                summary.review_count,
                summary.last_review_at,
                now
            ],
        )
        .context("Failed to upsert marketplace rating summary")?;

        conn.query_row(
            "SELECT
                skill_key,
                source_id,
                rating_avg,
                rating_count,
                review_count,
                last_review_at,
                updated_at
             FROM marketplace_rating_summary
             WHERE skill_key = ?1 AND source_id = ?2",
            params![skill_key, source_id],
            row_to_rating_summary,
        )
        .context("Failed to load upserted marketplace rating summary")
    })
}

pub fn list_rating_summaries_for_skill(skill_key: &str) -> Result<Vec<MarketplaceRatingSummary>> {
    let skill_key = skill_key.trim().to_ascii_lowercase();
    if skill_key.is_empty() {
        return Ok(Vec::new());
    }

    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT
                    skill_key,
                    source_id,
                    rating_avg,
                    rating_count,
                    review_count,
                    last_review_at,
                    updated_at
                 FROM marketplace_rating_summary
                 WHERE skill_key = ?1
                 ORDER BY CASE WHEN source_id = '' THEN 0 ELSE 1 END ASC, source_id ASC",
            )
            .context("Failed to prepare marketplace rating-summary query")?;
        let rows = stmt
            .query_map([skill_key], row_to_rating_summary)
            .context("Failed to read marketplace rating-summary rows")?;

        let mut summaries = Vec::new();
        for row in rows {
            summaries.push(row.context("Failed to decode marketplace rating summary")?);
        }
        Ok(summaries)
    })
}

pub fn upsert_review(review: MarketplaceReviewUpsert) -> Result<MarketplaceReview> {
    with_conn(|conn| {
        let review_id = review.review_id.trim().to_string();
        if review_id.is_empty() {
            return Err(anyhow!("Marketplace review_id cannot be empty"));
        }
        let skill_key = normalize_skill_key_value(&review.skill_key)?;
        let source_id = normalize_optional_source_id(review.source_id);
        validate_rating_value(review.rating)?;

        let now = now_rfc3339();
        let status = trim_optional(review.status).unwrap_or_else(|| "published".to_string());
        conn.execute(
            "INSERT INTO marketplace_review (
                review_id,
                skill_key,
                source_id,
                author_hash,
                rating,
                title,
                body,
                locale,
                status,
                reviewed_at,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
            ON CONFLICT(review_id) DO UPDATE SET
                skill_key = excluded.skill_key,
                source_id = excluded.source_id,
                author_hash = excluded.author_hash,
                rating = excluded.rating,
                title = excluded.title,
                body = excluded.body,
                locale = excluded.locale,
                status = excluded.status,
                reviewed_at = excluded.reviewed_at,
                updated_at = excluded.updated_at",
            params![
                review_id,
                skill_key,
                source_id,
                trim_optional(review.author_hash),
                review.rating,
                trim_optional(review.title),
                trim_optional(review.body),
                trim_optional(review.locale),
                status,
                review.reviewed_at,
                now
            ],
        )
        .context("Failed to upsert marketplace review")?;

        conn.query_row(
            "SELECT
                review_id,
                skill_key,
                source_id,
                author_hash,
                rating,
                title,
                body,
                locale,
                status,
                reviewed_at,
                created_at,
                updated_at
             FROM marketplace_review
             WHERE review_id = ?1",
            [review_id],
            row_to_review,
        )
        .context("Failed to load upserted marketplace review")
    })
}

pub fn list_reviews_for_skill(skill_key: &str) -> Result<Vec<MarketplaceReview>> {
    let skill_key = skill_key.trim().to_ascii_lowercase();
    if skill_key.is_empty() {
        return Ok(Vec::new());
    }

    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT
                    review_id,
                    skill_key,
                    source_id,
                    author_hash,
                    rating,
                    title,
                    body,
                    locale,
                    status,
                    reviewed_at,
                    created_at,
                    updated_at
                 FROM marketplace_review
                 WHERE skill_key = ?1
                 ORDER BY COALESCE(reviewed_at, updated_at) DESC, review_id ASC",
            )
            .context("Failed to prepare marketplace review query")?;
        let rows = stmt
            .query_map([skill_key], row_to_review)
            .context("Failed to read marketplace review rows")?;

        let mut reviews = Vec::new();
        for row in rows {
            reviews.push(row.context("Failed to decode marketplace review")?);
        }
        Ok(reviews)
    })
}

pub fn upsert_update_notification(
    notification: MarketplaceUpdateNotificationUpsert,
) -> Result<MarketplaceUpdateNotification> {
    with_conn(|conn| {
        let skill_key = normalize_skill_key_value(&notification.skill_key)?;
        let source_id = normalize_observation_source_id(&notification.source_id)?;
        let now = now_rfc3339();
        let detected_at = trim_optional(notification.detected_at).unwrap_or_else(|| now.clone());

        conn.execute(
            "INSERT INTO marketplace_update_notification (
                skill_key,
                source_id,
                installed_version,
                available_version,
                installed_hash,
                available_hash,
                detected_at,
                dismissed_at,
                message,
                metadata_json,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10)
            ON CONFLICT(skill_key, source_id) DO UPDATE SET
                installed_version = excluded.installed_version,
                available_version = excluded.available_version,
                installed_hash = excluded.installed_hash,
                available_hash = excluded.available_hash,
                detected_at = excluded.detected_at,
                dismissed_at = NULL,
                message = excluded.message,
                metadata_json = excluded.metadata_json,
                updated_at = excluded.updated_at",
            params![
                skill_key,
                source_id,
                trim_optional(notification.installed_version),
                trim_optional(notification.available_version),
                trim_optional(notification.installed_hash),
                trim_optional(notification.available_hash),
                detected_at,
                trim_optional(notification.message),
                trim_optional(notification.metadata_json),
                now
            ],
        )
        .context("Failed to upsert marketplace update notification")?;

        load_update_notification(conn, &skill_key, &source_id)?
            .ok_or_else(|| anyhow!("Marketplace update notification was not persisted"))
    })
}

pub fn list_update_notifications(
    include_dismissed: bool,
) -> Result<Vec<MarketplaceUpdateNotification>> {
    with_conn(|conn| {
        let where_clause = if include_dismissed {
            ""
        } else {
            "WHERE dismissed_at IS NULL"
        };
        let sql = format!(
            "SELECT
                skill_key,
                source_id,
                installed_version,
                available_version,
                installed_hash,
                available_hash,
                detected_at,
                dismissed_at,
                message,
                metadata_json,
                updated_at
             FROM marketplace_update_notification
             {where_clause}
             ORDER BY detected_at DESC, skill_key ASC, source_id ASC"
        );
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare marketplace update notification list query")?;
        let rows = stmt
            .query_map([], row_to_update_notification)
            .context("Failed to read marketplace update notification rows")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to decode marketplace update notification rows")
    })
}

pub fn list_update_notifications_for_skill(
    skill_key: &str,
    include_dismissed: bool,
) -> Result<Vec<MarketplaceUpdateNotification>> {
    let skill_key = match normalize_skill_key_value(skill_key) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };
    with_conn(|conn| {
        let dismissed_filter = if include_dismissed {
            ""
        } else {
            "AND dismissed_at IS NULL"
        };
        let sql = format!(
            "SELECT
                skill_key,
                source_id,
                installed_version,
                available_version,
                installed_hash,
                available_hash,
                detected_at,
                dismissed_at,
                message,
                metadata_json,
                updated_at
             FROM marketplace_update_notification
             WHERE skill_key = ?1 {dismissed_filter}
             ORDER BY detected_at DESC, source_id ASC"
        );
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare marketplace update notification skill query")?;
        let rows = stmt
            .query_map([skill_key], row_to_update_notification)
            .context("Failed to read marketplace update notification skill rows")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to decode marketplace update notification skill rows")
    })
}

pub fn dismiss_update_notification(skill_key: &str, source_id: &str) -> Result<bool> {
    with_conn(|conn| {
        let skill_key = normalize_skill_key_value(skill_key)?;
        let source_id = normalize_observation_source_id(source_id)?;
        let now = now_rfc3339();
        let updated = conn
            .execute(
                "UPDATE marketplace_update_notification
                 SET dismissed_at = ?3,
                     updated_at = ?3
                 WHERE skill_key = ?1 AND source_id = ?2",
                params![skill_key, source_id, now],
            )
            .context("Failed to dismiss marketplace update notification")?;
        Ok(updated > 0)
    })
}

pub(crate) fn load_update_notification(
    conn: &Connection,
    skill_key: &str,
    source_id: &str,
) -> Result<Option<MarketplaceUpdateNotification>> {
    conn.query_row(
        "SELECT
            skill_key,
            source_id,
            installed_version,
            available_version,
            installed_hash,
            available_hash,
            detected_at,
            dismissed_at,
            message,
            metadata_json,
            updated_at
         FROM marketplace_update_notification
         WHERE skill_key = ?1 AND source_id = ?2",
        params![skill_key, source_id],
        row_to_update_notification,
    )
    .optional()
    .context("Failed to load marketplace update notification")
}
