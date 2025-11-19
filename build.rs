fn main() {
    // Only run embuild when targeting ESP-IDF
    #[cfg(target_os = "espidf")]
    {
        embuild::espidf::sysenv::output();
    }
}
