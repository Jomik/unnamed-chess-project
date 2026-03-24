fn main() {
    // Only run embuild when targeting ESP-IDF
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "espidf" {
        embuild::espidf::sysenv::output();
    }

    // Lichess config: emit defaults for optional env vars so env!() works at compile time.
    // If the user sets these in .env, their values take precedence.
    emit_default_env("LICHESS_AI_LEVEL", "4");
    emit_default_env("LICHESS_CLOCK_LIMIT", "600");
    emit_default_env("LICHESS_CLOCK_INCREMENT", "0");
}

fn emit_default_env(key: &str, default: &str) {
    if std::env::var(key).is_err() {
        println!("cargo:rustc-env={key}={default}");
    }

    // Validate at build time
    let value = std::env::var(key).unwrap_or_else(|_| default.to_string());
    match key {
        "LICHESS_AI_LEVEL" => {
            let v: u8 = value
                .parse()
                .unwrap_or_else(|_| panic!("{key}={value} is not a valid u8"));
            assert!(
                (1..=8).contains(&v),
                "{key}={v} is out of range (must be 1-8)"
            );
        }
        "LICHESS_CLOCK_LIMIT" | "LICHESS_CLOCK_INCREMENT" => {
            let _: u32 = value
                .parse()
                .unwrap_or_else(|_| panic!("{key}={value} is not a valid u32"));
        }
        _ => {}
    }

    // Re-run build script if this env var changes
    println!("cargo:rerun-if-env-changed={key}");
}
