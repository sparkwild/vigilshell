fn main() {
    let schema = vigilshell_desktop_protocol::contract_schema();
    serde_json::to_writer_pretty(std::io::stdout().lock(), &schema).unwrap();
    println!();
}
