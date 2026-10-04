use x509_parser::certificate::{TbsCertificate, X509Certificate};
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::FromDer;

pub(crate) fn names_from_certificate(der: &[u8]) -> Vec<String> {
    match X509Certificate::from_der(der) {
        Ok((_, certificate)) => collect(&certificate.tbs_certificate),
        Err(_) => Vec::new(),
    }
}

pub(crate) fn names_from_tbs(der: &[u8]) -> Vec<String> {
    match TbsCertificate::from_der(der) {
        Ok((_, tbs)) => collect(&tbs),
        Err(_) => Vec::new(),
    }
}

fn collect(tbs: &TbsCertificate<'_>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(Some(san)) = tbs.subject_alternative_name() {
        for general in &san.value.general_names {
            if let GeneralName::DNSName(name) = general {
                push_unique(&mut names, name);
            }
        }
    }
    for common in tbs.subject.iter_common_name() {
        if let Ok(value) = common.as_str()
            && looks_like_hostname(value)
        {
            push_unique(&mut names, value);
        }
    }
    names
}

fn push_unique(names: &mut Vec<String>, raw: &str) {
    let lowered = raw.trim().to_ascii_lowercase();
    if !lowered.is_empty() && !names.contains(&lowered) {
        names.push(lowered);
    }
}

fn looks_like_hostname(value: &str) -> bool {
    let value = value.trim();
    value.contains('.')
        && value.len() <= 253
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'*' | b'_'))
}
