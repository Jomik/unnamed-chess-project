fn main() {
    // Only run embuild when targeting ESP32
    #[cfg(target_arch = "xtensa")]
    {
        embuild::espidf::sysenv::output();
    }
}
