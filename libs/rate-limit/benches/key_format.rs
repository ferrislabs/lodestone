use rate_limit::RateLimitKey;

fn main() {
    divan::main();
}

#[divan::bench]
fn ip_v4_key(bencher: divan::Bencher) {
    bencher.bench(|| {
        let key = RateLimitKey::ip(divan::black_box("203.0.113.42"));
        divan::black_box(key.to_string())
    });
}

#[divan::bench]
fn ip_v6_key(bencher: divan::Bencher) {
    bencher.bench(|| {
        let key = RateLimitKey::ip(divan::black_box("2001:db8::1"));
        divan::black_box(key.to_string())
    });
}
