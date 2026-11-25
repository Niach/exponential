#[cfg(not(target_os = "linux"))]
fn main() {}
#[cfg(target_os = "linux")]
fn main() {
    member_space::run_if_requested();
    println!("{:?}", member_space::probe(&std::env::current_exe().unwrap()));
}
