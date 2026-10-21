use crate::domain::{
    company::OfficialData,
    lead::LeadId,
    profile::definition::{NafPrefix, SectorFilter, TargetCompanies},
};

pub const TARGET_REASON_PREFIX: &str = "hors cible";
pub const CLOSED_COMPANY_REASON: &str = "entreprise fermée";

const CORSICA_SOUTH: &str = "2A";
const CORSICA_NORTH: &str = "2B";

#[derive(Debug, Clone, PartialEq)]
pub struct TargetedLead {
    pub lead_id: LeadId,
    pub exclusion: Option<String>,
    pub official: OfficialData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ruling {
    Keep,
    Exclude(String),
    Restore(String),
    Replace { stored: String, reason: String },
}

pub fn is_target_reason(reason: &str) -> bool {
    reason.starts_with(TARGET_REASON_PREFIX) || reason == CLOSED_COMPANY_REASON
}

pub fn ruling(rules: &TargetCompanies, stored: Option<&str>, official: &OfficialData) -> Ruling {
    match (stored, target_exclusion(rules, official)) {
        (None, Some(reason)) => Ruling::Exclude(reason),
        (Some(stored), None) if is_target_reason(stored) => Ruling::Restore(stored.to_owned()),
        (Some(stored), Some(reason)) if is_target_reason(stored) && stored != reason => {
            Ruling::Replace {
                stored: stored.to_owned(),
                reason,
            }
        }
        _ => Ruling::Keep,
    }
}

pub fn target_exclusion(rules: &TargetCompanies, official: &OfficialData) -> Option<String> {
    if official.active == Some(false) {
        return Some(CLOSED_COMPANY_REASON.to_owned());
    }
    if let Some(seen) = outside_headcount(rules, official) {
        return Some(format!("{TARGET_REASON_PREFIX} : effectif {seen}"));
    }
    if let Some(category) = unlisted_category(rules, official) {
        return Some(format!("{TARGET_REASON_PREFIX} : catégorie {category}"));
    }
    match (rules.sector, is_public(official)) {
        (SectorFilter::Public, Some(false)) => {
            return Some(format!("{TARGET_REASON_PREFIX} : secteur privé"));
        }
        (SectorFilter::Private, Some(true)) => {
            return Some(format!("{TARGET_REASON_PREFIX} : secteur public"));
        }
        _ => {}
    }
    if let Some(code) = unlisted_activity(rules, official) {
        return Some(format!("{TARGET_REASON_PREFIX} : activité {code}"));
    }
    let department = unlisted_department(rules, official)?;
    Some(format!("{TARGET_REASON_PREFIX} : département {department}"))
}

pub fn department_of(official: &OfficialData) -> Option<&str> {
    if is_foreign(official) {
        return None;
    }
    let declared = official
        .head_office_department
        .as_deref()
        .map(str::trim)
        .filter(|department| !department.is_empty());
    declared.or_else(|| {
        official
            .head_office_postcode
            .as_deref()
            .and_then(postcode_department)
    })
}

fn headcount_band(code: &str) -> Option<(u32, Option<u32>)> {
    Some(match code {
        "NN" | "00" => (0, Some(0)),
        "01" => (1, Some(2)),
        "02" => (3, Some(5)),
        "03" => (6, Some(9)),
        "11" => (10, Some(19)),
        "12" => (20, Some(49)),
        "21" => (50, Some(99)),
        "22" => (100, Some(199)),
        "31" => (200, Some(249)),
        "32" => (250, Some(499)),
        "41" => (500, Some(999)),
        "42" => (1_000, Some(1_999)),
        "51" => (2_000, Some(4_999)),
        "52" => (5_000, Some(9_999)),
        "53" => (10_000, None),
        _ => return None,
    })
}

fn outside_headcount<'a>(rules: &TargetCompanies, official: &'a OfficialData) -> Option<&'a str> {
    let code = official.headcount_code.as_deref()?.trim();
    let (low, high) = headcount_band(code)?;
    let below = rules
        .min_employees
        .is_some_and(|min| high.is_some_and(|high| high < min));
    let above = rules.max_employees.is_some_and(|max| low > max);
    (below || above).then(|| official.headcount_range.as_deref().unwrap_or(code))
}

fn unlisted_category<'a>(rules: &TargetCompanies, official: &'a OfficialData) -> Option<&'a str> {
    if rules.categories.is_empty() {
        return None;
    }
    let category = official
        .category
        .as_deref()
        .map(str::trim)
        .filter(|category| !category.is_empty())?;
    let listed = rules
        .categories
        .iter()
        .any(|wanted| wanted.as_str().eq_ignore_ascii_case(category));
    (!listed).then_some(category)
}

fn is_public(official: &OfficialData) -> Option<bool> {
    let first = official.legal_nature.as_deref()?.trim().chars().next()?;
    first.is_ascii_digit().then_some(matches!(first, '4' | '7'))
}

fn unlisted_activity<'a>(rules: &TargetCompanies, official: &'a OfficialData) -> Option<&'a str> {
    let code = official
        .naf_code
        .as_deref()
        .map(str::trim)
        .filter(|code| !code.is_empty())?;
    let excluded = starts_with_any(code, &rules.naf_exclude);
    let missed = !rules.naf_include.is_empty() && !starts_with_any(code, &rules.naf_include);
    (excluded || missed).then_some(code)
}

fn starts_with_any(code: &str, prefixes: &[NafPrefix]) -> bool {
    prefixes
        .iter()
        .any(|prefix| code.starts_with(prefix.as_str()))
}

fn unlisted_department<'a>(rules: &TargetCompanies, official: &'a OfficialData) -> Option<&'a str> {
    if rules.departments.is_empty() {
        return None;
    }
    let department = department_of(official)?;
    let listed = rules
        .departments
        .iter()
        .any(|wanted| wanted.as_str().eq_ignore_ascii_case(department));
    (!listed).then_some(department)
}

fn is_foreign(official: &OfficialData) -> bool {
    match official.head_office_country_code.as_deref() {
        Some(code) => !code.trim().eq_ignore_ascii_case("FR"),
        None => official
            .head_office_country
            .as_deref()
            .map(str::trim)
            .is_some_and(|country| !country.is_empty() && !country.eq_ignore_ascii_case("France")),
    }
}

fn postcode_department(postcode: &str) -> Option<&str> {
    let postcode = postcode.trim();
    if postcode.len() != 5 || !postcode.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if postcode.starts_with("97") {
        return Some(&postcode[..3]);
    }
    match &postcode[..3] {
        "200" | "201" => Some(CORSICA_SOUTH),
        "202" | "203" | "204" | "205" | "206" => Some(CORSICA_NORTH),
        _ => Some(&postcode[..2]),
    }
}

#[cfg(test)]
mod tests;
