use std::{
    fmt::Write as _,
    sync::{Mutex, MutexGuard, PoisonError},
};

use axum::{
    body::Body,
    extract::{Path, Query, State},
};
use http::{
    HeaderValue, StatusCode,
    header::{CONTENT_DISPOSITION, CONTENT_TYPE},
};
use lodestone_core::{LeadEvent, LeadEventKind, LeadRecord, LeadSink, ProfileId, SinkError};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    errors::ApiError,
    leads::{ListLeadsParams, invalid},
    state::AppState,
};

pub const CSV_COLUMNS: [&str; 18] = [
    "lead_id",
    "profile_id",
    "profile_name",
    "domain",
    "status",
    "score",
    "first_seen_at",
    "last_seen_at",
    "contact_address",
    "contact_method",
    "contact_confidence",
    "summary",
    "approach_angle",
    "draft_subject",
    "draft_status",
    "signal_kinds",
    "top_evidence_quote",
    "top_evidence_provenance",
];

pub enum Cell<'a> {
    Text(&'a str),
    Number(f64),
}

fn is_formula_trigger(first: char) -> bool {
    matches!(first, '=' | '+' | '-' | '@' | '\t' | '\r')
}

fn needs_quotes(text: &str) -> bool {
    text.contains([',', '"', '\n', '\r'])
}

pub fn encode_cell(out: &mut String, cell: &Cell<'_>) {
    let neutralised;
    let text = match cell {
        Cell::Number(value) => {
            let _ = write!(out, "{value}");
            return;
        }
        Cell::Text(text) => {
            if text.chars().next().is_some_and(is_formula_trigger) {
                neutralised = format!("'{text}");
                neutralised.as_str()
            } else {
                text
            }
        }
    };
    if needs_quotes(text) {
        out.push('"');
        for c in text.chars() {
            if c == '"' {
                out.push('"');
            }
            out.push(c);
        }
        out.push('"');
    } else {
        out.push_str(text);
    }
}

