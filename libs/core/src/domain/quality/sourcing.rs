use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    lead::{LeadId, str_enum},
    profile::{
        ProfileId,
        definition::{Agent, ProfileDefinition, SearchAgent},
    },
    qualification::Verdict,
    quality::{DecisionKind, metrics::ratio},
    text::fold,
};

pub const MIN_DECIDED_PER_KEYWORD: u64 = 5;
pub const MIN_REJECTED_WITH_WORD: u64 = 3;
pub const MAX_EXCLUSION_SUGGESTIONS_PER_AGENT: usize = 5;
pub const MIN_EXCLUSION_WORD_CHARS: usize = 4;

const STOP_WORDS: &[&str] = &[
    "avec", "dans", "pour", "sans", "sous", "chez", "vers", "plus", "tout", "tous", "toute",
    "toutes", "cette", "cela", "mais", "donc", "dont", "aussi", "entre", "leur", "leurs", "nous",
    "vous", "elle", "elles", "sont", "etre", "avoir", "fait", "faire", "comme", "ainsi", "alors",
    "apres", "avant", "depuis", "encore", "parce", "pendant", "puis", "quand", "selon", "tres",
    "votre", "notre", "ceux", "celui", "celle", "autre", "autres", "meme", "peut", "peuvent",
    "doit", "lors", "quel", "quelle", "quels", "quelles", "with", "from", "that", "this", "these",
    "those", "have", "will", "your", "their", "they", "them", "then", "than", "into", "over",
    "under", "about", "after", "before", "between", "also", "each", "more", "most", "such", "very",
    "been", "being", "were", "what", "when", "where", "which", "while", "would", "could", "should",
    "other", "some", "only", "just", "like", "many", "much", "must", "does", "done", "here",
    "there", "both", "whose", "whom", "because", "within", "without",
];

