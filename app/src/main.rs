fn main() {
    let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".config/lg-remote");
    std::fs::create_dir_all(&dir).unwrap();
    lg_app::run(dir);
}
