//! Filters for queries over discoverable_letterings.
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

/// Append filters to a query whose `l` relation is discoverable_letterings.
pub fn apply_public_filters<'a>(
    sql: &mut QueryBuilder<'a, Postgres>,
    city_id: Option<Uuid>,
    script: Option<&'a str>,
    style: Option<&'a str>,
) {
    sql.push(" WHERE true");
    if let Some(id) = city_id {
        sql.push(" AND l.city_id=").push_bind(id);
    }
    for (column, value) in [(" AND l.ml_script=", script), (" AND l.ml_style=", style)] {
        if let Some(value) = normalized(value) {
            sql.push(column).push_bind(value);
        }
    }
}

/// Treat blank filters as missing values.
pub fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
