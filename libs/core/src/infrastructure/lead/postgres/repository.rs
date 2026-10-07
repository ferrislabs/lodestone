use std::collections::HashMap;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use sqlx::{Postgres, QueryBuilder};

use crate::{
    Lead, LeadId, LeadStatus, NormalizedDomain, ProfileId, ScoreBreakdown, Verdict,
    domain::lead::{
        ports::LeadRepository,
        query::{LeadPage, LeadQuery, LeadSort, ProfileSummary, VerdictFilter},
    },
    infrastructure::{
        lead::postgres::model::{GroupedLeadRow, LeadRow, ProfileSummaryRow},
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

#[repository(domain = Lead, backend = Postgres)]
pub struct PgLeadRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgLeadRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

pub(crate) const NOT_SUPPRESSED: &str = " AND NOT EXISTS (SELECT 1 FROM suppression_list sup WHERE sup.kind = 'domain' AND (sup.value = leads.domain OR right(leads.domain, length(sup.value) + 1) = '.' || sup.value))";

fn escape_like(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for c in raw.chars() {
        if matches!(c, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

pub(crate) fn push_verdicts<'a>(
    builder: &mut QueryBuilder<'a, Postgres>,
    verdicts: &[VerdictFilter],
) {
    if verdicts.is_empty() {
        return;
    }
    let stored: Vec<&'static str> = verdicts
        .iter()
        .filter_map(|filter| filter.stored().map(Verdict::as_str))
        .collect();
    builder
        .push(" AND (leads.verdict = ANY(")
        .push_bind(stored)
        .push(")");
    if verdicts.contains(&VerdictFilter::Pending) {
        builder.push(" OR leads.verdict IS NULL");
    }
    builder.push(")");
}

fn push_filters<'a>(
    builder: &mut QueryBuilder<'a, Postgres>,
    profile_id: ProfileId,
    query: &'a LeadQuery,
) {
    builder
        .push(" WHERE leads.profile_id = ")
        .push_bind(profile_id.0);
    if let Some(status) = query.status {
        builder
            .push(" AND leads.status = ")
            .push_bind(status.as_str());
    }
    push_verdicts(builder, &query.verdicts);
    if let Some(min_confidence) = query.min_confidence {
        builder
            .push(" AND leads.confidence >= ")
            .push_bind(min_confidence);
    }
    if let Some(min_score) = query.min_score {
        builder.push(" AND leads.score >= ").push_bind(min_score);
    }
    if let Some(needle) = query.domain_contains.as_deref() {
        let pattern = format!("%{}%", escape_like(&needle.to_ascii_lowercase()));
        builder
            .push(" AND leads.domain LIKE ")
            .push_bind(pattern)
            .push(" ESCAPE '\\'");
    }
    if let Some(kind) = &query.signal_kind {
        builder
            .push(" AND EXISTS (SELECT 1 FROM signals WHERE signals.lead_id = leads.id AND signals.kind = ")
            .push_bind(kind.as_str())
            .push(")");
    }
    builder.push(NOT_SUPPRESSED);
}

fn order_by(sort: LeadSort) -> &'static str {
    match sort {
        LeadSort::ConfidenceDesc => {
            " ORDER BY leads.confidence DESC NULLS LAST, leads.score DESC, leads.id ASC"
        }
        LeadSort::ConfidenceAsc => {
            " ORDER BY leads.confidence ASC NULLS LAST, leads.score DESC, leads.id ASC"
        }
        LeadSort::ScoreDesc => " ORDER BY leads.score DESC, leads.id ASC",
        LeadSort::ScoreAsc => " ORDER BY leads.score ASC, leads.id ASC",
        LeadSort::LlmScoreDesc => {
            " ORDER BY leads.llm_score DESC NULLS LAST, leads.confidence DESC NULLS LAST, leads.id ASC"
        }
        LeadSort::LlmScoreAsc => {
            " ORDER BY leads.llm_score ASC NULLS LAST, leads.confidence DESC NULLS LAST, leads.id ASC"
        }
        LeadSort::LastSeenDesc => " ORDER BY leads.last_seen_at DESC, leads.id ASC",
        LeadSort::FirstSeenDesc => " ORDER BY leads.first_seen_at DESC, leads.id ASC",
        LeadSort::DomainAsc => " ORDER BY leads.domain ASC, leads.id ASC",
    }
}

const LEAD_COLUMNS: &str = "leads.id, leads.profile_id, leads.domain, leads.status, leads.score, leads.score_breakdown, leads.confidence, leads.verdict, leads.llm_score, leads.qualified_at, leads.first_seen_at, leads.last_seen_at, leads.updated_at";

const COMPANY_KEY: &str = "COALESCE(d.company_siren, leads.id::text)";

impl<'tx> PgLeadRepository<'tx> {
    async fn list_grouped_page(
        &mut self,
        profile_id: ProfileId,
        query: &LeadQuery,
    ) -> Result<LeadPage, CoreError> {
        let mut count = QueryBuilder::<Postgres>::new(format!(
            "SELECT COUNT(DISTINCT {COMPANY_KEY}) FROM leads LEFT JOIN company_dossiers d ON d.domain = leads.domain"
        ));
        push_filters(&mut count, profile_id, query);

        let mut select = QueryBuilder::<Postgres>::new(format!(
            "WITH ranked AS (SELECT {LEAD_COLUMNS}, d.company_siren AS siren, ROW_NUMBER() OVER (PARTITION BY {COMPANY_KEY} ORDER BY leads.confidence DESC NULLS LAST, leads.score DESC, leads.id ASC) AS group_rank FROM leads LEFT JOIN company_dossiers d ON d.domain = leads.domain"
        ));
        push_filters(&mut select, profile_id, query);
        select.push(") SELECT leads.id, leads.profile_id, leads.domain, leads.status, leads.score, leads.score_breakdown, leads.confidence, leads.verdict, leads.llm_score, leads.qualified_at, leads.first_seen_at, leads.last_seen_at, leads.updated_at, leads.siren FROM ranked AS leads WHERE leads.group_rank = 1");
        select.push(order_by(query.sort));
        select.push(" LIMIT ").push_bind(i64::from(query.limit));
        select
            .push(" OFFSET ")
            .push_bind(i64::try_from(query.offset).unwrap_or(i64::MAX));

        let mut tx = self.tx.lock().await;
        let total: i64 = count
            .build_query_scalar()
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        let rows = select
            .build_query_as::<GroupedLeadRow>()
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;

        let mut representatives: HashMap<String, LeadId> = HashMap::new();
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let lead = Lead::try_from(row.lead)?;
            if let Some(siren) = row.siren {
                representatives.insert(siren, lead.id);
            }
            items.push(lead);
        }
        let mut members: HashMap<LeadId, Vec<Lead>> = HashMap::new();
        if !representatives.is_empty() {
            let sirens: Vec<String> = representatives.keys().cloned().collect();
            let mut siblings = QueryBuilder::<Postgres>::new(format!(
                "SELECT {LEAD_COLUMNS}, d.company_siren AS siren FROM leads JOIN company_dossiers d ON d.domain = leads.domain"
            ));
            push_filters(&mut siblings, profile_id, query);
            siblings
                .push(" AND d.company_siren = ANY(")
                .push_bind(sirens)
                .push(
                    ") ORDER BY leads.confidence DESC NULLS LAST, leads.score DESC, leads.id ASC",
                );
            let rows = siblings
                .build_query_as::<GroupedLeadRow>()
                .fetch_all(&mut ***tx)
                .await
                .map_err(map_sqlx_error)?;
            for row in rows {
                let lead = Lead::try_from(row.lead)?;
                let representative = row
                    .siren
                    .as_deref()
                    .and_then(|siren| representatives.get(siren));
                if let Some(representative) = representative
                    && *representative != lead.id
                {
                    members.entry(*representative).or_default().push(lead);
                }
            }
        }
        Ok(LeadPage {
            items,
            total: u64::try_from(total).unwrap_or_default(),
            members,
        })
    }
}

impl<'tx> LeadRepository for PgLeadRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "upsert", db.table = "leads", profile.id = %profile_id, lead.domain = %domain), err)]
    async fn record_seen(
        &mut self,
        profile_id: ProfileId,
        domain: &NormalizedDomain,
        seen_at: DateTime<Utc>,
    ) -> Result<Lead, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LeadRow,
            r#"
            INSERT INTO leads (id, profile_id, domain, status, score, first_seen_at, last_seen_at, updated_at)
            VALUES ($1, $2, $3, 'new', 0, $4, $4, $4)
            ON CONFLICT (profile_id, domain) DO UPDATE SET
                last_seen_at  = GREATEST(leads.last_seen_at, EXCLUDED.last_seen_at),
                first_seen_at = LEAST(leads.first_seen_at, EXCLUDED.first_seen_at),
                updated_at    = EXCLUDED.updated_at
            RETURNING id, profile_id, domain, status, score, score_breakdown, confidence, verdict, llm_score, qualified_at, first_seen_at, last_seen_at, updated_at
            "#,
            common::generate_uuid_v7(),
            profile_id.0,
            domain.as_str(),
            seen_at,
        )
        .fetch_one(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Lead::try_from(row)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", lead.id = %id), err)]
    async fn find(&mut self, id: LeadId) -> Result<Option<Lead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LeadRow,
            r#"
            SELECT id, profile_id, domain, status, score, score_breakdown, confidence, verdict, llm_score, qualified_at, first_seen_at, last_seen_at, updated_at
            FROM leads
            WHERE id = $1
            "#,
            id.0,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Lead::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id, lead.domain = %domain), err)]
    async fn find_by_domain(
        &mut self,
        profile_id: ProfileId,
        domain: &NormalizedDomain,
    ) -> Result<Option<Lead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LeadRow,
            r#"
            SELECT id, profile_id, domain, status, score, score_breakdown, confidence, verdict, llm_score, qualified_at, first_seen_at, last_seen_at, updated_at
            FROM leads
            WHERE profile_id = $1 AND domain = $2
            "#,
            profile_id.0,
            domain.as_str(),
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Lead::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads"), err)]
    async fn list_by_domain(&mut self, domain: &NormalizedDomain) -> Result<Vec<Lead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            LeadRow,
            r#"
            SELECT id, profile_id, domain, status, score, score_breakdown, confidence, verdict, llm_score, qualified_at, first_seen_at, last_seen_at, updated_at
            FROM leads
            WHERE domain = $1
            ORDER BY profile_id
            "#,
            domain.as_str(),
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(Lead::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "leads", lead.id = %id), err)]
    async fn update_status(
        &mut self,
        id: LeadId,
        from: LeadStatus,
        to: LeadStatus,
        at: DateTime<Utc>,
    ) -> Result<Option<Lead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LeadRow,
            r#"
            UPDATE leads
            SET status = $3, updated_at = $4
            WHERE id = $1 AND status = $2
            RETURNING id, profile_id, domain, status, score, score_breakdown, confidence, verdict, llm_score, qualified_at, first_seen_at, last_seen_at, updated_at
            "#,
            id.0,
            from.as_str(),
            to.as_str(),
            at,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Lead::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "leads", lead.id = %id), err)]
    async fn update_score(
        &mut self,
        id: LeadId,
        breakdown: &ScoreBreakdown,
        at: DateTime<Utc>,
    ) -> Result<Option<Lead>, CoreError> {
        let contributions = serde_json::to_value(&breakdown.contributions)
            .map_err(|_| CoreError::Internal("score breakdown is not serializable".to_owned()))?;
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LeadRow,
            r#"
            UPDATE leads
            SET score = $2, score_breakdown = $4, updated_at = $3
            WHERE id = $1
            RETURNING id, profile_id, domain, status, score, score_breakdown, confidence, verdict, llm_score, qualified_at, first_seen_at, last_seen_at, updated_at
            "#,
            id.0,
            breakdown.total.get(),
            at,
            contributions,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Lead::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn list_by_profile(&mut self, profile_id: ProfileId) -> Result<Vec<Lead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            LeadRow,
            r#"
            SELECT id, profile_id, domain, status, score, score_breakdown, confidence, verdict, llm_score, qualified_at, first_seen_at, last_seen_at, updated_at
            FROM leads
            WHERE profile_id = $1
            ORDER BY score DESC, domain ASC
            "#,
            profile_id.0,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(Lead::try_from).collect()
    }

    #[tracing::instrument(skip(self, query), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn list_page(
        &mut self,
        profile_id: ProfileId,
        query: &LeadQuery,
    ) -> Result<LeadPage, CoreError> {
        if query.group_by_company {
            return self.list_grouped_page(profile_id, query).await;
        }
        let mut count = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM leads");
        push_filters(&mut count, profile_id, query);

        let mut select = QueryBuilder::<Postgres>::new(
            "SELECT leads.id, leads.profile_id, leads.domain, leads.status, leads.score, leads.score_breakdown, leads.confidence, leads.verdict, leads.llm_score, leads.qualified_at, leads.first_seen_at, leads.last_seen_at, leads.updated_at FROM leads",
        );
        push_filters(&mut select, profile_id, query);
        select.push(order_by(query.sort));
        select.push(" LIMIT ").push_bind(i64::from(query.limit));
        select
            .push(" OFFSET ")
            .push_bind(i64::try_from(query.offset).unwrap_or(i64::MAX));

        let mut tx = self.tx.lock().await;
        let total: i64 = count
            .build_query_scalar()
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        let rows = select
            .build_query_as::<LeadRow>()
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;

        Ok(LeadPage {
            items: rows
                .into_iter()
                .map(Lead::try_from)
                .collect::<Result<_, _>>()?,
            total: u64::try_from(total).unwrap_or_default(),
            members: HashMap::new(),
        })
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "profiles"), err)]
    async fn profile_summaries(&mut self) -> Result<Vec<ProfileSummary>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, ProfileSummaryRow>(
            r#"
            SELECT
                p.id,
                p.name,
                p.active,
                (SELECT MAX(r.revision) FROM profile_revisions r WHERE r.profile_id = p.id) AS revision,
                (
                    SELECT COUNT(*) FROM leads
                    WHERE leads.profile_id = p.id
                    AND NOT EXISTS (
                        SELECT 1 FROM suppression_list sup
                        WHERE sup.kind = 'domain'
                        AND (sup.value = leads.domain
                             OR right(leads.domain, length(sup.value) + 1) = '.' || sup.value)
                    )
                ) AS lead_count
            FROM profiles p
            ORDER BY p.name ASC, p.id ASC
            "#,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(ProfileSummary::try_from).collect()
    }
}
