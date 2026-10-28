//! Shared visibility and filters for public upload queries.
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

/// Share gallery visibility and filter semantics.
pub fn apply_public_filters<'a>(
    sql: &mut QueryBuilder<'a, Postgres>,
    city_id: Option<Uuid>,
    script: Option<&'a str>,
    style: Option<&'a str>,
) {
    sql.push(" WHERE l.status='APPROVED' AND COALESCE(rp.discoverability_enabled,true)");
    if let Some(id) = city_id {
        sql.push(" AND l.city_id=").push_bind(id);
    }
    for (column, value) in [(" AND l.ml_script=", script), (" AND l.ml_style=", style)] {
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            sql.push(column).push_bind(value);
        }
    }
}
