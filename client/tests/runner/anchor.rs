use tokio::process::Command;

pub async fn build_program() {
    let status = Command::new("anchor")
        .arg("build")
        .arg("-p")
        .arg("hyperplane")
        .status()
        .await
        .expect("Failed to build anchor program");

    if !status.success() {
        panic!("Failed to build anchor program");
    }
    println!("Anchor program built!");
}
