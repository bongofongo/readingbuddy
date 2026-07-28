//! The user's rating scale, and what its values mean on Goodreads.
//!
//! Numeric scales only (min, max, step), and the Goodreads mapping is an
//! explicit lookup the user edits — never a formula, because formulas are always
//! wrong at the ends, and never a silent round, because Goodreads' `My Rating`
//! is an integer 0–5 with no halves.

use anyhow::{Result, bail};
use readingbuddy::{Engine, RatingScale};

pub async fn scale(engine: &Engine, name: &str, min: f64, max: f64, step: f64) -> Result<()> {
    let scale = engine
        .storage
        .put_rating_scale(name, min, max, step)
        .await?;
    println!(
        "scale '{}': {} .. {} step {}",
        scale.name, scale.min, scale.max, scale.step
    );
    println!(
        "  {} points, {}",
        scale.points().len(),
        mapped_summary(engine, &scale).await?
    );
    Ok(())
}

pub async fn map(engine: &Engine, name: Option<&str>, value: f64, goodreads: u8) -> Result<()> {
    let scale = resolve_scale(engine, name).await?;
    engine.storage.map_rating(&scale, value, goodreads).await?;
    println!("'{}' {value} → goodreads {goodreads}", scale.name);
    Ok(())
}

pub async fn show(engine: &Engine, name: Option<&str>) -> Result<()> {
    let scales = engine.storage.list_rating_scales().await?;
    if scales.is_empty() {
        println!("no rating scales — `readingbuddy rating scale --min 0 --max 5 --step 0.5`");
        return Ok(());
    }
    let active = engine.storage.active_rating_scale().await?;
    for s in scales {
        if name.is_some_and(|n| n != s.name) {
            continue;
        }
        let marker = if active.as_ref().is_some_and(|a| a.id == s.id) {
            "  (new ratings use this one)"
        } else {
            ""
        };
        println!("{}: {} .. {} step {}{marker}", s.name, s.min, s.max, s.step);
        let map: Vec<(f64, u8)> = engine.storage.rating_map(s.id).await?;
        for p in s.points() {
            match map.iter().find(|(v, _)| *v == p) {
                Some((_, g)) => println!("  {p:>6}  → goodreads {g}"),
                // An unmapped point is a fact worth showing: it is what a review
                // on that value will report instead of a number.
                None => println!("  {p:>6}  → (unmapped)"),
            }
        }
    }
    Ok(())
}

async fn resolve_scale(engine: &Engine, name: Option<&str>) -> Result<RatingScale> {
    match name {
        Some(n) => match engine.storage.rating_scale_by_name(n).await? {
            Some(s) => Ok(s),
            None => bail!("no rating scale called '{n}' — `readingbuddy rating show` lists them"),
        },
        None => match engine.storage.active_rating_scale().await? {
            Some(s) => Ok(s),
            None => {
                bail!("no rating scale — `readingbuddy rating scale --min 0 --max 5 --step 0.5`")
            }
        },
    }
}

async fn mapped_summary(engine: &Engine, scale: &RatingScale) -> Result<String> {
    let mapped = engine.storage.rating_map(scale.id).await?.len();
    let total = scale.points().len();
    Ok(match total.saturating_sub(mapped) {
        0 => "every point maps to goodreads".to_string(),
        n => format!("{n} of them with no goodreads mapping (`rating map <value> <0-5>`)"),
    })
}
