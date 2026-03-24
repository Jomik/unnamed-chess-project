fn main() {
    // Only run embuild when targeting ESP-IDF
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "espidf" {
        embuild::espidf::sysenv::output();
    }
}
