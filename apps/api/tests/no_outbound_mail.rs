use std::{
    fs,
    path::{Path, PathBuf},
};

const MAIL_SENDING_CRATES: [&str; 12] = [
    "lettre",
    "mail-send",
    "async-smtp",
    "samotop",
    "sendgrid",
    "postmark",
    "aws-sdk-ses",
    "aws-sdk-sesv2",
    "rusoto_ses",
    "sendmail",
    "mandrill",
    "sparkpost",
];

const MAIL_SENDING_PREFIXES: [&str; 3] = ["mailgun", "resend", "smtp"];

const SOURCE_ROOTS: [&str; 2] = ["libs", "apps"];

const SMTP_PORTS: [&str; 3] = ["25", "465", "587"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn is_mail_sending_crate(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    MAIL_SENDING_CRATES.contains(&name.as_str())
        || MAIL_SENDING_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

fn walk(dir: &Path, wanted: &dyn Fn(&Path) -> bool, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == "target" || name == "node_modules" || name == ".git" || name == "tests" {
            continue;
        }
        if path.is_dir() {
            walk(&path, wanted, found);
        } else if wanted(&path) {
            found.push(path);
        }
    }
}

fn quoted_strings(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

fn lock_packages(lock: &str) -> Vec<&str> {
    lock.lines()
        .filter_map(|line| line.strip_prefix("name = "))
        .flat_map(quoted_strings)
        .collect()
}

fn manifest_mail_crates(manifest: &str) -> Vec<String> {
    let mut offenders = Vec::new();
    for line in manifest.lines().map(str::trim) {
        let key = line
            .split('=')
            .next()
            .unwrap_or_default()
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']');
        let section_leaf = key.rsplit('.').next().unwrap_or(key);
        if is_mail_sending_crate(section_leaf) {
            offenders.push(section_leaf.to_owned());
        }
        for text in quoted_strings(line) {
            if line.contains("package") && is_mail_sending_crate(text) {
                offenders.push(text.to_owned());
            }
        }
    }
    offenders
}

fn mentions_port(line: &str, port: &str) -> bool {
    let needle = format!(":{port}");
    line.match_indices(&needle).any(|(at, _)| {
        !line[at + needle.len()..]
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_alphanumeric())
    })
}

fn mail_channel_mentions(source: &str) -> Vec<String> {
    let mut mentions = Vec::new();
    for (number, line) in source.lines().enumerate() {
        let lowered = line.to_ascii_lowercase();
        let smtp_port = SMTP_PORTS.iter().any(|port| mentions_port(&lowered, port))
            && (lowered.contains("connect") || lowered.contains("tcp") || lowered.contains("mail"));
        if lowered.contains("smtp") || lowered.contains("sendmail") || smtp_port {
            mentions.push(format!("{}: {}", number + 1, line.trim()));
        }
    }
    mentions
}

#[test]
fn cargo_lock_resolves_no_mail_sending_crate() {
    let lock = fs::read_to_string(workspace_root().join("Cargo.lock")).expect("Cargo.lock");
    let packages = lock_packages(&lock);
    assert!(packages.len() > 100, "Cargo.lock looks unparsed");

    let offenders: Vec<_> = packages
        .into_iter()
        .filter(|name| is_mail_sending_crate(name))
        .collect();

    assert!(
        offenders.is_empty(),
        "mail-sending crates in Cargo.lock: {offenders:?}"
    );
}

#[test]
fn no_manifest_declares_a_mail_sending_crate() {
    let mut manifests = Vec::new();
    let root = workspace_root();
    walk(
        &root,
        &|path| path.file_name().is_some_and(|name| name == "Cargo.toml"),
        &mut manifests,
    );
    assert!(manifests.len() > 5, "workspace manifests not found");

    for manifest in manifests {
        let text = fs::read_to_string(&manifest).expect("manifest");
        let offenders = manifest_mail_crates(&text);
        assert!(
            offenders.is_empty(),
            "{} declares mail-sending crates: {offenders:?}",
            manifest.display()
        );
    }
}

#[test]
fn no_source_file_opens_an_smtp_or_sendmail_channel() {
    let root = workspace_root();
    let mut files = Vec::new();
    for source_root in SOURCE_ROOTS {
        walk(
            &root.join(source_root),
            &|path| path.extension().is_some_and(|ext| ext == "rs"),
            &mut files,
        );
    }
    assert!(files.len() > 50, "workspace sources not found");

    for file in files {
        let text = fs::read_to_string(&file).expect("source");
        let mentions = mail_channel_mentions(&text);
        assert!(
            mentions.is_empty(),
            "{} refers to a mail channel: {mentions:?}",
            file.display()
        );
    }
}

#[test]
fn detector_flags_known_mail_senders_and_ignores_plain_crates() {
    for name in [
        "lettre",
        "Mail-Send",
        "mailgun-rs",
        "resend-rs",
        "smtp-client",
        "aws-sdk-ses",
    ] {
        assert!(is_mail_sending_crate(name), "{name}");
    }
    for name in ["serde", "mail-builder", "email_address", "axum"] {
        assert!(!is_mail_sending_crate(name), "{name}");
    }
    assert_eq!(
        manifest_mail_crates(
            "[dependencies]\nlettre = \"0.11\"\nmailer = { package = \"mail-send\", version = \"1\" }"
        ),
        ["lettre", "mail-send"]
    );
    assert!(manifest_mail_crates("[dependencies]\nserde = \"1\"").is_empty());
    assert_eq!(
        mail_channel_mentions("let s = \"smtp.example.com\";").len(),
        1
    );
    assert_eq!(
        mail_channel_mentions("tcp_connect(host, \"mail:587\")").len(),
        1
    );
    assert!(mail_channel_mentions("let url = \"http://host:2500/path\";").is_empty());
}