pub fn encode_row(out: &mut String, cells: &[Cell<'_>]) {
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        encode_cell(out, cell);
    }
    out.push_str("\r\n");
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct CsvSink {
    buffer: Mutex<String>,
}

impl Default for CsvSink {
    fn default() -> Self {
        Self::new()
    }
}

impl CsvSink {
    pub fn new() -> Self {
        let mut header = String::new();
        let cells: Vec<Cell<'_>> = CSV_COLUMNS.iter().map(|name| Cell::Text(name)).collect();
        encode_row(&mut header, &cells);
        Self {
            buffer: Mutex::new(header),
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
            .into_bytes()
    }
}

fn csv_row(record: &LeadRecord) -> String {
    let profile_id = record.profile.id.to_string();
    let lead_id = record.lead_id.to_string();
    let first_seen = record.first_seen_at.to_rfc3339();
    let last_seen = record.last_seen_at.to_rfc3339();
    let contact = record.contact.as_ref();
    let assessment = record.assessment.as_ref();
    let draft = assessment.and_then(|assessment| assessment.draft.as_ref());
    let kinds = record
        .signals
        .iter()
        .map(|signal| signal.kind.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let evidence = assessment.and_then(|assessment| assessment.evidence.first());
    let cells = [
        Cell::Text(&lead_id),
        Cell::Text(&profile_id),
        Cell::Text(&record.profile.name),
        Cell::Text(&record.domain),
        Cell::Text(&record.status),
        Cell::Number(record.score),
        Cell::Text(&first_seen),
        Cell::Text(&last_seen),
        Cell::Text(contact.map_or("", |contact| contact.address.as_str())),
        Cell::Text(contact.map_or("", |contact| contact.method.as_str())),
        contact.map_or(Cell::Text(""), |contact| Cell::Number(contact.confidence)),
        Cell::Text(assessment.map_or("", |assessment| assessment.summary.as_str())),
        Cell::Text(assessment.map_or("", |assessment| assessment.approach_angle.as_str())),
        Cell::Text(draft.map_or("", |draft| draft.subject.as_str())),
        Cell::Text(draft.map_or("", |draft| draft.status.as_str())),
        Cell::Text(&kinds),
        Cell::Text(evidence.map_or("", |evidence| evidence.quote.as_str())),
        Cell::Text(evidence.map_or("", |evidence| evidence.provenance.as_str())),
    ];
    let mut row = String::new();
    encode_row(&mut row, &cells);
    row
}

impl LeadSink for CsvSink {
    async fn deliver(&self, event: &LeadEvent) -> Result<(), SinkError> {
        lock(&self.buffer).push_str(&csv_row(&event.record));
        Ok(())
    }
}

pub struct JsonSink {
    state: Mutex<(Vec<u8>, bool)>,
}

impl Default for JsonSink {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonSink {
    pub fn new() -> Self {
        Self {
            state: Mutex::new((vec![b'['], false)),
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let (mut buffer, _) = self
            .state
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
        buffer.push(b']');
        buffer
    }
}

impl LeadSink for JsonSink {
    async fn deliver(&self, event: &LeadEvent) -> Result<(), SinkError> {
        let encoded = serde_json::to_vec(&event.record)
            .map_err(|error| SinkError::Failed(error.to_string()))?;
        let mut state = lock(&self.state);
        if state.1 {
            state.0.push(b',');
        }
        state.0.extend_from_slice(&encoded);
        state.1 = true;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportFormat {
    Json,
    Csv,
}

impl ExportFormat {
    fn parse(raw: Option<&str>) -> Result<Self, ApiError> {
        match raw {
            None | Some("json") => Ok(Self::Json),
            Some("csv") => Ok(Self::Csv),
            Some(_) => Err(invalid("format", "must be json or csv")),
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Csv => "text/csv; charset=utf-8",
        }
    }
}

fn file_stem(name: &str, fallback: &str) -> String {
    let mut stem = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            stem.push(c.to_ascii_lowercase());
        } else if !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        fallback.to_owned()
    } else {
        stem.to_owned()
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExportLeadsParams {
    /// json (default) or csv.
    format: Option<String>,
    /// One of new, qualified, validated, rejected, contacted, uncertain.
    status: Option<String>,
    /// Comma-separated verdicts to keep: fiable, a_verifier, ecarte, en_attente. All leads when absent.
    verdict: Option<String>,
    /// Minimum confidence in percent, inclusive. Leads without a confidence are left out.
    #[param(value_type = Option<f64>, minimum = 0, maximum = 100)]
    min_confidence: Option<String>,
    /// Minimum score, inclusive. Must be a non-negative number.
    #[param(value_type = Option<f64>, minimum = 0)]
    min_score: Option<String>,
    /// Case-insensitive substring of the lead domain.
    domain: Option<String>,
    /// Only leads having at least one signal of this kind.
    signal: Option<String>,
    /// One of confidence_desc (default), confidence_asc, score_desc, score_asc, llm_score_desc, llm_score_asc, last_seen_desc, first_seen_desc, domain_asc.
    sort: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/profiles/{profile_id}/leads/export",
    tag = "leads",
    security(("bearer_auth" = [])),
    params(
        ("profile_id" = String, Path, description = "Profile identifier (UUID)"),
        ExportLeadsParams,
    ),
    responses(
        (
            status = 200,
            description = "Every matching lead of the profile as an attachment (application/json, or text/csv with format=csv), suppressed domains and addresses excluded. Open to service accounts",
            content_type = "application/json",
            body = Vec<LeadRecord>
        ),
        (status = 400, description = "A filter or the format is invalid"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 404, description = "Unknown profile"),
        (status = 422, description = "More leads match than one export holds, narrow the filters"),
    )
)]
pub async fn export_leads(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    Query(params): Query<ExportLeadsParams>,
) -> Result<axum::response::Response, ApiError> {
    let profile_id: ProfileId = profile_id
        .parse()
        .map_err(|_| invalid("profile_id", "must be a UUID"))?;
    let format = ExportFormat::parse(params.format.as_deref())?;
    let query = ListLeadsParams {
        status: params.status,
        verdict: params.verdict,
        min_confidence: params.min_confidence,
        min_score: params.min_score,
        domain: params.domain,
        signal: params.signal,
        group_by: None,
        sort: params.sort,
        page: None,
        per_page: None,
    }
    .into_query()?;

    let set = state.usecase.lead_records(profile_id, query).await?;

    let occurred_at = chrono::Utc::now();
    let bytes = match format {
        ExportFormat::Json => {
            let sink = JsonSink::new();
            deliver_all(&sink, set.records, occurred_at).await?;
            sink.into_bytes()
        }
        ExportFormat::Csv => {
            let sink = CsvSink::new();
            deliver_all(&sink, set.records, occurred_at).await?;
            sink.into_bytes()
        }
    };

    let stem = file_stem(&set.profile.name, &profile_id.0.to_string());
    let disposition = format!(
        "attachment; filename=\"{stem}-leads.{}\"",
        format.extension()
    );
    let mut response = axum::response::Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static(format.content_type()),
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|_| ApiError::Internal)?,
    );
    Ok(response)
}

async fn deliver_all(
    sink: &impl LeadSink,
    records: Vec<LeadRecord>,
    occurred_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), ApiError> {
    for record in records {
        let event = LeadEvent {
            kind: LeadEventKind::Validated,
            occurred_at,
            record,
        };
        sink.deliver(&event).await.map_err(|error| {
            tracing::error!(%error, "export sink failed");
            ApiError::Internal
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use lodestone_core::{
        AssessmentRecord, ContactRecord, DraftOutput, EvidenceRecord, ProfileRecord, SignalRecord,
    };
    use uuid::Uuid;

    use super::*;

    fn text(raw: &str) -> String {
        let mut out = String::new();
        encode_cell(&mut out, &Cell::Text(raw));
        out
    }

    #[test]
    fn plain_cells_are_left_alone() {
        assert_eq!(text("acme.com"), "acme.com");
        assert_eq!(text(""), "");
    }

    #[test]
    fn commas_quotes_and_line_breaks_are_quoted() {
        assert_eq!(text("a,b"), "\"a,b\"");
        assert_eq!(text("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(text("one\ntwo"), "\"one\ntwo\"");
        assert_eq!(text("one\r\ntwo"), "\"one\r\ntwo\"");
    }

    #[test]
    fn formula_triggers_are_neutralised() {
        for trigger in ["=", "+", "-", "@", "\t", "\r"] {
            let cell = text(&format!("{trigger}SUM(A1)"));
            assert!(
                cell.trim_start_matches('"').starts_with('\''),
                "{trigger:?}"
            );
        }
        assert_eq!(text("=1+1"), "'=1+1");
        assert_eq!(text("+cmd|' /C calc'!A0"), "'+cmd|' /C calc'!A0");
        assert_eq!(text("a=b"), "a=b");
    }

    #[test]
    fn a_neutralised_cell_is_still_quoted_when_needed() {
        assert_eq!(text("=a,b"), "\"'=a,b\"");
    }

    #[test]
    fn numbers_are_written_verbatim_even_when_negative() {
        let mut out = String::new();
        encode_row(&mut out, &[Cell::Number(-1.5), Cell::Number(42.0)]);
        assert_eq!(out, "-1.5,42\r\n");
    }

    fn record(domain: &str) -> LeadRecord {
        let at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        LeadRecord {
            lead_id: Uuid::from_u128(1),
            profile: ProfileRecord {
                id: Uuid::from_u128(2),
                name: "Alpha, Inc".to_owned(),
                revision: Some(1),
            },
            domain: domain.to_owned(),
            status: "validated".to_owned(),
            score: 12.5,
            score_breakdown: vec![],
            first_seen_at: at,
            last_seen_at: at,
            contact: Some(ContactRecord {
                address: "security@acme.com".to_owned(),
                method: "public_page".to_owned(),
                confidence: 0.8,
                provenance: "https://acme.com/contact".to_owned(),
            }),
            assessment: Some(AssessmentRecord {
                summary: "Runs\nKeycloak".to_owned(),
                approach_angle: "=HYPERLINK(\"x\")".to_owned(),
                evidence: vec![EvidenceRecord {
                    quote: "Powered by Keycloak".to_owned(),
                    provenance: "https://acme.com/about".to_owned(),
                }],
                draft: Some(DraftOutput {
                    subject: "Hello".to_owned(),
                    body: "Body".to_owned(),
                    status: "approved".to_owned(),
                }),
            }),
            signals: vec![
                SignalRecord {
                    kind: "product_detected".to_owned(),
                    value: None,
                    source: "http_probe".to_owned(),
                    provenance: "https://acme.com/".to_owned(),
                    observed_at: at,
                },
                SignalRecord {
                    kind: "dns_cname".to_owned(),
                    value: None,
                    source: "dns_cname".to_owned(),
                    provenance: "dns:acme.com".to_owned(),
                    observed_at: at,
                },
            ],
        }
    }

    fn event(record: LeadRecord) -> LeadEvent {
        LeadEvent {
            kind: LeadEventKind::Validated,
            occurred_at: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
            record,
        }
    }

    #[tokio::test]
    async fn the_csv_header_lists_the_columns_in_order() {
        let csv = String::from_utf8(CsvSink::new().into_bytes()).unwrap();
        assert_eq!(
            csv,
            "lead_id,profile_id,profile_name,domain,status,score,first_seen_at,last_seen_at,contact_address,contact_method,contact_confidence,summary,approach_angle,draft_subject,draft_status,signal_kinds,top_evidence_quote,top_evidence_provenance\r\n"
        );
    }

    #[tokio::test]
    async fn a_csv_row_is_flat_quoted_and_neutralised() {
        let sink = CsvSink::new();
        sink.deliver(&event(record("acme.com"))).await.unwrap();
        let csv = String::from_utf8(sink.into_bytes()).unwrap();
        let row = csv.split_once("\r\n").unwrap().1;
        assert_eq!(
            row,
            "00000000-0000-0000-0000-000000000001,00000000-0000-0000-0000-000000000002,\"Alpha, Inc\",acme.com,validated,12.5,2027-01-15T08:00:00+00:00,2027-01-15T08:00:00+00:00,security@acme.com,public_page,0.8,\"Runs\nKeycloak\",\"'=HYPERLINK(\"\"x\"\")\",Hello,approved,product_detected;dns_cname,Powered by Keycloak,https://acme.com/about\r\n"
        );
    }

    #[tokio::test]
    async fn a_record_without_contact_or_assessment_leaves_blank_cells() {
        let mut bare = record("acme.com");
        bare.contact = None;
        bare.assessment = None;
        bare.signals.clear();
        let sink = CsvSink::new();
        sink.deliver(&event(bare)).await.unwrap();
        let csv = String::from_utf8(sink.into_bytes()).unwrap();
        let row = csv.split_once("\r\n").unwrap().1;
        assert!(row.ends_with(",,,,,,,,,,\r\n"), "{row}");
    }

    #[tokio::test]
    async fn the_json_export_is_an_array_of_records() {
        let sink = JsonSink::new();
        sink.deliver(&event(record("a.com"))).await.unwrap();
        sink.deliver(&event(record("b.com"))).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&sink.into_bytes()).unwrap();
        let items = value.as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["domain"], "a.com");
        assert_eq!(
            items[1]["profile"]["id"],
            "00000000-0000-0000-0000-000000000002"
        );
        assert_eq!(items[0]["lead_id"], "00000000-0000-0000-0000-000000000001");
    }

    #[test]
    fn an_empty_json_export_is_an_empty_array() {
        assert_eq!(JsonSink::new().into_bytes(), b"[]");
    }

    #[test]
    fn the_file_name_is_derived_from_the_profile_name() {
        assert_eq!(file_stem("Alpha, Inc", "id"), "alpha-inc");
        assert_eq!(file_stem("\"; evil", "id"), "evil");
        assert_eq!(file_stem("!!!", "fallback"), "fallback");
    }
}
