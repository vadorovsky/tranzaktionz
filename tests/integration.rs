use std::{
    ffi::OsStr,
    process::{Command, Output},
};

fn cli_output_for<P: AsRef<OsStr>>(file: P) -> Output {
    #[cfg(debug_assertions)]
    let mut cmd = Command::new("target/debug/tranzaktionz");
    #[cfg(not(debug_assertions))]
    let mut cmd = Command::new("target/release/tranzaktionz");

    cmd.arg(file).output().expect("Failed to execute CLI")
}

#[test]
fn test_cli() {
    let output1 = cli_output_for("tests/example1.csv");
    assert_eq!(
        String::from_utf8_lossy(&output1.stdout),
        "\
client,available,held,total,locked
1,1.5,0,1.5,false
2,2.0,0,2.0,false
"
    );

    let output2 = cli_output_for("tests/example2.csv");
    assert_eq!(
        String::from_utf8_lossy(&output2.stdout),
        "\
client,available,held,total,locked
1,1.5,0.0,1.5,false
2,0.0,0.0,0.0,true
"
    );
}
