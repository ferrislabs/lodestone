use super::*;

#[test]
fn entities_are_decoded_once() {
    assert_eq!(
        decode_entities("R&amp;D &#x2F; &#39;ops&#39; &lt;b&gt; &amp;lt; &unknown; &"),
        "R&D / 'ops' <b> &lt; &unknown; &"
    );
}

#[test]
fn html_becomes_lines_and_anchors_keep_their_full_href() {
    let html = "Acme | Engineer | REMOTE<p>We build things &amp; more.<br>Apply: <a href=\"https:&#x2F;&#x2F;acme.com&#x2F;jobs\" rel=\"nofollow\">https:&#x2F;&#x2F;acme.com&#x2F;jo...</a><p><i>Stack</i>: Rust";
    assert_eq!(
        plain_text(html),
        "Acme | Engineer | REMOTE\nWe build things & more.\nApply: https://acme.com/jobs\nStack: Rust"
    );
}

#[test]
fn a_named_anchor_keeps_its_text_and_href() {
    let text = plain_text("See <a href=\"https://acme.com/x\">our careers</a> page");
    assert_eq!(text, "See our careers (https://acme.com/x) page");
}

#[test]
fn the_first_line_splits_into_company_role_and_location() {
    let parsed = parse("Acme Corp | Senior Rust Engineer | Berlin, Germany | Full-time | ONSITE<p>Details https://acme.com").unwrap();
    assert_eq!(parsed.company, "Acme Corp");
    assert_eq!(parsed.role.as_deref(), Some("Senior Rust Engineer"));
    assert_eq!(parsed.location, ["Berlin, Germany"]);
    assert_eq!(parsed.work_mode, WorkMode::OnSite);
    assert_eq!(parsed.contracts, [ContractHint::FullTime]);
    assert_eq!(parsed.title(), "Senior Rust Engineer - Acme Corp");
}

#[test]
fn a_header_without_separator_is_a_company_only() {
    let parsed = parse("Acme is hiring engineers<p>https://acme.io").unwrap();
    assert_eq!(parsed.company, "Acme is hiring engineers");
    assert_eq!(parsed.role, None);
    assert_eq!(parsed.title(), "Acme is hiring engineers");
}

#[test]
fn work_modes_are_read_from_the_header_tokens() {
    let mode = |header: &str| parse(header).unwrap().work_mode;
    assert_eq!(mode("A | Dev | REMOTE"), WorkMode::FullRemote);
    assert_eq!(mode("A | Dev | Remote (US only)"), WorkMode::FullRemote);
    assert_eq!(mode("A | Dev | ONSITE | Paris"), WorkMode::OnSite);
    assert_eq!(mode("A | Dev | On-site"), WorkMode::OnSite);
    assert_eq!(mode("A | Dev | Hybrid | Paris"), WorkMode::Hybrid);
    assert_eq!(mode("A | Dev | REMOTE or ONSITE"), WorkMode::Hybrid);
    assert_eq!(mode("A | Dev | Remote-friendly"), WorkMode::Hybrid);
    assert_eq!(mode("A | Dev | No remote | Paris"), WorkMode::OnSite);
    assert_eq!(mode("A | Dev | Paris"), WorkMode::Unknown);
    assert_eq!(mode("Remote.com | Dev | Paris"), WorkMode::Unknown);
}

#[test]
fn pure_mode_tokens_are_not_locations_but_mixed_ones_are() {
    let parsed =
        parse("A | Dev | REMOTE (US only) | Contract | San Francisco (REMOTE ok)").unwrap();
    assert_eq!(parsed.role.as_deref(), Some("Dev"));
    assert_eq!(parsed.location, ["San Francisco (REMOTE ok)"]);
}

#[test]
fn contract_hints_are_read_from_the_header() {
    let hints = |header: &str| parse(header).unwrap().contracts;
    assert_eq!(hints("A | Dev | Contract"), [ContractHint::Freelance]);
    assert_eq!(hints("A | Dev | Freelance"), [ContractHint::Freelance]);
    assert_eq!(hints("A | Dev | Part-time"), [ContractHint::PartTime]);
    assert_eq!(
        hints("A | Dev | Full time | contractor"),
        [ContractHint::Freelance, ContractHint::FullTime]
    );
    assert!(hints("A | Dev | Paris").is_empty());
}

#[test]
fn country_hints_come_from_unambiguous_codes() {
    assert_eq!(parse("A | Dev | NYC, US").unwrap().country, Some("USA"));
    assert_eq!(parse("A | Dev | London, UK").unwrap().country, Some("GBR"));
    assert_eq!(
        parse("A | Dev | Austin, U.S.").unwrap().country,
        Some("USA")
    );
    assert_eq!(parse("A | Dev | Sacramento, CA").unwrap().country, None);
    assert_eq!(parse("A | Dev | Paris").unwrap().country, None);
}

#[test]
fn the_domain_is_the_first_url_that_is_not_hn_or_a_job_board() {
    let html = "Acme | Dev | REMOTE<p>Discussed at <a href=\"https://news.ycombinator.com/item?id=1\">hn</a>, apply on https://jobs.lever.co/acme and https://www.linkedin.com/company/acme or https://github.com/acme, product https://www.acme.com/about, blog https://other.io";
    assert_eq!(parse(html).unwrap().domain.unwrap().as_str(), "acme.com");
}

#[test]
fn urls_are_found_in_headers_without_spaces_and_bare_www() {
    assert_eq!(
        parse("Acme|Dev|https://acme.io/jobs")
            .unwrap()
            .domain
            .unwrap()
            .as_str(),
        "acme.io"
    );
    assert_eq!(
        parse("Acme | Dev<p>Visit (www.acme.fr), thanks")
            .unwrap()
            .domain
            .unwrap()
            .as_str(),
        "acme.fr"
    );
}

#[test]
fn no_acceptable_url_means_no_domain() {
    assert!(
        parse("Acme | Dev | REMOTE<p>Email jobs@acme.com")
            .unwrap()
            .domain
            .is_none()
    );
    assert!(
        parse("Acme | Dev<p>https://news.ycombinator.com/item?id=1 https://indeed.com/x")
            .unwrap()
            .domain
            .is_none()
    );
}

#[test]
fn hints_prefix_the_text_for_the_shared_filters() {
    let parsed = parse("Acme | Dev | ONSITE | Contract<p>Body line").unwrap();
    let text = parsed.text(4000);
    assert!(text.starts_with("On-site only, no remote.\nFreelance contract.\nAcme | Dev"));
    assert!(text.ends_with("Body line"));
    assert_eq!(parsed.text(5).chars().count(), 5);
}

#[test]
fn an_empty_comment_is_not_parsed() {
    assert!(parse("").is_none());
    assert!(parse("<p> </p>").is_none());
}
