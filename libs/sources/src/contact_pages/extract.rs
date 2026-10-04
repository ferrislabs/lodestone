use std::sync::LazyLock;

use lodestone_core::EmailAddress;
use regex::{Captures, Regex};

const MAILTO_CONFIDENCE: f64 = 0.9;
const TEXT_CONFIDENCE: f64 = 0.8;
const CLOUDFLARE_CONFIDENCE: f64 = 0.8;
const OBFUSCATED_CONFIDENCE: f64 = 0.6;

const FILE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "svg", "webp", "css", "js", "ico", "bmp", "avif", "woff", "woff2",
    "ttf", "map",
];
const PLACEHOLDER_DOMAINS: &[&str] = &[
    "example.com",
    "example.org",
    "example.net",
    "domain.com",
    "email.com",
    "sentry.io",
    "wixpress.com",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Extracted {
    pub address: EmailAddress,
    pub confidence: f64,
}

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => unreachable!("static pattern {pattern} is invalid: {error}"),
    }
}

static INERT: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?is)<!--.*?-->|<script\b.*?</script\s*>|<style\b.*?</style\s*>"));
static MAILTO: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"(?i)href\s*=\s*["']\s*mailto:([^"']*)["']"#));
static CF_ATTRIBUTE: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"(?i)data-cfemail\s*=\s*["']([0-9a-f]+)["']"#));
static CF_HREF: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?i)/cdn-cgi/l/email-protection#([0-9a-f]+)"));
static TAG: LazyLock<Regex> = LazyLock::new(|| compile(r"<[^>]*>"));
static ENTITY: LazyLock<Regex> =
    LazyLock::new(|| compile(r"&(#[xX][0-9a-fA-F]{1,6}|#[0-9]{1,7}|[A-Za-z][A-Za-z0-9]{1,15});"));
static AT_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?i)\s*[\[\(\{]\s*(?:at|arobase)\s*[\]\)\}]\s*"));
static DOT_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?i)\s*[\[\(\{]\s*(?:dot|point)\s*[\]\)\}]\s*"));
static PLAIN: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+)*\.[A-Za-z]{2,}")
});
static SPACED: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"([A-Za-z0-9._%+\-]+)[ \t]*@[ \t]*([A-Za-z0-9\-]+(?:(?:\.|[ \t]+\.[ \t]+)[A-Za-z0-9\-]+)*(?:\.|[ \t]+\.[ \t]+)[A-Za-z]{2,})",
    )
});

fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        "commat" => '@',
        "period" => '.',
        "amp" => '&',
        "plus" => '+',
        "lowbar" => '_',
        "hyphen" | "dash" => '-',
        "percnt" => '%',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" | "NonBreakingSpace" => ' ',
        "lsqb" | "lbrack" => '[',
        "rsqb" | "rbrack" => ']',
        "lpar" => '(',
        "rpar" => ')',
        _ => return None,
    })
}

pub fn decode_entities(text: &str) -> String {
    ENTITY
        .replace_all(text, |caps: &Captures<'_>| {
            let body = &caps[1];
            let decoded = match body.strip_prefix('#') {
                Some(number) => match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok().and_then(char::from_u32),
                    None => number.parse::<u32>().ok().and_then(char::from_u32),
                },
                None => named_entity(body),
            };
            match decoded {
                Some(character) => character.to_string(),
                None => caps[0].to_owned(),
            }
        })
        .into_owned()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = bytes
            .get(index + 1..index + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[index], hex) {
            (b'%', Some(value)) => {
                out.push(value);
                index += 3;
            }
            (byte, _) => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn decode_cloudflare(hex: &str) -> Option<String> {
    if hex.len() < 4 || !hex.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).ok())
        .collect::<Option<_>>()?;
    let (key, rest) = bytes.split_first()?;
    let decoded: Vec<u8> = rest.iter().map(|byte| byte ^ key).collect();
    String::from_utf8(decoded).ok()
}