#[derive(Debug, Clone, PartialEq)]
pub struct SourcingRow {
    pub profile_id: ProfileId,
    pub agent_id: String,
    pub lead_id: LeadId,
    pub matched: Vec<String>,
    pub title: String,
    pub verdict: Option<Verdict>,
    pub decision: Option<DecisionKind>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct YieldVerdicts {
    pub reliable: u64,
    pub to_verify: u64,
    pub discarded: u64,
    pub none: u64,
}

impl YieldVerdicts {
    fn add(&mut self, verdict: Option<Verdict>) {
        match verdict {
            Some(Verdict::Reliable) => self.reliable += 1,
            Some(Verdict::ToCheck) => self.to_verify += 1,
            Some(Verdict::Discarded) => self.discarded += 1,
            None => self.none += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeywordYield {
    pub keyword: String,
    pub leads: u64,
    pub exclusive_leads: u64,
    pub verdicts: YieldVerdicts,
    pub validated: u64,
    pub rejected: u64,
    pub precision: Option<f64>,
}

impl KeywordYield {
    pub fn decided(&self) -> u64 {
        self.validated + self.rejected
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentYield {
    pub profile_id: ProfileId,
    pub agent_id: String,
    pub agent_name: Option<String>,
    pub leads: u64,
    pub verdicts: YieldVerdicts,
    pub validated: u64,
    pub rejected: u64,
    pub precision: Option<f64>,
    pub keywords: Vec<KeywordYield>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionAction {
    RemoveKeyword,
    AddExclusion,
}

str_enum!(SuggestionAction {
    RemoveKeyword => "remove_keyword",
    AddExclusion => "add_exclusion",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordSuggestion {
    pub profile_id: ProfileId,
    pub agent_id: String,
    pub action: SuggestionAction,
    pub word: String,
    pub decided: u64,
    pub rejected: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourcingYield {
    pub agents: Vec<AgentYield>,
    pub suggestions: Vec<KeywordSuggestion>,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentRules<'a> {
    pub profile_id: ProfileId,
    pub agent: &'a Agent,
}

struct LeadView<'a> {
    verdict: Option<Verdict>,
    decision: Option<DecisionKind>,
    signals: Vec<&'a SourcingRow>,
}

impl LeadView<'_> {
    fn matched(&self) -> BTreeSet<&str> {
        self.signals
            .iter()
            .flat_map(|signal| signal.matched.iter().map(String::as_str))
            .collect()
    }

    fn title_words(&self) -> BTreeSet<String> {
        self.signals
            .iter()
            .flat_map(|signal| words(&signal.title))
            .collect()
    }
}

type AgentLeads<'a> = BTreeMap<Uuid, LeadView<'a>>;

#[derive(Default)]
struct Tally {
    leads: u64,
    exclusive: u64,
    verdicts: YieldVerdicts,
    validated: u64,
    rejected: u64,
}

impl Tally {
    fn add(&mut self, lead: &LeadView<'_>) {
        self.leads += 1;
        self.verdicts.add(lead.verdict);
        match lead.decision {
            Some(DecisionKind::LeadValidated) => self.validated += 1,
            Some(DecisionKind::LeadRejected) => self.rejected += 1,
            _ => {}
        }
    }

    fn precision(&self) -> Option<f64> {
        ratio(self.validated, self.validated + self.rejected)
    }
}

fn words(title: &str) -> Vec<String> {
    fold(title)
        .split(|letter: char| !letter.is_alphanumeric())
        .filter(|word| word.chars().count() >= MIN_EXCLUSION_WORD_CHARS)
        .filter(|word| !word.chars().all(|letter| letter.is_ascii_digit()))
        .map(str::to_owned)
        .collect()
}

fn keyword_yields(leads: &AgentLeads<'_>) -> Vec<KeywordYield> {
    let mut tallies: BTreeMap<&str, Tally> = BTreeMap::new();
    for lead in leads.values() {
        let matched = lead.matched();
        let exclusive = u64::from(matched.len() == 1);
        for keyword in matched {
            let tally = tallies.entry(keyword).or_default();
            tally.add(lead);
            tally.exclusive += exclusive;
        }
    }
    let mut yields: Vec<KeywordYield> = tallies
        .into_iter()
        .map(|(keyword, tally)| KeywordYield {
            keyword: keyword.to_owned(),
            leads: tally.leads,
            exclusive_leads: tally.exclusive,
            verdicts: tally.verdicts,
            validated: tally.validated,
            rejected: tally.rejected,
            precision: tally.precision(),
        })
        .collect();
    yields.sort_by(|a, b| {
        b.leads
            .cmp(&a.leads)
            .then_with(|| a.keyword.cmp(&b.keyword))
    });
    yields
}

fn removal_suggestions(
    scope: (ProfileId, &str),
    search: &SearchAgent,
    keywords: &[KeywordYield],
) -> Vec<KeywordSuggestion> {
    if search.keywords.len() < 2 {
        return Vec::new();
    }
    let mut found: Vec<KeywordSuggestion> = keywords
        .iter()
        .filter(|row| search.keywords.contains(&row.keyword))
        .filter(|row| row.decided() >= MIN_DECIDED_PER_KEYWORD && row.validated == 0)
        .map(|row| KeywordSuggestion {
            profile_id: scope.0,
            agent_id: scope.1.to_owned(),
            action: SuggestionAction::RemoveKeyword,
            word: row.keyword.clone(),
            decided: row.decided(),
            rejected: row.rejected,
        })
        .collect();
    found.sort_by(|a, b| {
        b.rejected
            .cmp(&a.rejected)
            .then_with(|| a.word.cmp(&b.word))
    });
    found
}

fn exclusion_suggestions(
    scope: (ProfileId, &str),
    search: &SearchAgent,
    leads: &AgentLeads<'_>,
) -> Vec<KeywordSuggestion> {
    let mut known: HashSet<String> = search
        .keywords
        .iter()
        .chain(&search.exclusions)
        .map(|entry| fold(entry))
        .collect();
    known.extend(search.keywords.iter().flat_map(|keyword| words(keyword)));
    let mut rejected: HashMap<String, u64> = HashMap::new();
    let mut validated: HashSet<String> = HashSet::new();
    for lead in leads.values() {
        match lead.decision {
            Some(DecisionKind::LeadRejected) => {
                for word in lead.title_words() {
                    *rejected.entry(word).or_default() += 1;
                }
            }
            Some(DecisionKind::LeadValidated) => validated.extend(lead.title_words()),
            _ => {}
        }
    }
    let mut candidates: Vec<(String, u64)> = rejected
        .into_iter()
        .filter(|(word, count)| {
            *count >= MIN_REJECTED_WITH_WORD
                && !validated.contains(word)
                && !known.contains(word)
                && !STOP_WORDS.contains(&word.as_str())
        })
        .collect();
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    candidates
        .into_iter()
        .take(MAX_EXCLUSION_SUGGESTIONS_PER_AGENT)
        .map(|(word, count)| KeywordSuggestion {
            profile_id: scope.0,
            agent_id: scope.1.to_owned(),
            action: SuggestionAction::AddExclusion,
            word,
            decided: count,
            rejected: count,
        })
        .collect()
}

impl SourcingYield {
    pub fn compute(rows: &[SourcingRow], rules: &[AgentRules<'_>]) -> Self {
        let mut grouped: BTreeMap<(Uuid, &str), AgentLeads<'_>> = BTreeMap::new();
        for row in rows {
            grouped
                .entry((row.profile_id.0, row.agent_id.as_str()))
                .or_default()
                .entry(row.lead_id.0)
                .or_insert_with(|| LeadView {
                    verdict: row.verdict,
                    decision: row.decision,
                    signals: Vec::new(),
                })
                .signals
                .push(row);
        }
        let current: HashMap<(Uuid, &str), &Agent> = rules
            .iter()
            .map(|rule| ((rule.profile_id.0, rule.agent.id().as_str()), rule.agent))
            .collect();
        let mut report = Self::default();
        for ((profile, agent_id), leads) in &grouped {
            let scope = (ProfileId(*profile), *agent_id);
            let agent = current.get(&(*profile, *agent_id)).copied();
            let keywords = keyword_yields(leads);
            if let Some(search) = agent.and_then(Agent::as_search) {
                report
                    .suggestions
                    .extend(removal_suggestions(scope, search, &keywords));
                report
                    .suggestions
                    .extend(exclusion_suggestions(scope, search, leads));
            }
            let mut total = Tally::default();
            for lead in leads.values() {
                total.add(lead);
            }
            report.agents.push(AgentYield {
                profile_id: scope.0,
                agent_id: (*agent_id).to_owned(),
                agent_name: agent.map(|agent| agent.name().to_owned()),
                leads: total.leads,
                verdicts: total.verdicts,
                validated: total.validated,
                rejected: total.rejected,
                precision: total.precision(),
                keywords,
            });
        }
        report
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionEdit {
    pub agent_id: String,
    pub action: SuggestionAction,
    pub word: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EditProblem {
    #[error("the profile has no search agent with this id")]
    UnknownAgent,
    #[error("the keyword is no longer in the agent")]
    KeywordMissing,
    #[error("the exclusion is already in the agent")]
    ExclusionPresent,
    #[error("the agent would be left without a keyword")]
    LastKeyword,
    #[error("the word must not be empty")]
    EmptyWord,
}

impl SuggestionEdit {
    pub fn apply(&self, definition: &ProfileDefinition) -> Result<ProfileDefinition, EditProblem> {
        let mut edited = definition.clone();
        let agent = edited
            .agents
            .iter_mut()
            .filter_map(|agent| match agent {
                Agent::Search(search) => Some(search),
                _ => None,
            })
            .find(|search| search.id.as_str() == self.agent_id)
            .ok_or(EditProblem::UnknownAgent)?;
        match self.action {
            SuggestionAction::RemoveKeyword => self.remove_keyword(agent)?,
            SuggestionAction::AddExclusion => self.add_exclusion(agent)?,
        }
        Ok(edited)
    }

    fn remove_keyword(&self, agent: &mut SearchAgent) -> Result<(), EditProblem> {
        let remaining = agent
            .keywords
            .iter()
            .filter(|keyword| **keyword != self.word)
            .count();
        if remaining == agent.keywords.len() {
            return Err(EditProblem::KeywordMissing);
        }
        if remaining == 0 {
            return Err(EditProblem::LastKeyword);
        }
        agent.keywords.retain(|keyword| *keyword != self.word);
        Ok(())
    }

    fn add_exclusion(&self, agent: &mut SearchAgent) -> Result<(), EditProblem> {
        let word = self.word.trim();
        if word.is_empty() {
            return Err(EditProblem::EmptyWord);
        }
        let folded = fold(word);
        if agent.exclusions.iter().any(|entry| fold(entry) == folded) {
            return Err(EditProblem::ExclusionPresent);
        }
        agent.exclusions.push(word.to_owned());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile::{
        ValidatedProfile,
        definition::{AgentId, CertificatesAgent},
    };

    const IAM: &str = include_str!("../../../../../docs/examples/profiles/iam-ferriskey.json");

    fn profile(n: u128) -> ProfileId {
        ProfileId(Uuid::from_u128(n))
    }

    struct Row(SourcingRow);

    fn row(lead: u128, matched: &[&str]) -> Row {
        Row(SourcingRow {
            profile_id: profile(1),
            agent_id: "jobs".to_owned(),
            lead_id: LeadId(Uuid::from_u128(lead)),
            matched: matched.iter().map(|word| (*word).to_owned()).collect(),
            title: String::new(),
            verdict: None,
            decision: None,
        })
    }

    impl Row {
        fn agent(mut self, agent: &str) -> Self {
            self.0.agent_id = agent.to_owned();
            self
        }

        fn profile(mut self, n: u128) -> Self {
            self.0.profile_id = profile(n);
            self
        }

        fn title(mut self, title: &str) -> Self {
            self.0.title = title.to_owned();
            self
        }

        fn verdict(mut self, verdict: Verdict) -> Self {
            self.0.verdict = Some(verdict);
            self
        }

        fn validated(mut self) -> Self {
            self.0.decision = Some(DecisionKind::LeadValidated);
            self
        }

        fn rejected(mut self) -> Self {
            self.0.decision = Some(DecisionKind::LeadRejected);
            self
        }

        fn done(self) -> SourcingRow {
            self.0
        }
    }

    fn rows(items: Vec<Row>) -> Vec<SourcingRow> {
        items.into_iter().map(Row::done).collect()
    }

    fn search(id: &str, keywords: &[&str], exclusions: &[&str]) -> Agent {
        Agent::Search(SearchAgent::new(
            AgentId::new(id),
            format!("Agent {id}"),
            &["boamp"],
            keywords,
            exclusions,
        ))
    }

    fn rules(agent: &Agent) -> AgentRules<'_> {
        AgentRules {
            profile_id: profile(1),
            agent,
        }
    }

    fn agent_of<'a>(report: &'a SourcingYield, id: &str) -> &'a AgentYield {
        report
            .agents
            .iter()
            .find(|agent| agent.agent_id == id)
            .unwrap()
    }

    fn keyword_of<'a>(agent: &'a AgentYield, keyword: &str) -> &'a KeywordYield {
        agent
            .keywords
            .iter()
            .find(|row| row.keyword == keyword)
            .unwrap()
    }

    fn removals(report: &SourcingYield) -> Vec<&str> {
        report
            .suggestions
            .iter()
            .filter(|suggestion| suggestion.action == SuggestionAction::RemoveKeyword)
            .map(|suggestion| suggestion.word.as_str())
            .collect()
    }

    fn exclusions(report: &SourcingYield) -> Vec<&str> {
        report
            .suggestions
            .iter()
            .filter(|suggestion| suggestion.action == SuggestionAction::AddExclusion)
            .map(|suggestion| suggestion.word.as_str())
            .collect()
    }

    fn rejected_with_titles(titles: &[&str]) -> Vec<Row> {
        titles
            .iter()
            .enumerate()
            .map(|(index, title)| row(100 + index as u128, &["sso"]).title(title).rejected())
            .collect()
    }

    #[test]
    fn leads_are_counted_per_agent_and_per_keyword_with_their_decisions() {
        let data = rows(vec![
            row(1, &["sso", "mfa"])
                .verdict(Verdict::Reliable)
                .validated(),
            row(2, &["sso"]).verdict(Verdict::ToCheck).rejected(),
            row(3, &["sso"]).verdict(Verdict::Discarded).rejected(),
            row(4, &["mfa"]),
            row(5, &["oidc"])
                .agent("tenders")
                .verdict(Verdict::Reliable)
                .validated(),
            row(6, &["oidc"]).agent("tenders").rejected(),
        ]);
        let report = SourcingYield::compute(&data, &[]);

        assert_eq!(report.agents.len(), 2);
        let jobs = agent_of(&report, "jobs");
        assert_eq!(jobs.leads, 4);
        assert_eq!(
            jobs.verdicts,
            YieldVerdicts {
                reliable: 1,
                to_verify: 1,
                discarded: 1,
                none: 1
            }
        );
        assert_eq!((jobs.validated, jobs.rejected), (1, 2));
        assert_eq!(jobs.precision, Some(1.0 / 3.0));
        assert_eq!(jobs.agent_name, None);

        let sso = keyword_of(jobs, "sso");
        assert_eq!(sso.leads, 3);
        assert_eq!(
            sso.verdicts,
            YieldVerdicts {
                reliable: 1,
                to_verify: 1,
                discarded: 1,
                none: 0
            }
        );
        assert_eq!((sso.validated, sso.rejected), (1, 2));
        assert_eq!(sso.precision, Some(1.0 / 3.0));
        let mfa = keyword_of(jobs, "mfa");
        assert_eq!(mfa.leads, 2);
        assert_eq!((mfa.validated, mfa.rejected), (1, 0));
        assert_eq!(mfa.precision, Some(1.0));
        assert_eq!(mfa.verdicts.none, 1);

        let tenders = agent_of(&report, "tenders");
        assert_eq!(tenders.leads, 2);
        assert_eq!(tenders.precision, Some(0.5));
        assert_eq!(keyword_of(tenders, "oidc").leads, 2);
    }

    #[test]
    fn precision_is_absent_until_a_lead_is_decided() {
        let data = rows(vec![row(1, &["sso"]).verdict(Verdict::Reliable)]);
        let report = SourcingYield::compute(&data, &[]);
        let jobs = agent_of(&report, "jobs");
        assert_eq!(jobs.precision, None);
        assert_eq!(keyword_of(jobs, "sso").precision, None);
    }

    #[test]
    fn a_lead_with_several_signals_of_an_agent_counts_once() {
        let data = rows(vec![
            row(1, &["sso"]).rejected(),
            row(1, &["sso", "mfa"]).rejected(),
            row(1, &["mfa"]).rejected(),
        ]);
        let report = SourcingYield::compute(&data, &[]);
        let jobs = agent_of(&report, "jobs");
        assert_eq!(jobs.leads, 1);
        assert_eq!(jobs.rejected, 1);
        assert_eq!(keyword_of(jobs, "sso").leads, 1);
        assert_eq!(keyword_of(jobs, "mfa").leads, 1);
    }

    #[test]
    fn the_same_lead_counts_for_each_agent_that_found_it() {
        let data = rows(vec![
            row(1, &["sso"]).validated(),
            row(1, &["oidc"]).agent("tenders").validated(),
        ]);
        let report = SourcingYield::compute(&data, &[]);
        assert_eq!(agent_of(&report, "jobs").validated, 1);
        assert_eq!(agent_of(&report, "tenders").validated, 1);
    }

    #[test]
    fn a_profile_keeps_its_own_agents() {
        let data = rows(vec![
            row(1, &["sso"]).profile(2),
            row(2, &["sso"]).profile(1),
        ]);
        let report = SourcingYield::compute(&data, &[]);
        assert_eq!(report.agents.len(), 2);
        assert_eq!(report.agents[0].profile_id, profile(1));
        assert_eq!(report.agents[1].profile_id, profile(2));
    }

    #[test]
    fn exclusive_leads_would_be_lost_without_the_keyword() {
        let data = rows(vec![
            row(1, &["sso"]),
            row(2, &["sso"]),
            row(3, &["sso", "mfa"]),
            row(4, &["sso"]),
            row(4, &["mfa"]),
            row(5, &["sso"]),
            row(5, &[]),
        ]);
        let report = SourcingYield::compute(&data, &[]);
        let jobs = agent_of(&report, "jobs");
        assert_eq!(keyword_of(jobs, "sso").leads, 5);
        assert_eq!(keyword_of(jobs, "sso").exclusive_leads, 3);
        assert_eq!(keyword_of(jobs, "mfa").leads, 2);
        assert_eq!(keyword_of(jobs, "mfa").exclusive_leads, 0);
    }

    #[test]
    fn keywords_are_sorted_by_leads_then_name() {
        let data = rows(vec![
            row(1, &["zeta"]),
            row(2, &["alpha"]),
            row(3, &["beta"]),
            row(4, &["beta"]),
            row(5, &["alpha", "beta"]),
        ]);
        let report = SourcingYield::compute(&data, &[]);
        let names: Vec<&str> = agent_of(&report, "jobs")
            .keywords
            .iter()
            .map(|row| row.keyword.as_str())
            .collect();
        assert_eq!(names, ["beta", "alpha", "zeta"]);
    }

    #[test]
    fn agents_without_keywords_still_report_their_leads() {
        let data = rows(vec![row(1, &[]).agent("certs").verdict(Verdict::Reliable)]);
        let report = SourcingYield::compute(&data, &[]);
        let certs = agent_of(&report, "certs");
        assert_eq!(certs.leads, 1);
        assert!(certs.keywords.is_empty());
    }

    #[test]
    fn the_name_comes_from_the_current_agent() {
        let data = rows(vec![row(1, &["sso"])]);
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert_eq!(
            agent_of(&report, "jobs").agent_name.as_deref(),
            Some("Agent jobs")
        );
    }

    fn decided_keyword(keyword: &str, validated: u128, rejected: u128) -> Vec<Row> {
        let mut data = Vec::new();
        for index in 0..validated {
            data.push(row(1_000 + index, &[keyword]).validated());
        }
        for index in 0..rejected {
            data.push(row(2_000 + index, &[keyword]).rejected());
        }
        data
    }

    #[test]
    fn a_keyword_with_enough_decided_leads_and_none_validated_is_suggested_for_removal() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let data = rows(decided_keyword("sso", 0, MIN_DECIDED_PER_KEYWORD as u128));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert_eq!(report.suggestions.len(), 1);
        assert_eq!(
            report.suggestions[0],
            KeywordSuggestion {
                profile_id: profile(1),
                agent_id: "jobs".to_owned(),
                action: SuggestionAction::RemoveKeyword,
                word: "sso".to_owned(),
                decided: 5,
                rejected: 5,
            }
        );
    }

    #[test]
    fn a_keyword_under_the_decided_threshold_is_not_suggested() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let data = rows(decided_keyword(
            "sso",
            0,
            MIN_DECIDED_PER_KEYWORD as u128 - 1,
        ));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert!(removals(&report).is_empty());
    }

    #[test]
    fn one_validated_lead_protects_a_keyword() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let data = rows(decided_keyword("sso", 1, 9));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert!(removals(&report).is_empty());
    }

    #[test]
    fn undecided_leads_do_not_count_toward_the_threshold() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let mut data = decided_keyword("sso", 0, 4);
        data.extend((0..10).map(|index| row(3_000 + index, &["sso"])));
        let report = SourcingYield::compute(&rows(data), &[rules(&agent)]);
        assert!(removals(&report).is_empty());
    }

    #[test]
    fn a_keyword_that_left_the_profile_is_not_suggested() {
        let agent = search("jobs", &["mfa", "oidc"], &[]);
        let data = rows(decided_keyword("sso", 0, 8));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert!(removals(&report).is_empty());
    }

    #[test]
    fn the_last_keyword_of_an_agent_is_never_suggested_for_removal() {
        let agent = search("jobs", &["sso"], &[]);
        let data = rows(decided_keyword("sso", 0, 8));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert!(removals(&report).is_empty());
    }

    #[test]
    fn the_suggested_keyword_keeps_the_spelling_of_the_profile() {
        let agent = search("jobs", &["SSO", "mfa"], &[]);
        let data = rows(decided_keyword("SSO", 0, 6));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert_eq!(removals(&report), ["SSO"]);
    }

    #[test]
    fn nothing_is_suggested_for_an_agent_the_profile_no_longer_has() {
        let agent = search("other", &["sso", "mfa"], &[]);
        let data = rows(decided_keyword("sso", 0, 8));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert!(report.suggestions.is_empty());
    }

    #[test]
    fn nothing_is_suggested_for_an_agent_that_is_not_a_search_agent() {
        let agent = Agent::Certificates(CertificatesAgent {
            id: AgentId::new("jobs"),
            name: "Certificates".to_owned(),
            enabled: true,
            labels: vec!["auth".to_owned()],
            countries: Vec::new(),
        });
        let mut data = decided_keyword("sso", 0, 8);
        data.extend(rejected_with_titles(&["Stage A", "Stage B", "Stage C"]));
        let report = SourcingYield::compute(&rows(data), &[rules(&agent)]);
        assert!(report.suggestions.is_empty());
        assert_eq!(
            agent_of(&report, "jobs").agent_name.as_deref(),
            Some("Certificates")
        );
    }

    #[test]
    fn a_word_in_enough_rejected_titles_and_no_validated_one_is_suggested_as_an_exclusion() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let data = rows(rejected_with_titles(&[
            "Stage développeur SSO",
            "Stage ingénieur",
            "Stage de fin d'études",
        ]));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert_eq!(exclusions(&report), ["stage"]);
        assert_eq!(
            report.suggestions[0],
            KeywordSuggestion {
                profile_id: profile(1),
                agent_id: "jobs".to_owned(),
                action: SuggestionAction::AddExclusion,
                word: "stage".to_owned(),
                decided: 3,
                rejected: 3,
            }
        );
    }

    #[test]
    fn a_word_in_too_few_rejected_titles_is_not_suggested() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let data = rows(rejected_with_titles(&[
            "Stage SSO",
            "Stage IAM",
            "Poste IAM",
        ]));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert!(exclusions(&report).is_empty());
    }

    #[test]
    fn a_word_also_in_a_validated_title_is_not_suggested() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let mut data = rejected_with_titles(&["Stage A", "Stage B", "Stage C"]);
        data.push(row(500, &["sso"]).title("Stage retenu").validated());
        let report = SourcingYield::compute(&rows(data), &[rules(&agent)]);
        assert!(exclusions(&report).is_empty());
    }

