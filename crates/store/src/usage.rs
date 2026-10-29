//! 用量与成本汇总：从 `response.completed` 事件里的 `usage` 聚合。

use crate::Result;
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize)]
pub struct ModelUsage {
    pub model: String,
    pub runs: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    pub runs: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost: Option<f64>,
    pub pricing_configured: bool,
    pub models: Vec<ModelUsage>,
}

pub async fn summary(pool: &PgPool) -> Result<UsageSummary> {
    let rows: Vec<(String, i64, i64, i64)> = sqlx::query_as(
        "SELECT r.model, count(DISTINCT r.id), \
                COALESCE(sum(COALESCE((e.payload->'usage'->>'input_tokens')::bigint, 0)), 0)::bigint, \
                COALESCE(sum(COALESCE((e.payload->'usage'->>'output_tokens')::bigint, 0)), 0)::bigint \
         FROM run_events e JOIN runs r ON r.id = e.run_id \
         WHERE e.type = 'response.completed' \
         GROUP BY r.model ORDER BY r.model",
    )
    .fetch_all(pool)
    .await?;

    let input_rate = number_setting(pool, "llm.price_input_per_million").await?;
    let output_rate = number_setting(pool, "llm.price_output_per_million").await?;
    let pricing_configured = input_rate.is_some() || output_rate.is_some();
    let mut models = Vec::with_capacity(rows.len());
    let mut totals = (0i64, 0i64, 0i64, 0i64, Some(0.0));
    for (model, runs, input, output) in rows {
        let cost = match (input_rate, output_rate) {
            (None, None) => None,
            (i, o) => Some(
                input as f64 / 1_000_000.0 * i.unwrap_or(0.0)
                    + output as f64 / 1_000_000.0 * o.unwrap_or(0.0),
            ),
        };
        totals.0 += runs;
        totals.1 += input;
        totals.2 += output;
        totals.3 += input + output;
        totals.4 = match (totals.4, cost) {
            (Some(a), Some(b)) => Some(a + b),
            _ => None,
        };
        models.push(ModelUsage {
            model,
            runs,
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
            estimated_cost: cost,
        });
    }

    Ok(UsageSummary {
        runs: totals.0,
        input_tokens: totals.1,
        output_tokens: totals.2,
        total_tokens: totals.3,
        estimated_cost: if pricing_configured { totals.4 } else { None },
        pricing_configured,
        models,
    })
}

async fn number_setting(pool: &PgPool, key: &str) -> Result<Option<f64>> {
    let value = crate::secrets::get_setting(pool, key).await?;
    Ok(value.and_then(|v| v.as_f64()))
}