fn is_plausible(address: &EmailAddress) -> bool {
    let domain = address.domain();
    let tld = domain.rsplit('.').next().unwrap_or("");
    if FILE_EXTENSIONS.contains(&tld) || domain.contains("yourdomain") {
        return false;
    }
    !PLACEHOLDER_DOMAINS.iter().any(|placeholder| {
        domain == *placeholder
            || domain
                .strip_suffix(placeholder)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

#[derive(Default)]
struct Collector {
    found: Vec<Extracted>,
}

impl Collector {
    fn add(&mut self, raw: &str, confidence: f64) {
        let trimmed = raw.trim().trim_start_matches(['.', '-']);
        let Ok(address) = EmailAddress::parse(trimmed) else {
            return;
        };
        if !is_plausible(&address) {
            return;
        }
        match self.found.iter_mut().find(|item| item.address == address) {
            Some(existing) if existing.confidence < confidence => existing.confidence = confidence,
            Some(_) => {}
            None => self.found.push(Extracted {
                address,
                confidence,
            }),
        }
    }
}

pub fn extract_addresses(html: &str) -> Vec<Extracted> {
    let live = INERT.replace_all(html, " ");
    let mut collector = Collector::default();

    for caps in MAILTO.captures_iter(&live) {
        let decoded = decode_entities(&caps[1]);
        let target = decoded.split('?').next().unwrap_or("");
        for part in percent_decode(target).split(',') {
            collector.add(part, MAILTO_CONFIDENCE);
        }
    }

    for caps in CF_ATTRIBUTE
        .captures_iter(&live)
        .chain(CF_HREF.captures_iter(&live))
    {
        if let Some(decoded) = decode_cloudflare(&caps[1]) {
            collector.add(&decoded, CLOUDFLARE_CONFIDENCE);
        }
    }

    let text = decode_entities(&TAG.replace_all(&live, " "));
    for found in PLAIN.find_iter(&text) {
        collector.add(found.as_str(), TEXT_CONFIDENCE);
    }

    let tokenized = AT_TOKEN.replace_all(&text, "@");
    let tokenized = DOT_TOKEN.replace_all(&tokenized, ".");
    for caps in SPACED.captures_iter(&tokenized) {
        let domain: String = caps[2].split_whitespace().collect();
        collector.add(&format!("{}@{domain}", &caps[1]), OBFUSCATED_CONFIDENCE);
    }

    collector.found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(html: &str) -> Vec<(String, f64)> {
        extract_addresses(html)
            .into_iter()
            .map(|item| (item.address.to_string(), item.confidence))
            .collect()
    }

    fn only(html: &str) -> (String, f64) {
        let mut all = found(html);
        assert_eq!(all.len(), 1, "{all:?}");
        all.remove(0)
    }

    fn encode_cloudflare(key: u8, address: &str) -> String {
        let mut out = format!("{key:02x}");
        for byte in address.bytes() {
            out.push_str(&format!("{:02x}", byte ^ key));
        }
        out
    }

    #[test]
    fn mailto_link_is_most_confident() {
        assert_eq!(
            only(r#"<a href="mailto:Contact@Acme.fr">write</a>"#),
            ("contact@acme.fr".to_owned(), 0.9)
        );
    }

    #[test]
    fn mailto_strips_query_and_percent_decodes() {
        assert_eq!(
            only(r#"<a href="mailto:jean%2Bpro@acme.fr?subject=Hello%20you&amp;body=x">w</a>"#).0,
            "jean+pro@acme.fr"
        );
        assert_eq!(
            only(r#"<a href='MAILTO:%73ales@acme.fr'>w</a>"#).0,
            "sales@acme.fr"
        );
    }

    #[test]
    fn mailto_with_several_recipients() {
        let all = found(r#"<a href="mailto:a@acme.fr,b@acme.fr">w</a>"#);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn plain_text_address() {
        assert_eq!(
            only("<p>Write to <b>info@acme.fr</b>.</p>"),
            ("info@acme.fr".to_owned(), 0.8)
        );
    }

    #[test]
    fn decimal_and_hex_entities() {
        assert_eq!(only("<p>info&#64;acme&#46;fr</p>").0, "info@acme.fr");
        assert_eq!(only("<p>info&#x40;acme&#x2E;fr</p>").0, "info@acme.fr");
    }

    #[test]
    fn named_entities() {
        assert_eq!(only("<p>info&commat;acme&period;fr</p>").0, "info@acme.fr");
        assert_eq!(
            only(r#"<a href="mailto:a&#64;acme&period;fr">x</a>"#).0,
            "a@acme.fr"
        );
        assert_eq!(only("<p>a&plus;b&lowbar;c@acme.fr</p>").0, "a+b_c@acme.fr");
    }

    #[test]
    fn unknown_entities_are_left_alone() {
        assert_eq!(decode_entities("&bogus; &amp; &#xZZ;"), "&bogus; & &#xZZ;");
    }

    #[test]
    fn bracketed_at_and_dot() {
        assert_eq!(
            only("contact [at] acme-industrie [dot] fr"),
            ("contact@acme-industrie.fr".to_owned(), 0.6)
        );
    }

    #[test]
    fn parenthesized_at_and_dot() {
        assert_eq!(only("name(at)acme(dot)com").0, "name@acme.com");
    }

    #[test]
    fn french_variants() {
        assert_eq!(
            only("contact [arobase] acme [point] fr").0,
            "contact@acme.fr"
        );
        assert_eq!(
            only("contact (arobase) acme (point) fr").0,
            "contact@acme.fr"
        );
    }

    #[test]
    fn uppercase_tokens() {
        assert_eq!(only("Contact [AT] Acme [DOT] FR").0, "contact@acme.fr");
    }

    #[test]
    fn spaces_around_at_and_dot() {
        assert_eq!(
            only("contact @ acme . fr"),
            ("contact@acme.fr".to_owned(), 0.6)
        );
        assert_eq!(only("contact @ acme.fr").0, "contact@acme.fr");
    }

    #[test]
    fn spaced_form_does_not_swallow_sentence_end() {
        assert_eq!(
            only("Mail contact @ acme.fr. Next sentence.").0,
            "contact@acme.fr"
        );
    }

    #[test]
    fn bare_words_are_prose() {
        assert!(found("Please look at this page, it is a dot com story").is_empty());
        assert!(found("look at this.example, or meet at noon dot com").is_empty());
        assert!(found("meet (at) noon").is_empty());
    }

    #[test]
    fn cloudflare_attribute_is_decoded() {
        let encoded = encode_cloudflare(0x5a, "dpo@acme.fr");
        let html = format!(
            r#"<a href="/cdn-cgi/l/email-protection" class="__cf_email__" data-cfemail="{encoded}">[email&#160;protected]</a>"#
        );
        assert_eq!(only(&html), ("dpo@acme.fr".to_owned(), 0.8));
    }

    #[test]
    fn cloudflare_href_is_decoded() {
        let encoded = encode_cloudflare(0x01, "sales@acme.fr");
        let html = format!(r#"<a href="/cdn-cgi/l/email-protection#{encoded}">mail</a>"#);
        assert_eq!(only(&html).0, "sales@acme.fr");
    }

    #[test]
    fn cloudflare_garbage_is_ignored() {
        assert!(found(r#"<span data-cfemail="zz"></span><i data-cfemail="ab"></i>"#).is_empty());
        assert!(found(r#"<i data-cfemail="00ffff"></i>"#).is_empty());
    }

    #[test]
    fn scripts_styles_and_comments_are_ignored() {
        let html = r#"
            <script>var m = "hidden@acme.fr";</script>
            <style>.a{content:"css@acme.fr"}</style>
            <!-- old@acme.fr <a href="mailto:old2@acme.fr">x</a> -->
            <p>visible@acme.fr</p>"#;
        assert_eq!(only(html).0, "visible@acme.fr");
    }

    #[test]
    fn script_with_attributes_and_uppercase_is_ignored() {
        assert!(found(r#"<SCRIPT type="text/javascript">a@acme.fr</SCRIPT>"#).is_empty());
    }

    #[test]
    fn image_names_are_ignored() {
        assert!(
            found("<img src=\"logo@2x.png\"> banner@3x.jpg a@b.svg c@d.webp e@f.css g@h.js")
                .is_empty()
        );
    }

    #[test]
    fn placeholders_are_ignored() {
        let html = "you@example.com a@example.org b@domain.com c@email.com d@yourdomain.fr \
                    e@sentry.io f@o1.ingest.sentry.io g@wixpress.com";
        assert!(found(html).is_empty());
    }

    #[test]
    fn real_domain_ending_like_placeholder_is_kept() {
        assert_eq!(only("a@myexample.com").0, "a@myexample.com");
    }

    #[test]
    fn duplicates_merge_with_highest_confidence() {
        let html = r#"<a href="mailto:a@acme.fr">a@acme.fr</a> a [at] acme [dot] fr"#;
        assert_eq!(found(html), vec![("a@acme.fr".to_owned(), 0.9)]);
    }

    #[test]
    fn leading_punctuation_is_trimmed() {
        assert_eq!(only("(mail: -.a@acme.fr)").0, "a@acme.fr");
    }

    #[test]
    fn large_hostile_input_is_linear() {
        let html = "a".repeat(500_000) + &"@ . ".repeat(50_000);
        let _ = extract_addresses(&html);
    }
}