    #[test]
    fn undecided_titles_are_ignored() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let mut data = rejected_with_titles(&["Stage A", "Stage B"]);
        data.push(row(500, &["sso"]).title("Stage C"));
        let report = SourcingYield::compute(&rows(data), &[rules(&agent)]);
        assert!(exclusions(&report).is_empty());
    }

    #[test]
    fn a_lead_counts_once_for_a_word_even_with_several_signals() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let data = rows(vec![
            row(1, &["sso"]).title("Stage un").rejected(),
            row(1, &["sso"]).title("Stage deux").rejected(),
            row(2, &["sso"]).title("Stage trois").rejected(),
        ]);
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert!(exclusions(&report).is_empty());
    }

    #[test]
    fn short_words_and_stop_words_are_never_suggested() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let data = rows(rejected_with_titles(&[
            "Poste pour IAM et SSO avec dans",
            "Poste pour IAM et SSO avec dans",
            "Poste pour IAM et SSO avec dans",
        ]));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert_eq!(exclusions(&report), ["poste"]);
    }

    #[test]
    fn english_stop_words_are_never_suggested() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let data = rows(rejected_with_titles(&[
            "with this from engineer",
            "with this from engineer",
            "with this from engineer",
        ]));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert_eq!(exclusions(&report), ["engineer"]);
    }

    #[test]
    fn words_already_keywords_or_exclusions_are_not_suggested_whatever_their_accents() {
        let agent = search("jobs", &["Identité", "mfa"], &["STAGE"]);
        let data = rows(rejected_with_titles(&[
            "identite stage alternance",
            "Identité Stage Alternance",
            "IDENTITE STAGE ALTERNANCE",
        ]));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert_eq!(exclusions(&report), ["alternance"]);
    }

    #[test]
    fn the_suggested_exclusion_is_folded() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let data = rows(rejected_with_titles(&[
            "Étudiant stagiaire",
            "Étudiant stagiaire",
            "ÉTUDIANT stagiaire",
        ]));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert_eq!(exclusions(&report), ["etudiant", "stagiaire"]);
    }

    #[test]
    fn at_most_five_exclusions_are_suggested_by_agent_most_frequent_first() {
        let agent = search("jobs", &["sso", "mfa"], &[]);
        let titles = [
            "alpha bravo charlie delta echos foxtrot golfe",
            "alpha bravo charlie delta echos foxtrot golfe",
            "alpha bravo charlie delta echos foxtrot golfe",
            "golfe",
        ];
        let data = rows(rejected_with_titles(&titles));
        let report = SourcingYield::compute(&data, &[rules(&agent)]);
        assert_eq!(
            exclusions(&report),
            ["golfe", "alpha", "bravo", "charlie", "delta"]
        );
        assert_eq!(report.suggestions[0].rejected, 4);
        assert_eq!(MAX_EXCLUSION_SUGGESTIONS_PER_AGENT, 5);
    }

    #[test]
    fn a_word_of_a_keyword_phrase_is_not_suggested() {
        let jobs = search("jobs", &["single sign-on", "mfa"], &[]);
        let data =
            rejected_with_titles(&["Single sign-on A", "Single sign-on B", "Single sign-on C"]);
        let report = SourcingYield::compute(&rows(data), &[rules(&jobs)]);
        assert!(!exclusions(&report).contains(&"single"));
    }

    #[test]
    fn a_number_is_not_suggested() {
        let jobs = search("jobs", &["sso", "mfa"], &[]);
        let data = rejected_with_titles(&["Appel 2024 A", "Appel 2024 B", "Appel 2024 C"]);
        let report = SourcingYield::compute(&rows(data), &[rules(&jobs)]);
        assert_eq!(exclusions(&report), ["appel"]);
    }

    #[test]
    fn exclusions_are_computed_per_agent() {
        let jobs = search("jobs", &["sso", "mfa"], &[]);
        let tenders = search("tenders", &["oidc", "saml"], &[]);
        let mut data = rejected_with_titles(&["Stage A", "Stage B"]);
        data.push(
            row(900, &["oidc"])
                .agent("tenders")
                .title("Stage C")
                .rejected(),
        );
        let report = SourcingYield::compute(&rows(data), &[rules(&jobs), rules(&tenders)]);
        assert!(exclusions(&report).is_empty());
    }

    #[test]
    fn suggestions_come_ordered_by_profile_agent_and_action() {
        let jobs = search("jobs", &["sso", "mfa"], &[]);
        let mut data = decided_keyword("sso", 0, 5);
        data.extend(rejected_with_titles(&["Stage A", "Stage B", "Stage C"]));
        let report = SourcingYield::compute(&rows(data), &[rules(&jobs)]);
        let actions: Vec<SuggestionAction> = report
            .suggestions
            .iter()
            .map(|suggestion| suggestion.action)
            .collect();
        assert_eq!(
            actions,
            [
                SuggestionAction::RemoveKeyword,
                SuggestionAction::AddExclusion
            ]
        );
    }

    #[test]
    fn the_action_round_trips_through_its_name() {
        for action in [
            SuggestionAction::RemoveKeyword,
            SuggestionAction::AddExclusion,
        ] {
            assert_eq!(SuggestionAction::parse(action.as_str()), Some(action));
        }
        assert_eq!(SuggestionAction::parse("rename"), None);
    }

    fn definition_with(agent: Agent) -> ProfileDefinition {
        let mut definition = ValidatedProfile::parse(serde_json::from_str(IAM).unwrap())
            .unwrap()
            .definition()
            .clone();
        definition.agents = vec![agent];
        definition
    }

    fn edit(action: SuggestionAction, word: &str) -> SuggestionEdit {
        SuggestionEdit {
            agent_id: "jobs".to_owned(),
            action,
            word: word.to_owned(),
        }
    }

    fn searched(definition: &ProfileDefinition) -> &SearchAgent {
        definition.agents[0].as_search().unwrap()
    }

    #[test]
    fn removing_a_keyword_edits_only_that_agent() {
        let definition = definition_with(search("jobs", &["sso", "mfa"], &["stage"]));
        let edited = edit(SuggestionAction::RemoveKeyword, "sso")
            .apply(&definition)
            .unwrap();
        assert_eq!(searched(&edited).keywords, ["mfa"]);
        assert_eq!(searched(&edited).exclusions, ["stage"]);
        assert_eq!(searched(&definition).keywords, ["sso", "mfa"]);
    }

    #[test]
    fn removing_a_keyword_is_exact() {
        let definition = definition_with(search("jobs", &["SSO", "mfa"], &[]));
        assert_eq!(
            edit(SuggestionAction::RemoveKeyword, "sso").apply(&definition),
            Err(EditProblem::KeywordMissing)
        );
    }

    #[test]
    fn the_last_keyword_cannot_be_removed() {
        let definition = definition_with(search("jobs", &["sso"], &[]));
        assert_eq!(
            edit(SuggestionAction::RemoveKeyword, "sso").apply(&definition),
            Err(EditProblem::LastKeyword)
        );
    }

    #[test]
    fn adding_an_exclusion_appends_it() {
        let definition = definition_with(search("jobs", &["sso"], &["stage"]));
        let edited = edit(SuggestionAction::AddExclusion, "alternance")
            .apply(&definition)
            .unwrap();
        assert_eq!(searched(&edited).exclusions, ["stage", "alternance"]);
        assert_eq!(searched(&edited).keywords, ["sso"]);
    }

    #[test]
    fn an_exclusion_already_present_is_a_conflict_whatever_its_accents() {
        let definition = definition_with(search("jobs", &["sso"], &["Stagé"]));
        assert_eq!(
            edit(SuggestionAction::AddExclusion, "stage").apply(&definition),
            Err(EditProblem::ExclusionPresent)
        );
    }

    #[test]
    fn an_empty_exclusion_is_refused() {
        let definition = definition_with(search("jobs", &["sso"], &[]));
        assert_eq!(
            edit(SuggestionAction::AddExclusion, "  ").apply(&definition),
            Err(EditProblem::EmptyWord)
        );
    }

    #[test]
    fn an_unknown_agent_cannot_be_edited() {
        let definition = definition_with(search("other", &["sso", "mfa"], &[]));
        assert_eq!(
            edit(SuggestionAction::RemoveKeyword, "sso").apply(&definition),
            Err(EditProblem::UnknownAgent)
        );
    }
}
